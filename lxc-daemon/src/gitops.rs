use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use crate::app::{AppState, GitStatus, LogLevel};

/// The homelab monorepo is sparse-checked-out here inside every LXC.
const GITOPS_REPO: &str = "/opt/gitops";

/// Lock file — prevents race conditions between cron fallback and API-triggered syncs.
const LOCK_FILE: &str = "/tmp/gitops.lock";

pub async fn run_checker(state: Arc<Mutex<AppState>>) {
    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, "GitOps checker started".to_string());
    }

    // On startup, make sure the sparse checkout exists
    ensure_sparse_checkout(state.clone()).await;

    loop {
        check_git_status(state.clone()).await;

        let requested = {
            let s = state.lock().unwrap();
            s.sync_requested
        };
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
             Set GITOPS_REPO_URL to the homelab git repo URL.".to_string(),
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
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        let _ = std::fs::create_dir_all(GITOPS_REPO);

        let clone = Command::new("git")
            .args(["clone", "--filter=blob:none", "--no-checkout", &repo_url, GITOPS_REPO])
            .output()
            .map_err(|e| e.to_string())?;
        if !clone.status.success() {
            return Err(String::from_utf8_lossy(&clone.stderr).to_string());
        }

        run_git(GITOPS_REPO, &["sparse-checkout", "init", "--cone"])?;
        run_git(GITOPS_REPO, &["sparse-checkout", "set", &format!("stacks/{}", stack_clone)])?;
        run_git(GITOPS_REPO, &["checkout", "main"])?;
        Ok(())
    })
    .await
    .unwrap_or_else(|_| Err("spawn failed".to_string()));

    let mut s = state.lock().unwrap();
    match result {
        Ok(_) => s.add_log(LogLevel::Ok, "Sparse checkout initialised".to_string()),
        Err(e) => s.add_log(LogLevel::Error, format!("Sparse checkout init failed: {}", e)),
    }
}

