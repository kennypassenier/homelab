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

fn validate_stack_scope(stack_name: &str) -> Result<(), String> {
    let trimmed = stack_name.trim();
    if trimmed.is_empty() || trimmed == "unknown" {
        return Err("stack identity is unknown; refusing sync to avoid sparse checkout drift".to_string());
    }

    let stack_dir = format!("{}/stacks/{}", GITOPS_REPO, trimmed);
    if !Path::new(&stack_dir).exists() {
        return Err(format!(
            "stack directory missing after checkout: {}",
            stack_dir
        ));
    }

    Ok(())
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

    if let Err(e) = validate_stack_scope(&stack_name) {
        return finish(state, false, format!("sync preflight failed: {}", e)).await;
    }

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

    if let Err(e) = validate_stack_scope(&stack_name) {
        return finish(state, false, format!("sync scope validation failed: {}", e)).await;
    }

    // ── Step 4: latch pull ────────────────────────────────────────────────
    step(&state, 4, TOTAL, "latch pull secrets", &stack_name);
    match latch_creds {
        Some(creds) => {
            if let Err(e) = secrets::latch_pull(state.clone(), creds).await {
                state.lock().unwrap().add_log(LogLevel::Error, format!("[latch] {}", e));
                return finish(state, false, format!("latch pull validation failed: {}", e)).await;
            }
        }
        None => {
            let msg =
                "no latch credentials from CLIENT (set LATCH_PAT/KEY/REPO in config/.env)"
                    .to_string();
            state
                .lock()
                .unwrap()
                .add_log(LogLevel::Error, format!("[latch] BLOCKED — {}", msg));
            return finish(state, false, format!("latch pull validation failed: {}", msg)).await;
        }
    }

    // ── Step 5: compose-driven filesystem prep ───────────────────────────
    step(&state, 5, TOTAL, "Prepare bind-mounted files from compose manifests", &stack_name);
    if let Err(e) = compose::prepare_stack_bind_mounts(state.clone(), &stack_name).await {
        return finish(state, false, format!("compose prep failed: {}", e)).await;
    }

    // ── Step 6: docker compose pull + up ─────────────────────────────────
    step(&state, 6, TOTAL, "docker compose pull + up for each app", &stack_name);
    if let Err(e) = compose::deploy_apps(state.clone(), &stack_name).await {
        return finish(state, false, format!("compose deployment failed: {}", e)).await;
    }
    compose::garbage_collect(state.clone(), &stack_name).await;

    finish(state, true, "Sync complete".to_string()).await;
}

async fn finish(state: Arc<Mutex<AppState>>, ok: bool, msg: String) {
    let mut s = state.lock().unwrap();
    s.add_log(if ok { LogLevel::Ok } else { LogLevel::Error }, msg);
    s.is_syncing = false;
    let _ = std::fs::remove_file(LOCK_FILE);
}
