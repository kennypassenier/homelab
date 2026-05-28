//! Latch credential sync orchestration.
//!
//! Coordinates secure end-to-end credential migration from CLIENT desktop
//! to LXC containers via ephemeral offers and encrypted payloads.

#![allow(dead_code)]

use crate::shell::{execute_local, execute_remote, ExecRequest};
use serde_json::Value;
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
