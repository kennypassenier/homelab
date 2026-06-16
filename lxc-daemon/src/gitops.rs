use crate::app::{AppState, GitStatus, LatchPullRequest, LogLevel};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// The homelab monorepo is sparse-checked-out here inside every LXC.
const GITOPS_REPO: &str = "/opt/gitops";

/// Lock file — prevents race conditions between cron fallback and API-triggered syncs.
const LOCK_FILE: &str = "/tmp/gitops.lock";

pub fn repo_path() -> &'static str {
    GITOPS_REPO
}

struct CommandCapture {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

fn capture_command_output(command: &mut Command) -> Result<CommandCapture, String> {
    let output = command.output().map_err(|e| e.to_string())?;
    Ok(CommandCapture {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn log_sync_command_start(state: &Arc<Mutex<AppState>>, preview: impl Into<String>) {
    let mut s = state.lock().unwrap();
    s.add_log(LogLevel::Info, format!("[sync][run] {}", preview.into()));
}

fn log_sync_command_capture(
    state: &Arc<Mutex<AppState>>,
    scope: &str,
    capture: &CommandCapture,
    success_level: LogLevel,
) {
    let mut s = state.lock().unwrap();
    s.add_log(
        success_level,
        format!("[sync][exit] {} exit={}", scope, capture.exit_code),
    );

    let mut emitted = false;
    for line in capture
        .stdout
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
    {
        emitted = true;
        s.add_log(LogLevel::Info, format!("[sync][stdout] {} {}", scope, line));
    }

    let stderr_level = if capture.exit_code == 0 {
        LogLevel::Warn
    } else {
        LogLevel::Error
    };
    for line in capture
        .stderr
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
    {
        emitted = true;
        s.add_log(
            stderr_level.clone(),
            format!("[sync][stderr] {} {}", scope, line),
        );
    }

    if !emitted {
        s.add_log(
            LogLevel::Info,
            format!("[sync][output] {} (no output)", scope),
        );
    }
}

fn log_sync_spawn_error(state: &Arc<Mutex<AppState>>, scope: &str, err: &str) {
    let mut s = state.lock().unwrap();
    for line in err.lines() {
        s.add_log(LogLevel::Error, format!("[sync][spawn] {} {}", scope, line));
    }
}

fn run_git_capture(repo_path: &str, args: &[&str]) -> Result<CommandCapture, String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(repo_path);
    capture_command_output(&mut command)
}

fn redact_latch_output(text: &str, latch: &LatchPullRequest) -> String {
    let mut redacted = text.to_string();
    for secret in [latch.pat.as_deref(), latch.key.as_deref()]
        .into_iter()
        .flatten()
    {
        let trimmed = secret.trim();
        if !trimmed.is_empty() {
            redacted = redacted.replace(trimmed, "[redacted]");
        }
    }
    redacted
}

pub async fn run_checker(state: Arc<Mutex<AppState>>) {
    let mut last_failsafe_window = std::time::Instant::now();

    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, "GitOps checker started".to_string());
    }

    // On startup, make sure the sparse checkout exists
    ensure_sparse_checkout(state.clone()).await;

    loop {
        let failsafe_interval_secs: u64 = std::env::var("FAILSAFE_SYNC_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1800) // Changed from 3600 to 1800 (30 minutes)
            .max(60);

        let heartbeat_ttl_secs: i64 = std::env::var("HEARTBEAT_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(180)
            .max(30);

        let (heartbeat_fresh, heartbeat_age_secs) = {
            let s = state.lock().unwrap();
            let now = chrono::Utc::now().timestamp();
            let age = s
                .client_heartbeat_ts
                .map(|ts| (now - ts).max(0))
                .unwrap_or(i64::MAX);
            (age <= heartbeat_ttl_secs, age)
        };

        let elapsed = last_failsafe_window.elapsed().as_secs();
        let remaining = failsafe_interval_secs.saturating_sub(elapsed);
        let next_sync = if heartbeat_fresh {
            format!(
                "failsafe in ~{}m (suppressed; heartbeat {}s ago)",
                (remaining + 59) / 60,
                heartbeat_age_secs
            )
        } else {
            format!("failsafe in ~{}m", (remaining + 59) / 60)
        };

        check_git_status(state.clone(), next_sync).await;

        let mut requested = {
            let s = state.lock().unwrap();
            s.sync_requested
        };

        let failsafe_due = elapsed >= failsafe_interval_secs;
        if failsafe_due {
            if heartbeat_fresh {
                let mut s = state.lock().unwrap();
                s.add_log(
                    LogLevel::Info,
                    format!(
                        "Failsafe sync window skipped: CLIENT heartbeat is fresh ({}s <= {}s)",
                        heartbeat_age_secs, heartbeat_ttl_secs
                    ),
                );
            } else {
                {
                    let mut s = state.lock().unwrap();
                    s.add_log(
                        LogLevel::Warn,
                        format!(
                            "Failsafe sync triggered: no CLIENT heartbeat (age={}s ttl={}s)",
                            heartbeat_age_secs, heartbeat_ttl_secs
                        ),
                    );
                    s.sync_requested = true;
                }
            }
            last_failsafe_window = std::time::Instant::now();
            requested = {
                let s = state.lock().unwrap();
                s.sync_requested
            };
        }

        if requested {
            perform_sync(state.clone()).await;
        }

        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

/// Initialises the Git sparse checkout if it does not already exist.
async fn ensure_sparse_checkout(state: Arc<Mutex<AppState>>) {
    if Path::new(&format!("{}/.git", GITOPS_REPO)).exists() {
        return; // Already initialised
    }

    let repo_url = std::env::var("GITOPS_REPO_URL").unwrap_or_default();
    if repo_url.is_empty() {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Warn,
            "GITOPS_REPO_URL not set — skipping sparse checkout init. \
             Set GITOPS_REPO_URL to the homelab git repo URL."
                .to_string(),
        );
        return;
    }

    let stack_name = {
        let s = state.lock().unwrap();
        s.stack_name.clone()
    };
    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!("Initialising sparse checkout for stack '{}'…", stack_name),
        );
    }

    let stack_clone = stack_name.clone();
    let auth_repo_url = authenticated_repo_url(&repo_url);
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let _ = std::fs::create_dir_all(GITOPS_REPO);

        let clone = Command::new("git")
            .args([
                "clone",
                "--filter=blob:none",
                "--no-checkout",
                &auth_repo_url,
                GITOPS_REPO,
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !clone.status.success() {
            return Err(String::from_utf8_lossy(&clone.stderr).to_string());
        }

        run_git(GITOPS_REPO, &["sparse-checkout", "init", "--cone"])?;
        run_git(
            GITOPS_REPO,
            &["sparse-checkout", "set", &format!("stacks/{}", stack_clone)],
        )?;
        run_git(GITOPS_REPO, &["checkout", "main"])?;
        Ok(())
    })
    .await
    .unwrap_or_else(|_| Err("spawn failed".to_string()));

    let mut s = state.lock().unwrap();
    match result {
        Ok(_) => s.add_log(LogLevel::Ok, "Sparse checkout initialised".to_string()),
        Err(e) => s.add_log(
            LogLevel::Error,
            format!("Sparse checkout init failed: {}", e),
        ),
    }
}

/// Reads git metadata and updates AppState::git (runs every 30 s).
async fn check_git_status(state: Arc<Mutex<AppState>>, next_sync: String) {
    let stack_name = {
        let s = state.lock().unwrap();
        s.stack_name.clone()
    };

    let (branch, commit, remote_url, is_synced) = tokio::task::spawn_blocking(move || {
        let branch = run_git(GITOPS_REPO, &["rev-parse", "--abbrev-ref", "HEAD"])
            .unwrap_or_else(|_| "unknown".to_string());
        let commit =
            run_git(GITOPS_REPO, &["log", "--oneline", "-1"]).unwrap_or_else(|_| "—".to_string());
        let commit_short = commit.split_whitespace().next().unwrap_or("—").to_string();
        let remote_url = run_git(GITOPS_REPO, &["config", "--get", "remote.origin.url"])
            .unwrap_or_else(|_| "—".to_string());
        let is_synced = check_is_synced(GITOPS_REPO);
        (
            branch.trim().to_string(),
            commit_short,
            remote_url.trim().to_string(),
            is_synced,
        )
    })
    .await
    .unwrap_or_else(|_| {
        (
            "unknown".to_string(),
            "—".to_string(),
            "—".to_string(),
            false,
        )
    });

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let lock_free = !Path::new(LOCK_FILE).exists();

    let mut s = state.lock().unwrap();
    s.git = GitStatus {
        repo_url: remote_url,
        branch,
        commit,
        sparse: format!("stacks/{}/**", stack_name),
        is_synced,
        last_sync: now,
        next_sync,
        lock_free,
    };
}

fn run_git(repo_path: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn run_latch_pull(
    state: &Arc<Mutex<AppState>>,
    repo_path: &str,
    latch: &LatchPullRequest,
) -> Result<String, String> {
    let bin = resolve_latch_binary().unwrap_or_else(|| "latch".to_string());

    // Build a display version with secrets redacted so the full command is visible in logs.
    let mut preview_parts: Vec<String> = vec![bin.clone(), "pull".to_string()];
    if latch.sparse.unwrap_or(true) {
        preview_parts.push("--sparse".to_string());
    }
    if let Some(v) = latch.env.as_deref().filter(|v| !v.trim().is_empty()) {
        preview_parts.extend(["--env".to_string(), v.to_string()]);
    }
    if latch
        .pat
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .is_some()
    {
        preview_parts.extend(["--PAT".to_string(), "[redacted]".to_string()]);
    }
    if latch
        .key
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .is_some()
    {
        preview_parts.extend(["--KEY".to_string(), "[redacted]".to_string()]);
    }
    if let Some(v) = latch
        .secrets_repo
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        preview_parts.extend(["--REPO".to_string(), v.to_string()]);
    }
    if let Some(v) = latch.project.as_deref().filter(|v| !v.trim().is_empty()) {
        preview_parts.extend(["--project".to_string(), v.to_string()]);
    }
    let command_preview = preview_parts.join(" ");

    log_sync_command_start(state, format!("cd {} && {}", repo_path, command_preview));

    let latch_bin = resolve_latch_binary().ok_or_else(|| "latch unavailable".to_string())?;
    let mut command = Command::new(latch_bin);
    command.arg("pull");
    if latch.sparse.unwrap_or(true) {
        command.arg("--sparse");
    }
    if let Some(v) = latch.env.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--env", v]);
    }
    if let Some(v) = latch.pat.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--PAT", v]);
    }
    if let Some(v) = latch.key.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--KEY", v]);
    }
    if let Some(v) = latch
        .secrets_repo
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        command.args(["--REPO", v]);
    }
    if let Some(v) = latch.project.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--project", v]);
    }

    let output = capture_command_output(command.current_dir(repo_path))?;
    let sanitized = CommandCapture {
        exit_code: output.exit_code,
        stdout: redact_latch_output(&output.stdout, latch),
        stderr: redact_latch_output(&output.stderr, latch),
    };

    if sanitized.exit_code == 0 {
        log_sync_command_capture(state, "latch pull", &sanitized, LogLevel::Ok);
        let out = sanitized.stdout.trim();
        if out.is_empty() {
            Ok("latch pull ok".to_string())
        } else {
            Ok(format!("latch pull ok: {}", out))
        }
    } else {
        log_sync_command_capture(state, "latch pull", &sanitized, LogLevel::Error);
        Err(format!(
            "latch pull failed (exit {:?}):\nstderr: {}\nstdout: {}",
            sanitized.exit_code,
            sanitized.stderr.trim(),
            sanitized.stdout.trim()
        ))
    }
}

