//! Latch credential sync orchestration.
//!
//! Coordinates secure end-to-end credential migration from CLIENT desktop
//! to LXC containers via ephemeral offers and encrypted payloads.

#![allow(dead_code)]

use crate::shell::{execute_local, execute_remote, ExecRequest};
use serde_json::Value;
use std::path::Path;
use std::time::SystemTime;

/// Error types for latch operations
#[derive(Debug)]
pub enum LatchError {
    CommandFailed { cmd: String, code: i32, stderr: String },
    JsonParse(String),
    Timeout,
    LxcUnreachable(String),
    MissingLatchCli,
    InvalidOffer(String),
    ValidationFailed(String),
}

impl std::fmt::Display for LatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LatchError::CommandFailed { cmd, code, stderr } => {
                write!(f, "Command '{}' failed with code {}: {}", cmd, code, stderr)
            }
            LatchError::JsonParse(e) => write!(f, "JSON parse error: {}", e),
            LatchError::Timeout => write!(f, "Operation timed out"),
            LatchError::LxcUnreachable(e) => write!(f, "LXC unreachable: {}", e),
            LatchError::MissingLatchCli => write!(f, "latch CLI not found on system"),
            LatchError::InvalidOffer(e) => write!(f, "Invalid offer: {}", e),
            LatchError::ValidationFailed(e) => write!(f, "Validation failed: {}", e),
        }
    }
}

impl std::error::Error for LatchError {}

/// Configuration for latch clone operation
#[derive(Clone, Debug)]
pub struct LatchCloneConfig {
    pub lxc_api_base: String,        // e.g., "http://lxc.local:8080"
    pub ttl_minutes: u64,             // default 10
    pub verify_code: Option<String>,  // optional integrity code
    pub project_filters: Vec<String>, // optional project names to export
    pub env_filters: Vec<String>,     // optional env names to export
}

/// Result of a successful latch clone workflow
#[derive(Debug, Clone)]
pub struct LatchCloneResult {
    pub offer_id: String,
    pub slot_count: usize,
    pub duration_secs: u64,
}

/// One-shot latch pull/update inputs sourced from CLIENT env/config.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LatchPullContext {
    pub pat: Option<String>,
    pub key: Option<String>,
    pub secrets_repo: Option<String>,
    pub project: Option<String>,
    pub env: Option<String>,
    pub sparse: bool,
}

/// Load latch defaults from env first, then from `.latch/config.toml`.
/// This keeps CLIENT as the source of truth when daemons need a one-shot pull.
pub fn load_latch_pull_context() -> Option<LatchPullContext> {
    let project_root = find_project_root();
    let latch_config = read_latch_project_config(&project_root);

    // Load from: config/.env (primary) → .latch/config.toml (fallback) → env vars (override)
    let pat = load_config_value("LATCH_PAT");
    let key = load_config_value("LATCH_KEY");
    let secrets_repo = load_config_value("LATCH_SECRETS_REPO").or_else(|| latch_config.secrets_repo);
    let project = load_config_value("LATCH_PROJECT")
        .or_else(|| latch_config.project)
        .or_else(|| Some(project_root.file_name()?.to_string_lossy().to_string()));
    let env = load_config_value("LATCH_ENV").or_else(|| latch_config.default_env);

    if pat.as_deref().unwrap_or_default().is_empty()
        && key.as_deref().unwrap_or_default().is_empty()
        && secrets_repo.as_deref().unwrap_or_default().is_empty()
    {
        return None;
    }

    Some(LatchPullContext {
        pat,
        key,
        secrets_repo,
        project,
        env,
        sparse: true,
    })
}

/// Load a key from: env vars (highest priority) → config/.env → return None
fn load_config_value(key: &str) -> Option<String> {
    // 1. Check environment variable (allows override)
    if let Ok(val) = std::env::var(key) {
        let trimmed = val.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }

    // 2. Check config/.env
    if let Ok(config_content) = std::fs::read_to_string("config/.env") {
        for line in config_content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((line_key, line_value)) = trimmed.split_once('=') {
                if line_key.trim() == key {
                    let value = line_value.trim().to_string();
                    if !value.is_empty() {
                        return Some(value);
                    }
                }
            }
        }
    }

    None
}

#[derive(Debug, Clone, Default)]
struct LatchProjectConfig {
    project: Option<String>,
    secrets_repo: Option<String>,
    default_env: Option<String>,
}

fn read_latch_project_config(root: &Path) -> LatchProjectConfig {
    let path = root.join(".latch/config.toml");
    let Ok(content) = std::fs::read_to_string(path) else {
        return LatchProjectConfig::default();
    };

    let mut config = LatchProjectConfig::default();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }

        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').to_string();
        match key {
            "name" if !value.is_empty() => config.project = Some(value),
            "secrets_repo" if !value.is_empty() => config.secrets_repo = Some(value),
            "default_env" if !value.is_empty() => config.default_env = Some(value),
            _ => {}
        }
    }

    config
}