/// Reads git metadata and updates AppState::git (runs every 30 s).
async fn check_git_status(state: Arc<Mutex<AppState>>) {
    let stack_name = {
        let s = state.lock().unwrap();
        s.stack_name.clone()
    };

    let (branch, commit, remote_url, is_synced) = tokio::task::spawn_blocking(move || {
        let branch = run_git(GITOPS_REPO, &["rev-parse", "--abbrev-ref", "HEAD"])
            .unwrap_or_else(|_| "unknown".to_string());
        let commit = run_git(GITOPS_REPO, &["log", "--oneline", "-1"])
            .unwrap_or_else(|_| "—".to_string());
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
    .unwrap_or_else(|_| (
        "unknown".to_string(),
        "—".to_string(),
        "—".to_string(),
        false,
    ));

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let lock_free = !Path::new(LOCK_FILE).exists();

    let mut s = state.lock().unwrap();
    s.git = GitStatus {
        repo_url: remote_url,
        branch,
        commit,
        sparse: format!("stacks/{}/*", stack_name),
        is_synced,
        last_sync: now,
        next_sync: "in 30m".to_string(),
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

fn check_is_synced(repo_path: &str) -> bool {
    let local  = run_git(repo_path, &["rev-parse", "HEAD"]).unwrap_or_default();
    let remote = run_git(repo_path, &["rev-parse", "@{u}"]).unwrap_or_default();
    !local.is_empty() && local.trim() == remote.trim()
}

/// Full sync: git fetch → reset --hard → setup.sh → docker compose pull+up for every app.
pub async fn perform_sync(state: Arc<Mutex<AppState>>) {
    // Prevent concurrent syncs
    if Path::new(LOCK_FILE).exists() {
        let mut s = state.lock().unwrap();
        s.sync_requested = false;
        s.add_log(LogLevel::Warn, "Sync skipped — lock file exists".to_string());
        return;
    }
    let _ = std::fs::write(LOCK_FILE, "");

    {
        let mut s = state.lock().unwrap();
        s.is_syncing = true;
        s.sync_requested = false;
        s.add_log(LogLevel::Info, "Sync started".to_string());
    }

    let stack_name = {
        let s = state.lock().unwrap();
        s.stack_name.clone()
    };

    // ── Step 1: git fetch ──────────────────────────────────────────────────
    let fetch_result =
        tokio::task::spawn_blocking(|| run_git(GITOPS_REPO, &["fetch", "origin"]))
            .await
            .unwrap_or_else(|_| Err("spawn failed".to_string()));

    if let Err(e) = fetch_result {
        finish_sync(state, false, format!("git fetch failed: {}", e.replace('\n', " "))).await;
        return;
    }

    // ── Step 2: git reset --hard origin/main ─────────────────────────────
    let reset_result = tokio::task::spawn_blocking(|| {
        run_git(GITOPS_REPO, &["reset", "--hard", "origin/main"])
    })
    .await
    .unwrap_or_else(|_| Err("spawn failed".to_string()));

    if let Err(e) = reset_result {
        finish_sync(state, false, format!("git reset failed: {}", e.replace('\n', " "))).await;
        return;
    }
    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Ok, "git reset complete".to_string());
    }

    // ── Step 3: pre-deploy hook (setup.sh) if present ────────────────────
    let setup_path = format!("{}/stacks/{}/setup.sh", GITOPS_REPO, stack_name);
    if Path::new(&setup_path).exists() {
        let stack_dir = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
        let sn = stack_name.clone();
        let hook_result = tokio::task::spawn_blocking(move || {
            Command::new("bash")
                .arg(&setup_path)
                .current_dir(&stack_dir)
                .output()
                .map_err(|e| e.to_string())
                .and_then(|o| {
                    if o.status.success() { Ok(()) }
                    else { Err(String::from_utf8_lossy(&o.stderr).to_string()) }
                })
        })
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));

        let mut s = state.lock().unwrap();
        match hook_result {
            Ok(_) => s.add_log(LogLevel::Ok, format!("ts=now stack={} app=setup.sh msg=\"hook ok\"", sn)),
            Err(e) => s.add_log(LogLevel::Warn, format!("setup.sh failed: {}", e)),
        }
    }

    // ── Step 4: docker compose pull + up for every app in the stack ───────
    let stack_dir = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
    let app_dirs = list_app_dirs(&stack_dir);

    for app_dir in app_dirs {
        let compose_file = format!("{}/docker-compose.yml", app_dir);
        if !Path::new(&compose_file).exists() {
            continue;
        }

        let app_name = Path::new(&app_dir)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // docker compose pull -q
        let dir_clone = app_dir.clone();
        let pull = tokio::task::spawn_blocking(move || {
            Command::new("docker")
                .args(["compose", "pull", "-q"])
                .current_dir(&dir_clone)
                .output()
                .map_err(|e| e.to_string())
                .and_then(|o| {
                    if o.status.success() { Ok(()) }
                    else { Err(String::from_utf8_lossy(&o.stderr).to_string()) }
                })
        })
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));

        {
            let mut s = state.lock().unwrap();
            match &pull {
                Ok(_) => s.add_log(LogLevel::Info,
                    format!("ts=now stack={} app={} msg=\"images pulled\"", stack_name, app_name)),
                Err(e) => s.add_log(LogLevel::Warn,
                    format!("ts=now stack={} app={} msg=\"pull warning: {}\"", stack_name, app_name,
                        e.lines().next().unwrap_or(""))),
            }
        }

        // docker compose up -d --remove-orphans
        let dir_clone2 = app_dir.clone();
        let up = tokio::task::spawn_blocking(move || {
            Command::new("docker")
                .args(["compose", "up", "-d", "--remove-orphans"])
                .current_dir(&dir_clone2)
                .output()
                .map_err(|e| e.to_string())
                .and_then(|o| {
                    if o.status.success() { Ok(()) }
                    else { Err(String::from_utf8_lossy(&o.stderr).to_string()) }
                })
        })
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));

        let mut s = state.lock().unwrap();
        match up {
            Ok(_) => s.add_log(LogLevel::Ok,
                format!("ts=now stack={} app={} msg=\"containers up\"", stack_name, app_name)),
            Err(e) => s.add_log(LogLevel::Error,
                format!("ts=now stack={} app={} msg=\"compose up failed: {}\"", stack_name, app_name,
                    e.lines().next().unwrap_or(""))),
        }
    }

    finish_sync(state, true, "Sync complete".to_string()).await;
}

async fn finish_sync(state: Arc<Mutex<AppState>>, ok: bool, msg: String) {
    let mut s = state.lock().unwrap();
    if ok {
        s.add_log(LogLevel::Ok, msg);
    } else {
        s.add_log(LogLevel::Error, msg);
    }
    s.is_syncing = false;
    let _ = std::fs::remove_file(LOCK_FILE);
}

fn list_app_dirs(stack_dir: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(stack_dir) else {
        return vec![];
    };
    let mut dirs: Vec<String> = entries
        .filter_map(|e| {
            let entry = e.ok()?;
            if entry.file_type().ok()?.is_dir() {
                Some(entry.path().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    dirs.sort(); // deterministic order
    dirs
}