fn authenticated_repo_url(repo_url: &str) -> String {
    let token = std::env::var("GITOPS_REPO_TOKEN").unwrap_or_default();
    if token.is_empty() || !repo_url.starts_with("https://") {
        return repo_url.to_string();
    }

    repo_url.replacen("https://", &format!("https://{}@", token), 1)
}

fn resolve_latch_binary() -> Option<String> {
    if let Ok(value) = std::env::var("LATCH_BIN") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    [
        "/usr/local/bin/latch",
        "/usr/bin/latch",
        "/home/linuxbrew/.linuxbrew/bin/latch",
        "latch",
    ]
    .iter()
    .find_map(|candidate| {
        if *candidate == "latch" {
            let output = Command::new(candidate).arg("--version").output().ok()?;
            if output.status.success() {
                return Some(candidate.to_string());
            }
            return None;
        }

        if std::path::Path::new(candidate).exists() {
            Some(candidate.to_string())
        } else {
            None
        }
    })
}

fn check_is_synced(repo_path: &str) -> bool {
    let local = run_git(repo_path, &["rev-parse", "HEAD"]).unwrap_or_default();
    let remote = run_git(repo_path, &["rev-parse", "@{u}"]).unwrap_or_default();
    !local.is_empty() && local.trim() == remote.trim()
}

/// Full sync — now delegated to the modular sync engine in sync/mod.rs.
pub async fn perform_sync(state: Arc<Mutex<AppState>>) {
    crate::sync::run(state).await;
}
