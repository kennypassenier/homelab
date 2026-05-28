//! Backup orchestration — pause each LXC's containers via HTTP API, run Restic,
//! then resume containers. Uses blocking reqwest since the HOST daemon is synchronous.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;

const LXC_API_PORT: u16 = 8080;
const POLICY_POLL_SECONDS: u64 = 30;
const KNOWN_STACKS: [&str; 7] = [
    "cloudflared",
    "downloader",
    "gateway",
    "media",
    "monitoring",
    "paperless",
    "vikunja",
];

static BACKUP_CYCLE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Result of a single stack's backup run.
#[allow(dead_code)]
pub struct BackupResult {
    pub stack: String,
    pub lxc_ip: String,
    pub paused: bool,
    pub backup_ok: bool,
    pub resumed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackupSchedule {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub retention_daily: u32,
    pub retention_weekly: u32,
    pub retention_monthly: u32,
    pub notify_on_success: bool,
    pub notify_on_failure: bool,
}

impl Default for BackupSchedule {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 24 * 60,
            retention_daily: 7,
            retention_weekly: 4,
            retention_monthly: 3,
            notify_on_success: false,
            notify_on_failure: true,
        }
    }
}

/// Runs the full backup cycle for a list of (stack_name, lxc_ip) pairs.
///
/// For each stack:
///   1. POST /api/backup/pause  → LXC freezes writes
///   2. restic backup /opt/appdata/<stack>
///   3. POST /api/backup/resume → LXC resumes writes
///
/// Uses a Rust Drop Guard pattern: resume is always sent, even if Restic panics.
pub fn run_backup_cycle(stacks: &[(&str, &str)]) -> Vec<BackupResult> {
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return stacks
                .iter()
                .map(|(s, ip)| BackupResult {
                    stack: s.to_string(),
                    lxc_ip: ip.to_string(),
                    paused: false,
                    backup_ok: false,
                    resumed: false,
                    message: format!("HTTP client error: {}", e),
                })
                .collect();
        }
    };

    stacks
        .iter()
        .map(|(stack, lxc_ip)| backup_one_stack(&client, stack, lxc_ip))
        .collect()
}

pub fn run_backup_cycle_owned_guarded(stacks: Vec<(String, String)>) -> Option<Vec<BackupResult>> {
    if BACKUP_CYCLE_ACTIVE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return None;
    }

    let refs: Vec<(&str, &str)> = stacks
        .iter()
        .map(|(s, ip)| (s.as_str(), ip.as_str()))
        .collect();
    let results = run_backup_cycle(&refs);
    BACKUP_CYCLE_ACTIVE.store(false, Ordering::SeqCst);
    Some(results)
}

pub fn start_policy_enforcer(status_tx: std::sync::mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let mut elapsed_seconds: u64 = 0;
        let mut last_enabled = false;

        loop {
            let schedule = load_schedule();

            if schedule.enabled != last_enabled {
                let _ = status_tx.send(format!(
                    "POLICY enabled={} interval={}m retention(d/w/m)={}/{}/{} notify(success/failure)={}/{}",
                    schedule.enabled,
                    schedule.interval_minutes,
                    schedule.retention_daily,
                    schedule.retention_weekly,
                    schedule.retention_monthly,
                    schedule.notify_on_success,
                    schedule.notify_on_failure,
                ));
                last_enabled = schedule.enabled;
            }

            if schedule.enabled {
                let interval_seconds = (schedule.interval_minutes as u64)
                    .saturating_mul(60)
                    .max(60);
                elapsed_seconds = elapsed_seconds.saturating_add(POLICY_POLL_SECONDS);

                if elapsed_seconds >= interval_seconds {
                    elapsed_seconds = 0;
                    run_policy_cycle(&status_tx, &schedule);
                }
            }

            std::thread::sleep(Duration::from_secs(POLICY_POLL_SECONDS));
        }
    });
}

fn run_policy_cycle(status_tx: &std::sync::mpsc::Sender<String>, schedule: &BackupSchedule) {
    let stacks = discover_stacks();
    if stacks.is_empty() {
        let _ = status_tx.send("POLICY no stack IP targets resolved for backup cycle".to_string());
        return;
    }

    let _ = status_tx.send(format!(
        "POLICY starting scheduled backup cycle for {} stack(s)",
        stacks.len()
    ));

    let Some(results) = run_backup_cycle_owned_guarded(stacks.clone()) else {
        let _ = status_tx.send("POLICY skipped: backup cycle already running".to_string());
        return;
    };

    for result in &results {
        if result.backup_ok {
            let repo = format!("{}/{}", restic_repo_base(), result.stack);
            let retention_ok = run_restic_retention(&repo, schedule);
            let _ = status_tx.send(format!(
                "POLICY [{}] backup=ok retention={} pause={} resume={}",
                result.stack,
                if retention_ok { "ok" } else { "fail" },
                if result.paused { "ok" } else { "err" },
                if result.resumed { "ok" } else { "err" }
            ));
        } else {
            let _ = status_tx.send(format!(
                "POLICY [{}] backup=fail pause={} resume={} msg={}",
                result.stack,
                if result.paused { "ok" } else { "err" },
                if result.resumed { "ok" } else { "err" },
                result.message
            ));
        }
    }
}

