// Docker compose operations and orphan garbage collection.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::app::{AppState, LogLevel};

const GITOPS_REPO: &str = "/opt/gitops";

fn capture(cmd: &mut Command) -> (i32, String, String) {
    match cmd.output() {
        Ok(o) => (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        ),
        Err(e) => (-1, String::new(), e.to_string()),
    }
}

fn log_cmd(state: &Arc<Mutex<AppState>>, label: &str, code: i32, stdout: &str, stderr: &str) {
    let mut s = state.lock().unwrap();
    let lvl = if code == 0 { LogLevel::Ok } else { LogLevel::Error };
    s.add_log(lvl, format!("[sync][exit] {} exit={}", label, code));
    for line in stdout.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        s.add_log(LogLevel::Info, format!("[sync][stdout] {} {}", label, line));
    }
    let elv = if code == 0 { LogLevel::Warn } else { LogLevel::Error };
    for line in stderr.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        s.add_log(elv.clone(), format!("[sync][stderr] {} {}", label, line));
    }
}

fn list_app_dirs(stack_dir: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(stack_dir) else { return vec![]; };
    let mut dirs: Vec<String> = rd.filter_map(|e| {
        let e = e.ok()?;
        if e.file_type().ok()?.is_dir() { Some(e.path().to_string_lossy().to_string()) } else { None }
    }).collect();
    dirs.sort();
    dirs
}

/// Pull images and start all app compose dirs for a stack.
pub async fn deploy_apps(state: Arc<Mutex<AppState>>, stack_name: &str) {
    let stack_dir = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
    let app_dirs = list_app_dirs(&stack_dir);

    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, format!("[sync] discovered {} app directories under {}", app_dirs.len(), stack_dir));
    }

    for app_dir in &app_dirs {
        let compose = format!("{}/docker-compose.yml", app_dir);
        if !Path::new(&compose).exists() { continue; }

        let app_name = Path::new(app_dir).file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // docker compose pull -q
        {
            let mut s = state.lock().unwrap();
            s.add_log(LogLevel::Info, format!("[sync][run] cd {} && docker compose pull -q", app_dir));
        }
        let dir = app_dir.clone();
        let an = app_name.clone();
        let (code, out, err) = tokio::task::spawn_blocking(move || {
            capture(Command::new("docker").args(["compose", "pull", "-q"]).current_dir(&dir))
        }).await.unwrap_or((-1, String::new(), "spawn failed".to_string()));
        log_cmd(&state, &format!("docker compose pull app={}", an), code, &out, &err);

        // docker compose up -d --remove-orphans
        {
            let mut s = state.lock().unwrap();
            s.add_log(LogLevel::Info, format!("[sync][run] cd {} && docker compose up -d --remove-orphans", app_dir));
        }
        let dir2 = app_dir.clone();
        let an2 = app_name.clone();
        let (code2, out2, err2) = tokio::task::spawn_blocking(move || {
            capture(Command::new("docker").args(["compose", "up", "-d", "--remove-orphans"]).current_dir(&dir2))
        }).await.unwrap_or((-1, String::new(), "spawn failed".to_string()));
        log_cmd(&state, &format!("docker compose up app={}", an2), code2, &out2, &err2);
    }
}

/// Stop and remove appdata dirs for apps that no longer exist in Git.
pub async fn garbage_collect(state: Arc<Mutex<AppState>>, stack_name: &str) {
    let appdata = Path::new("/appdata");
    if !appdata.exists() { return; }

    let git_stack = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
    let git_apps: HashSet<String> = list_app_dirs(&git_stack).into_iter()
        .filter_map(|p| Path::new(&p).file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();

    let Ok(entries) = std::fs::read_dir(appdata) else { return; };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let app_name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        if git_apps.contains(&app_name) { continue; }

        {
            let mut s = state.lock().unwrap();
            s.add_log(LogLevel::Warn, format!("[sync] orphan gc: {} — no longer in git", app_name));
        }

        let compose = path.join("docker-compose.yml");
        if compose.exists() {
            let p2 = path.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = Command::new("docker").args(["compose", "down", "--remove-orphans"]).current_dir(&p2).output();
            }).await;
        }

        let p3 = path.clone();
        let sn = stack_name.to_string();
        let an = app_name.clone();
        let result = tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&p3).map_err(|e| e.to_string()))
            .await.unwrap_or_else(|_| Err("spawn failed".to_string()));
        let mut s = state.lock().unwrap();
        match result {
            Ok(_)  => s.add_log(LogLevel::Ok, format!("[sync] orphan gc: stack={} app={} removed", sn, an)),
            Err(e) => s.add_log(LogLevel::Error, format!("[sync] orphan gc: stack={} app={} error: {}", sn, an, e)),
        }
    }
}
