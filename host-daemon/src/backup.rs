//! Backup orchestration — pause each LXC's containers via HTTP API, run Restic,
//! then resume containers. Uses blocking reqwest since the HOST daemon is synchronous.

use std::process::Command;
use std::time::Duration;

const LXC_API_PORT: u16 = 8080;
const RESTIC_REPO_BASE: &str = "/backups";
const APPDATA_BASE: &str = "/opt/appdata";

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

fn backup_one_stack(
    client: &reqwest::blocking::Client,
    stack: &str,
    lxc_ip: &str,
) -> BackupResult {
    let pause_url = format!("http://{}:{}/api/backup/pause", lxc_ip, LXC_API_PORT);
    let resume_url = format!("http://{}:{}/api/backup/resume", lxc_ip, LXC_API_PORT);
    let appdata_path = format!("{}/{}", APPDATA_BASE, stack);
    let restic_repo = format!("{}/{}", RESTIC_REPO_BASE, stack);

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
            "-r", repo,
            "backup", source,
            "--compression", "max",
            "--tag", "homelab-auto",
        ])
        .output();

    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

/// Convenience wrapper that accepts owned Strings — suitable for passing to `std::thread::spawn`.
pub fn run_backup_cycle_owned(stacks: Vec<(String, String)>) -> Vec<BackupResult> {
    let refs: Vec<(&str, &str)> = stacks.iter().map(|(s, ip)| (s.as_str(), ip.as_str())).collect();
    run_backup_cycle(&refs)
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
