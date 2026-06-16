// Secrets pull via the latch CLI.
// All credential values are redacted before logging.

use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::app::{AppState, LatchPullRequest, LogLevel};

const GITOPS_REPO: &str = "/opt/gitops";

fn redact(text: &str, latch: &LatchPullRequest) -> String {
    let mut out = text.to_string();
    for secret in [latch.pat.as_deref(), latch.key.as_deref()].into_iter().flatten() {
        let trimmed = secret.trim();
        if !trimmed.is_empty() {
            out = out.replace(trimmed, "[redacted]");
        }
    }
    out
}

fn resolve_latch() -> Option<String> {
    for candidate in ["/usr/local/bin/latch", "/usr/bin/latch", "latch"] {
        if candidate == "latch" {
            if Command::new("latch").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
                return Some("latch".to_string());
            }
        } else if std::path::Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

/// Run `latch pull` with provided credentials.
pub async fn latch_pull(state: Arc<Mutex<AppState>>, latch: LatchPullRequest) -> Result<(), String> {
    let bin = resolve_latch().ok_or_else(|| "latch CLI not found in container".to_string())?;

    // Build redacted preview for the log.
    let mut preview = vec![bin.clone(), "pull".to_string()];
    if latch.sparse.unwrap_or(true) { preview.push("--sparse".to_string()); }
    if let Some(e) = latch.env.as_deref().filter(|v| !v.trim().is_empty()) { preview.extend(["--env".to_string(), e.to_string()]); }
    if latch.pat.as_deref().filter(|v| !v.trim().is_empty()).is_some() { preview.extend(["--PAT".to_string(), "[redacted]".to_string()]); }
    if latch.key.as_deref().filter(|v| !v.trim().is_empty()).is_some() { preview.extend(["--KEY".to_string(), "[redacted]".to_string()]); }
    if let Some(r) = latch.secrets_repo.as_deref().filter(|v| !v.trim().is_empty()) { preview.extend(["--REPO".to_string(), r.to_string()]); }
    if let Some(p) = latch.project.as_deref().filter(|v| !v.trim().is_empty()) { preview.extend(["--project".to_string(), p.to_string()]); }

    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, format!("[sync][run] cd {} && {}", GITOPS_REPO, preview.join(" ")));
    }

    let latch_clone = latch.clone();
    let output = tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(&bin);
        cmd.arg("pull").current_dir(GITOPS_REPO);
        if latch_clone.sparse.unwrap_or(true) { cmd.arg("--sparse"); }
        if let Some(v) = latch_clone.env.as_deref().filter(|v| !v.trim().is_empty()) { cmd.args(["--env", v]); }
        if let Some(v) = latch_clone.pat.as_deref().filter(|v| !v.trim().is_empty()) { cmd.args(["--PAT", v]); }
        if let Some(v) = latch_clone.key.as_deref().filter(|v| !v.trim().is_empty()) { cmd.args(["--KEY", v]); }
        if let Some(v) = latch_clone.secrets_repo.as_deref().filter(|v| !v.trim().is_empty()) { cmd.args(["--REPO", v]); }
        if let Some(v) = latch_clone.project.as_deref().filter(|v| !v.trim().is_empty()) { cmd.args(["--project", v]); }
        cmd.output().map_err(|e| e.to_string()).map(|o| {
            (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string())
        })
    }).await.map_err(|_| "spawn failed".to_string())??;

    let (code, stdout, stderr) = output;
    let safe_stdout = redact(&stdout, &latch);
    let safe_stderr = redact(&stderr, &latch);

    {
        let mut s = state.lock().unwrap();
        let lvl = if code == 0 { LogLevel::Ok } else { LogLevel::Error };
        s.add_log(lvl, format!("[sync][exit] latch pull exit={}", code));
        for line in safe_stdout.lines().filter(|l| !l.trim().is_empty()) {
            s.add_log(LogLevel::Info, format!("[sync][stdout] latch pull {}", line));
        }
        for line in safe_stderr.lines().filter(|l| !l.trim().is_empty()) {
            s.add_log(LogLevel::Warn, format!("[sync][stderr] latch pull {}", line));
        }
    }

    if code != 0 {
        return Err(format!("latch pull failed (exit {}): {}", code, safe_stderr.trim()));
    }
    Ok(())
}