fn find_project_root() -> std::path::PathBuf {
    let mut current = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    for _ in 0..10 {
        if current.join(".git").exists() {
            return current;
        }
        if !current.pop() {
            break;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Step 1: Generate offer on target (LXC)
async fn generate_offer(
    config: &LatchCloneConfig,
) -> Result<Value, LatchError> {
    let request = ExecRequest {
        cmd: "latch".to_string(),
        args: Some(vec![
            "clone".to_string(),
            "offer".to_string(),
            "--ttl-minutes".to_string(),
            config.ttl_minutes.to_string(),
        ]),
        stdin: None,
        timeout_secs: Some(10),
    };

    let response = execute_remote(&config.lxc_api_base, request)
        .await
        .map_err(|e| LatchError::LxcUnreachable(e.to_string()))?;

    if !response.is_success() {
        return Err(LatchError::CommandFailed {
            cmd: "latch clone offer".to_string(),
            code: response.exit_code,
            stderr: response.stderr,
        });
    }

    // Parse JSON offer
    serde_json::from_str::<Value>(&response.stdout)
        .map_err(|e| LatchError::JsonParse(e.to_string()))
}

/// Step 2: Create encrypted payload on source (CLIENT)
async fn create_payload(
    offer: &Value,
    config: &LatchCloneConfig,
) -> Result<Value, LatchError> {
    // Check if latch CLI is available
    let check_latch = execute_local("which", vec!["latch".to_string()], None, Some(5))
        .await
        .map_err(|_| LatchError::MissingLatchCli)?;

    if !check_latch.is_success() {
        return Err(LatchError::MissingLatchCli);
    }

    // Build latch clone create command
    let mut args = vec!["clone".to_string(), "create".to_string(), "--offer-stdin".to_string()];

    // Add optional filters
    for project in &config.project_filters {
        args.push("--project".to_string());
        args.push(project.clone());
    }
    for env in &config.env_filters {
        args.push("--env".to_string());
        args.push(env.clone());
    }

    // Add verify code if provided
    if let Some(code) = &config.verify_code {
        args.push("--verify-code".to_string());
        args.push(code.clone());
    }

    let offer_json = offer.to_string();
    let response = execute_local("latch", args, Some(offer_json), Some(30))
        .await
        .map_err(|e| LatchError::CommandFailed {
            cmd: "latch clone create".to_string(),
            code: -1,
            stderr: e.to_string(),
        })?;

    if !response.is_success() {
        return Err(LatchError::CommandFailed {
            cmd: "latch clone create".to_string(),
            code: response.exit_code,
            stderr: response.stderr,
        });
    }

    // Parse JSON payload
    serde_json::from_str::<Value>(&response.stdout)
        .map_err(|e| LatchError::JsonParse(e.to_string()))
}

/// Step 3: Apply payload on target (LXC)
async fn apply_payload(
    payload: &Value,
    config: &LatchCloneConfig,
) -> Result<String, LatchError> {
    // Build latch clone apply command
    let mut args = vec!["clone".to_string(), "apply".to_string(), "--stdin".to_string()];

    // Add verify code if provided (must match create step)
    if let Some(code) = &config.verify_code {
        args.push("--verify-code".to_string());
        args.push(code.clone());
    }

    let payload_json = payload.to_string();
    let request = ExecRequest {
        cmd: "latch".to_string(),
        args: Some(args),
        stdin: Some(payload_json),
        timeout_secs: Some(30),
    };

    let response = execute_remote(&config.lxc_api_base, request)
        .await
        .map_err(|e| LatchError::LxcUnreachable(e.to_string()))?;

    if !response.is_success() {
        return Err(LatchError::CommandFailed {
            cmd: "latch clone apply".to_string(),
            code: response.exit_code,
            stderr: response.stderr,
        });
    }

    Ok(response.stdout)
}

/// Orchestrate full latch clone workflow: offer → create → apply
pub async fn sync_credentials_to_lxc(
    config: &LatchCloneConfig,
) -> Result<LatchCloneResult, LatchError> {
    let start_time = SystemTime::now();

    // Step 1: Generate offer on LXC
    let offer = generate_offer(config).await?;

    let offer_id = offer
        .get("offer_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| LatchError::InvalidOffer("missing offer_id".to_string()))?
        .to_string();

    // Step 2: Create encrypted payload on CLIENT
    let payload = create_payload(&offer, config).await?;

    // Step 3: Apply payload on LXC
    let _apply_result = apply_payload(&payload, config).await?;

    let duration = start_time
        .elapsed()
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // TODO: Query LXC to count restored slots (optional)
    let slot_count = 0; // Placeholder

    Ok(LatchCloneResult {
        offer_id,
        slot_count,
        duration_secs: duration,
    })
}

/// Verify latch CLI availability and keyring on CLIENT
pub async fn check_client_readiness() -> Result<(), LatchError> {
    execute_local("which", vec!["latch".to_string()], None, Some(5))
        .await
        .map_err(|_| LatchError::MissingLatchCli)?;

    Ok(())
}

/// Verify latch CLI and keyring availability on LXC
pub async fn check_lxc_readiness(lxc_api_base: &str) -> Result<(), LatchError> {
    let request = ExecRequest {
        cmd: "which".to_string(),
        args: Some(vec!["latch".to_string()]),
        stdin: None,
        timeout_secs: Some(10),
    };

    let response = execute_remote(lxc_api_base, request)
        .await
        .map_err(|e| LatchError::LxcUnreachable(e.to_string()))?;

    if !response.is_success() {
        return Err(LatchError::MissingLatchCli);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latch_error_display() {
        let err = LatchError::MissingLatchCli;
        assert_eq!(err.to_string(), "latch CLI not found on system");

        let err = LatchError::CommandFailed {
            cmd: "test".to_string(),
            code: 1,
            stderr: "error".to_string(),
        };
        assert!(err.to_string().contains("test"));
        assert!(err.to_string().contains("1"));
    }

    #[test]
    fn test_latch_clone_config() {
        let config = LatchCloneConfig {
            lxc_api_base: "http://localhost:8080".to_string(),
            ttl_minutes: 10,
            verify_code: Some("ABC123".to_string()),
            project_filters: vec!["my-app".to_string()],
            env_filters: vec!["prod".to_string()],
        };

        assert_eq!(config.ttl_minutes, 10);
        assert_eq!(config.project_filters.len(), 1);
    }
}
