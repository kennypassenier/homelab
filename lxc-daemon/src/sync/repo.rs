// Git operations for the LXC sync cycle.
// Responsible for: sparse checkout enforcement, fetch, and reset.

use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::app::{AppState, LogLevel};

const GITOPS_REPO: &str = "/opt/gitops";

struct Capture {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

fn run(args: &[&str]) -> Result<Capture, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(GITOPS_REPO)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(Capture {
        exit_code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

fn log_cmd(state: &Arc<Mutex<AppState>>, cmd: &str, cap: &Capture) {
    let mut s = state.lock().unwrap();
    let lvl = if cap.exit_code == 0 { LogLevel::Ok } else { LogLevel::Error };
    s.add_log(lvl, format!("[sync][exit] {} exit={}", cmd, cap.exit_code));
    for line in cap.stdout.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        s.add_log(LogLevel::Info, format!("[sync][stdout] {} {}", cmd, line));
    }
    let err_lvl = if cap.exit_code == 0 { LogLevel::Warn } else { LogLevel::Error };
    for line in cap.stderr.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        s.add_log(err_lvl.clone(), format!("[sync][stderr] {} {}", cmd, line));
    }
}

/// Enforce sparse checkout scope for the given stack (idempotent).
pub async fn enforce_sparse_checkout(state: Arc<Mutex<AppState>>, stack_name: &str) -> Result<(), String> {
    let scope = format!("stacks/{}", stack_name);
    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, format!("[sync][run] cd {} && git sparse-checkout set {}", GITOPS_REPO, scope));
    }
    let cap = tokio::task::spawn_blocking(move || run(&["sparse-checkout", "set", &scope]))
        .await
        .map_err(|_| "spawn failed".to_string())??;
    log_cmd(&state, "git sparse-checkout set", &cap);
    if cap.exit_code != 0 { return Err(format!("sparse-checkout set: {}", cap.stderr.trim())); }
    Ok(())
}

/// Fetch new commits from origin.
pub async fn git_fetch(state: Arc<Mutex<AppState>>) -> Result<(), String> {
    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, format!("[sync][run] cd {} && git fetch origin", GITOPS_REPO));
    }
    let cap = tokio::task::spawn_blocking(|| run(&["fetch", "origin"]))
        .await
        .map_err(|_| "spawn failed".to_string())??;
    log_cmd(&state, "git fetch origin", &cap);
    if cap.exit_code != 0 { return Err(format!("git fetch: {}", cap.stderr.trim())); }
    Ok(())
}

/// Reset local tree to origin/main.
pub async fn git_reset(state: Arc<Mutex<AppState>>) -> Result<String, String> {
    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, format!("[sync][run] cd {} && git reset --hard origin/main", GITOPS_REPO));
    }
    let cap = tokio::task::spawn_blocking(|| run(&["reset", "--hard", "origin/main"]))
        .await
        .map_err(|_| "spawn failed".to_string())??;
    log_cmd(&state, "git reset --hard origin/main", &cap);
    if cap.exit_code != 0 { return Err(format!("git reset: {}", cap.stderr.trim())); }
    Ok(cap.stdout.trim().to_string())
}
