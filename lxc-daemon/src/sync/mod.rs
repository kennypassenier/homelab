// Sync orchestrator — drives the 6-step LXC sync cycle and emits
// a step-header log event before each named step.
//
// Sub-modules:
//   repo.rs     — git sparse-checkout, fetch, reset
//   secrets.rs  — latch pull
//   compose.rs  — docker compose pull + up, orphan GC

pub mod compose;
pub mod repo;
pub mod secrets;

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::app::{AppState, LatchPullRequest, LogLevel};

const GITOPS_REPO: &str = "/opt/gitops";
const LOCK_FILE: &str = "/tmp/gitops.lock";

/// Emit a numbered step-header log line (rendered in amber in CLIENT UI).
fn step(state: &Arc<Mutex<AppState>>, index: u32, total: u32, label: &str, stack: &str) {
    let msg = format!("[STEP {:>2}/{}] {} — {} (LXC)", index, total, label, stack);
    state.lock().unwrap().add_log(LogLevel::Step, msg);
}

/// Run the full sync cycle.
pub async fn run(state: Arc<Mutex<AppState>>) {
    if Path::new(LOCK_FILE).exists() {
        state.lock().unwrap().add_log(LogLevel::Warn, "Sync skipped — lock file exists".to_string());
        return;
    }
    let _ = std::fs::write(LOCK_FILE, "");

    let (stack_name, latch_creds) = {
        let mut s = state.lock().unwrap();
        s.is_syncing = true;
        s.sync_requested = false;
        s.add_log(LogLevel::Info, "[sync] ============ Sync cycle started ============".to_string());
        s.add_log(LogLevel::Info, "Sync started".to_string());
        let creds = s.pending_latch_pull.take().or_else(|| s.latch_credentials.clone());
        (s.stack_name.clone(), creds)
    };

    const TOTAL: u32 = 6;

    // ── Step 1: enforce sparse checkout ───────────────────────────────────
    step(&state, 1, TOTAL, "Enforce sparse checkout", &stack_name);
    if let Err(e) = repo::enforce_sparse_checkout(state.clone(), &stack_name).await {
        return finish(state, false, format!("sparse checkout: {}", e)).await;
    }

    // ── Step 2: git fetch ─────────────────────────────────────────────────
    step(&state, 2, TOTAL, "git fetch origin", &stack_name);
    if let Err(e) = repo::git_fetch(state.clone()).await {
        return finish(state, false, format!("git fetch: {}", e)).await;
    }

    // ── Step 3: git reset --hard origin/main ─────────────────────────────
    step(&state, 3, TOTAL, "git reset --hard origin/main", &stack_name);
    if let Err(e) = repo::git_reset(state.clone()).await {
        return finish(state, false, format!("git reset: {}", e)).await;
    }

    // ── Step 4: latch pull ────────────────────────────────────────────────
    step(&state, 4, TOTAL, "latch pull secrets", &stack_name);
    match latch_creds {
        Some(creds) => {
            if let Err(e) = secrets::latch_pull(state.clone(), creds).await {
                state.lock().unwrap().add_log(LogLevel::Error, format!("[latch] {}", e));
                // non-fatal — continue
            }
        }
        None => {
            state.lock().unwrap().add_log(LogLevel::Error,
                "[latch] SKIP — no credentials from CLIENT (set LATCH_PAT/KEY/REPO in config/.env)".to_string());
        }
    }

    // ── Step 5: pre-sync hook ─────────────────────────────────────────────
    let pre_sync = format!("{}/stacks/{}/pre-sync.sh", GITOPS_REPO, stack_name);
    if Path::new(&pre_sync).exists() {
        step(&state, 5, TOTAL, "Run pre-sync.sh hook", &stack_name);
        run_hook(&state, &pre_sync, &stack_name).await;
    }

    // ── Step 6: docker compose pull + up ─────────────────────────────────
    step(&state, 6, TOTAL, "docker compose pull + up for each app", &stack_name);
    compose::deploy_apps(state.clone(), &stack_name).await;
    compose::garbage_collect(state.clone(), &stack_name).await;

    finish(state, true, "Sync complete".to_string()).await;
}

async fn run_hook(state: &Arc<Mutex<AppState>>, hook_path: &str, stack_name: &str) {
    let stack_dir = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
    let hook = hook_path.to_string();
    let sn = stack_name.to_string();
    {
        state.lock().unwrap().add_log(LogLevel::Info, format!("[sync][run] cd {} && bash {}", stack_dir, hook));
    }
    let result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("bash").arg(&hook).current_dir(&stack_dir)
            .output().map_err(|e| e.to_string())
    }).await.unwrap_or_else(|_| Err("spawn failed".to_string()));

    match result {
        Ok(o) => {
            let code = o.status.code().unwrap_or(-1);
            let mut s = state.lock().unwrap();
            let lvl = if code == 0 { LogLevel::Ok } else { LogLevel::Warn };
            s.add_log(lvl, format!("[sync][exit] pre-sync.sh stack={} exit={}", sn, code));
            for line in String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.trim().is_empty()) {
                s.add_log(LogLevel::Info, format!("[sync][stdout] pre-sync.sh {}", line));
            }
            for line in String::from_utf8_lossy(&o.stderr).lines().filter(|l| !l.trim().is_empty()) {
                s.add_log(LogLevel::Warn, format!("[sync][stderr] pre-sync.sh {}", line));
            }
        }
        Err(e) => state.lock().unwrap().add_log(LogLevel::Error, format!("[sync][spawn] pre-sync.sh {}", e)),
    }
}

async fn finish(state: Arc<Mutex<AppState>>, ok: bool, msg: String) {
    let mut s = state.lock().unwrap();
    s.add_log(if ok { LogLevel::Ok } else { LogLevel::Error }, msg);
    s.is_syncing = false;
    let _ = std::fs::remove_file(LOCK_FILE);
}