fn discover_stacks() -> Vec<(String, String)> {
    KNOWN_STACKS
        .iter()
        .filter_map(|stack| {
            let env_key = format!("LXC_{}_IP", stack.replace('-', "_").to_uppercase());
            std::env::var(&env_key)
                .ok()
                .or_else(|| std::env::var("LXC_API_IP").ok())
                .map(|ip| ((*stack).to_string(), ip))
        })
        .collect()
}

fn load_schedule() -> BackupSchedule {
    let path = schedule_path();
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return BackupSchedule::default(),
    };
    serde_json::from_str(&raw).unwrap_or_else(|_| BackupSchedule::default())
}

fn schedule_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".config")
        .join("homelab")
        .join("backup-schedule.json")
}

fn backup_one_stack(client: &reqwest::blocking::Client, stack: &str, lxc_ip: &str) -> BackupResult {
    let pause_url = format!("http://{}:{}/api/backup/pause", lxc_ip, LXC_API_PORT);
    let resume_url = format!("http://{}:{}/api/backup/resume", lxc_ip, LXC_API_PORT);
    let appdata_path = format!("{}/{}", appdata_base(), stack);
    let restic_repo = format!("{}/{}", restic_repo_base(), stack);

    // ── Step 1: pause ──────────────────────────────────────────────────────
    let paused = match client.post(&pause_url).send() {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    };

    // ── Step 2: Restic backup (always runs, even if pause failed) ──────────
    // A Drop Guard ensures resume is sent even if this function panics.
    let _guard = ResumeGuard {
        client,
        url: &resume_url,
    };

    let backup_ok = run_restic(&restic_repo, &appdata_path);

    // ── Step 3: resume is sent by the Drop Guard when _guard drops ─────────
    // We drop it explicitly here so we can capture the result.
    let resumed = _guard.send_now();
    std::mem::forget(_guard); // prevent double-send on drop

    let message = if backup_ok {
        format!("Backup OK — repo: {}", restic_repo)
    } else {
        format!("Restic failed — check logs for {}", stack)
    };

    BackupResult {
        stack: stack.to_string(),
        lxc_ip: lxc_ip.to_string(),
        paused,
        backup_ok,
        resumed,
        message,
    }
}

/// Runs `restic backup <source>` into the given repository path.
/// Returns true on success.
fn run_restic(repo: &str, source: &str) -> bool {
    let output = Command::new("restic")
        .args([
            "-r",
            repo,
            "backup",
            source,
            "--compression",
            "max",
            "--tag",
            "homelab-auto",
        ])
        .output();

    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

fn run_restic_retention(repo: &str, schedule: &BackupSchedule) -> bool {
    let output = Command::new("restic")
        .args([
            "-r",
            repo,
            "forget",
            "--keep-daily",
            &schedule.retention_daily.to_string(),
            "--keep-weekly",
            &schedule.retention_weekly.to_string(),
            "--keep-monthly",
            &schedule.retention_monthly.to_string(),
            "--prune",
        ])
        .output();

    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

fn restic_repo_base() -> String {
    std::env::var("RESTIC_REPO_BASE")
        .or_else(|_| std::env::var("RESTIC_REPOSITORY"))
        .unwrap_or_else(|_| "/backups".to_string())
}

fn appdata_base() -> String {
    std::env::var("APPDATA_BASE").unwrap_or_else(|_| "/opt/appdata".to_string())
}

/// Prevents containers from staying paused if something panics mid-backup.
struct ResumeGuard<'a> {
    client: &'a reqwest::blocking::Client,
    url: &'a str,
}

impl<'a> ResumeGuard<'a> {
    /// Send the resume signal and return whether it succeeded.
    fn send_now(&self) -> bool {
        self.client
            .post(self.url)
            .send()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

impl<'a> Drop for ResumeGuard<'a> {
    fn drop(&mut self) {
        let _ = self.send_now();
    }
}
