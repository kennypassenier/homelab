use crate::app::{AppState, GitStatus, LatchPullRequest, LogLevel};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// The homelab monorepo is sparse-checked-out here inside every LXC.
const GITOPS_REPO: &str = "/opt/gitops";

/// Lock file — prevents race conditions between cron fallback and API-triggered syncs.
const LOCK_FILE: &str = "/tmp/gitops.lock";

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
    if latch.sparse.unwrap_or(true) { preview_parts.push("--sparse".to_string()); }
    if let Some(v) = latch.env.as_deref().filter(|v| !v.trim().is_empty()) {
        preview_parts.extend(["--env".to_string(), v.to_string()]);
    }
    if latch.pat.as_deref().filter(|v| !v.trim().is_empty()).is_some() {
        preview_parts.extend(["--PAT".to_string(), "[redacted]".to_string()]);
    }
    if latch.key.as_deref().filter(|v| !v.trim().is_empty()).is_some() {
        preview_parts.extend(["--KEY".to_string(), "[redacted]".to_string()]);
    }
    if let Some(v) = latch.secrets_repo.as_deref().filter(|v| !v.trim().is_empty()) {
        preview_parts.extend(["--REPO".to_string(), v.to_string()]);
    }
    if let Some(v) = latch.project.as_deref().filter(|v| !v.trim().is_empty()) {
        preview_parts.extend(["--project".to_string(), v.to_string()]);
    }
    let command_preview = preview_parts.join(" ");

    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!("[sync] running command: cd {} && {}", repo_path, command_preview),
        );
    }

    let latch_bin = resolve_latch_binary().ok_or_else(|| "latch unavailable".to_string())?;
    let mut command = Command::new(latch_bin);
    command.arg("pull");
    if latch.sparse.unwrap_or(true) { command.arg("--sparse"); }
    if let Some(v) = latch.env.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--env", v]);
    }
    if let Some(v) = latch.pat.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--PAT", v]);
    }
    if let Some(v) = latch.key.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--KEY", v]);
    }
    if let Some(v) = latch.secrets_repo.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--REPO", v]);
    }
    if let Some(v) = latch.project.as_deref().filter(|v| !v.trim().is_empty()) {
        command.args(["--project", v]);
    }

    let output = command
        .current_dir(repo_path)
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let out = stdout.trim();
        if out.is_empty() {
            Ok("latch pull ok".to_string())
        } else {
            Ok(format!("latch pull ok: {}", out))
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "latch pull failed (exit {:?}):\nstderr: {}\nstdout: {}",
            output.status.code(),
            stderr.trim(),
            stdout.trim()
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

    ["/usr/local/bin/latch", "/usr/bin/latch", "/home/linuxbrew/.linuxbrew/bin/latch", "latch"]
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

/// Full sync: git fetch → reset --hard → setup.sh → docker compose pull+up for every app.
pub async fn perform_sync(state: Arc<Mutex<AppState>>) {
    // Prevent concurrent syncs
    if Path::new(LOCK_FILE).exists() {
        let mut s = state.lock().unwrap();
        s.sync_requested = false;
        s.add_log(
            LogLevel::Warn,
            "Sync skipped — lock file exists".to_string(),
        );
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

    // Enforce strict stack-scoped sparse checkout on every run to avoid drift.
    let sparse_scope = format!("stacks/{}", stack_name);
    let sparse_result = tokio::task::spawn_blocking(move || {
        run_git(GITOPS_REPO, &["sparse-checkout", "set", &sparse_scope])
    })
    .await
    .unwrap_or_else(|_| Err("spawn failed".to_string()));

    if let Err(e) = sparse_result {
        finish_sync(
            state,
            false,
            format!("sparse scope update failed: {}", e.replace('\n', " ")),
        )
        .await;
        return;
    }

    // ── Step 1: git fetch ──────────────────────────────────────────────────
    let fetch_result = tokio::task::spawn_blocking(|| run_git(GITOPS_REPO, &["fetch", "origin"]))
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));

    if let Err(e) = fetch_result {
        finish_sync(
            state,
            false,
            format!("git fetch failed: {}", e.replace('\n', " ")),
        )
        .await;
        return;
    }

    // ── Step 2: git reset --hard origin/main ─────────────────────────────
    let reset_result =
        tokio::task::spawn_blocking(|| run_git(GITOPS_REPO, &["reset", "--hard", "origin/main"]))
            .await
            .unwrap_or_else(|_| Err("spawn failed".to_string()));

    if let Err(e) = reset_result {
        finish_sync(
            state,
            false,
            format!("git reset failed: {}", e.replace('\n', " ")),
        )
        .await;
        return;
    }
    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Ok, "git reset complete".to_string());
    }

    // ── Step 3: latch pull (unconditional — creates .env files from secrets) ─
    // Prefer a one-shot payload from CLIENT over the cached credentials; fall
    // back to whatever CLIENT pushed last on a heartbeat.
    let latch_creds = {
        let mut s = state.lock().unwrap();
        s.pending_latch_pull
            .take()
            .or_else(|| s.latch_credentials.clone())
    };

    if let Some(ref latch) = latch_creds {
        let sn = stack_name.clone();
        match run_latch_pull(&state, GITOPS_REPO, latch) {
            Ok(msg) => {
                let mut s = state.lock().unwrap();
                s.add_log(
                    LogLevel::Ok,
                    format!("ts=now level=info stack={} latch msg=\"{}\"", sn, msg),
                );
            }
            Err(e) => {
                let mut s = state.lock().unwrap();
                s.add_log(
                    LogLevel::Warn,
                    format!(
                        "ts=now level=warn stack={} latch msg=\"pull failed: {}\"",
                        sn, e
                    ),
                );
            }
        }
    } else {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            "latch pull skipped: no credentials available (CLIENT will push on next heartbeat)"
                .to_string(),
        );
    }

    // ── Step 4: pre-sync hooks (pre-sync.sh) if present ────────────────────
    let pre_sync_path = format!("{}/stacks/{}/pre-sync.sh", GITOPS_REPO, stack_name);
    if Path::new(&pre_sync_path).exists() {
        let stack_dir = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
        let sn = stack_name.clone();
        let hook_result = tokio::task::spawn_blocking(move || {
            Command::new("bash")
                .arg(&pre_sync_path)
                .current_dir(&stack_dir)
                .output()
                .map_err(|e| e.to_string())
                .and_then(|o| {
                    if o.status.success() {
                        Ok(())
                    } else {
                        Err(String::from_utf8_lossy(&o.stderr).to_string())
                    }
                })
        })
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));

        let mut s = state.lock().unwrap();
        match hook_result {
            Ok(_) => s.add_log(
                LogLevel::Ok,
                format!(
                    "ts=now level=info stack={} hook=pre-sync.sh msg=\"hook executed\"",
                    sn
                ),
            ),
            Err(e) => s.add_log(
                LogLevel::Warn,
                format!(
                    "ts=now level=warn stack={} hook=pre-sync.sh msg=\"hook failed: {}\"",
                    sn, e
                ),
            ),
        }
    }

    // ── Step 5: docker compose pull + up for every app in the stack ───────
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

        {
            let mut s = state.lock().unwrap();
            s.add_log(
                LogLevel::Info,
                format!(
                    "[sync] running command: cd {} && docker compose pull -q",
                    app_dir
                ),
            );
        }

        // docker compose pull -q
        let dir_clone = app_dir.clone();
        let pull = tokio::task::spawn_blocking(move || {
            Command::new("docker")
                .args(["compose", "pull", "-q"])
                .current_dir(&dir_clone)
                .output()
                .map_err(|e| e.to_string())
                .and_then(|o| {
                    if o.status.success() {
                        Ok(())
                    } else {
                        Err(String::from_utf8_lossy(&o.stderr).to_string())
                    }
                })
        })
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));

        {
            let mut s = state.lock().unwrap();
            match &pull {
                Ok(_) => s.add_log(
                    LogLevel::Info,
                    format!(
                        "ts=now stack={} app={} msg=\"images pulled\"",
                        stack_name, app_name
                    ),
                ),
                Err(e) => s.add_log(
                    LogLevel::Warn,
                    format!(
                        "ts=now level=warn stack={} app={} msg=\"pull warning: {}\"",
                        stack_name,
                        app_name,
                        e.lines().next().unwrap_or("")
                    ),
                ),
            }
        }

        {
            let mut s = state.lock().unwrap();
            s.add_log(
                LogLevel::Info,
                format!(
                    "[sync] running command: cd {} && docker compose up -d --remove-orphans",
                    app_dir
                ),
            );
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
                    if o.status.success() {
                        Ok(())
                    } else {
                        Err(String::from_utf8_lossy(&o.stderr).to_string())
                    }
                })
        })
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));

        let mut s = state.lock().unwrap();
        match up {
            Ok(_) => s.add_log(
                LogLevel::Ok,
                format!(
                    "ts=now level=info stack={} app={} msg=\"containers up\"",
                    stack_name, app_name
                ),
            ),
            Err(e) => s.add_log(
                LogLevel::Error,
                format!(
                    "ts=now level=error stack={} app={} msg=\"compose up failed: {}\"",
                    stack_name,
                    app_name,
                    e.lines().next().unwrap_or("")
                ),
            ),
        }
    }

    // ── Step 5: Garbage collection - remove orphaned apps ──────────────────
    garbage_collect_orphans(state.clone(), &stack_name).await;

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

/// Remove orphaned apps that exist in /appdata but no longer exist in Git
async fn garbage_collect_orphans(state: Arc<Mutex<AppState>>, stack_name: &str) {
    let appdata_path = Path::new("/appdata");
    if !appdata_path.exists() {
        return;
    }

    let git_stack_path = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
    let git_apps: std::collections::HashSet<String> = list_app_dirs(&git_stack_path)
        .into_iter()
        .filter_map(|p| {
            Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .collect();

    let Ok(entries) = std::fs::read_dir(appdata_path) else {
        return;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let app_name = match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        // Skip if app still exists in Git
        if git_apps.contains(&app_name) {
            continue;
        }

        // App is orphaned - remove it
        {
            let mut s = state.lock().unwrap();
            s.add_log(
                LogLevel::Warn,
                format!(
                    "ts=now level=warn stack={} app={} msg=\"orphaned app detected (no longer in git)\"",
                    stack_name, app_name
                ),
            );
        }

        // Try to stop containers in this app directory
        let app_path = path.clone();
        let app_name_clone = app_name.clone();
        let _stop_result = tokio::task::spawn_blocking(move || {
            // Try docker compose down first
            let compose_path = app_path.join("docker-compose.yml");
            if compose_path.exists() {
                let down = Command::new("docker")
                    .args(["compose", "down", "--remove-orphans"])
                    .current_dir(&app_path)
                    .output();
                if down.is_ok() && down.unwrap().status.success() {
                    return Ok(());
                }
            }

            // Fallback: try stopping by container name pattern
            let _ = Command::new("docker")
                .args([
                    "ps",
                    "-a",
                    "--filter",
                    &format!("name={}", app_name_clone),
                    "--format",
                    "{{.ID}}",
                ])
                .output()
                .ok()
                .and_then(|o| {
                    let ids = String::from_utf8_lossy(&o.stdout);
                    for id in ids.lines() {
                        let _ = Command::new("docker").args(["stop", id]).output();
                        let _ = Command::new("docker").args(["rm", id]).output();
                    }
                    Some(())
                });

            Ok::<(), String>(())
        })
        .await;

        // Remove appdata directory
        let path_clone = path.clone();
        let remove_result = tokio::task::spawn_blocking(move || {
            std::fs::remove_dir_all(&path_clone).map_err(|e| e.to_string())
        })
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));

        let mut s = state.lock().unwrap();
        match remove_result {
            Ok(_) => s.add_log(
                LogLevel::Ok,
                format!(
                    "ts=now level=info stack={} app={} msg=\"orphaned app removed (containers stopped, data deleted)\"",
                    stack_name, app_name
                ),
            ),
            Err(e) => s.add_log(
                LogLevel::Error,
                format!(
                    "ts=now level=error stack={} app={} msg=\"failed to remove orphaned app: {}\"",
                    stack_name, app_name, e
                ),
            ),
        }
    }
}
