//! The recorded state of the watched backups (decision "Fleet check speed",
//! 2026-09-29: backup-read = reuse).
//!
//! `homelab check` used to list every watched backup on Google Drive each
//! time it ran: the first check stage took 10.4 s on pve, nearly all of it
//! rclone. The host now keeps what it learned instead, in a file beside
//! `state.json` so it survives a restart:
//!
//! - the nightly round lists every watched backup and records the answer;
//! - every device backup this suite makes records "newest = now" for each
//!   watcher that points at the repository it just wrote;
//! - the fleet check (and `homelab today`) reads the record, and lists only
//!   a watcher that has no record yet (the first run after the upgrade, or a
//!   watcher whose path changed), recording that answer.
//!
//! A backup made or removed outside this suite shows after the next nightly
//! round. A listing that failed is never recorded: the last good answer
//! stays, and its age keeps growing until the next one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::executor::Executor;

/// The file, in the host's state directory.
pub const WATCHED_BACKUPS_FILE: &str = "watched-backups.json";

/// What was last learned about one watched backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchedRecord {
    /// The path the answer is about; a watcher moved elsewhere is asked anew.
    pub rclone_path: String,
    /// The newest file's time; `None` = the listing worked and held no files.
    pub newest_unix: Option<u64>,
    /// When this was learned.
    pub learned_at: u64,
    /// `nightly`, `check` (first read) or `device-backup <name>`.
    pub source: String,
}

/// Keyed by watcher name.
pub type WatchedRecords = BTreeMap<String, WatchedRecord>;

fn path(state_dir: &str) -> String {
    format!("{}/{}", state_dir, WATCHED_BACKUPS_FILE)
}

/// The recorded answers; a missing or unreadable file is an empty record,
/// which only means the next check asks once.
pub async fn load(exec: &dyn Executor, state_dir: &str) -> WatchedRecords {
    exec.read_file(&path(state_dir))
        .await
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Write the records back. Best effort: a failed write costs one listing on
/// the next check, nothing more.
pub async fn save(exec: &dyn Executor, state_dir: &str, records: &WatchedRecords) {
    if let Ok(raw) = serde_json::to_string_pretty(records) {
        let _ = exec.write_file(&path(state_dir), &raw, 0o644).await;
    }
}

/// The recorded answer for this watcher, if it is about the same path.
pub fn recorded<'a>(
    records: &'a WatchedRecords,
    name: &str,
    rclone_path: &str,
) -> Option<&'a WatchedRecord> {
    records.get(name).filter(|r| r.rclone_path == rclone_path)
}

/// Does a watcher at `rclone_path` watch the repository the device backup
/// `device` writes, `<restic_base>/<device>-config`?
///
/// F259: `restic_base` carries restic's `rclone:` prefix and an rclone path
/// does not; a watcher usefully points at `<repo>/snapshots` rather than the
/// repo root. So the prefix is normalised and the repo or anything under it
/// matches.
pub fn feeds(rclone_path: &str, restic_base: &str, device: &str) -> bool {
    let norm = |p: &str| {
        p.trim_start_matches("rclone:")
            .trim_end_matches('/')
            .to_string()
    };
    let repo = norm(&format!("{}/{}-config", restic_base, device));
    let path = norm(rclone_path);
    path == repo || path.starts_with(&format!("{}/", repo))
}

/// A device backup this suite just made: every watcher on its repository
/// now has a file from `now`. Returns the watchers it recorded.
pub async fn record_device_backup(
    exec: &dyn Executor,
    state_dir: &str,
    watchers: &[(String, String)],
    restic_base: &str,
    device: &str,
    now: u64,
) -> Vec<String> {
    let fed: Vec<&(String, String)> = watchers
        .iter()
        .filter(|(_, p)| feeds(p, restic_base, device))
        .collect();
    if fed.is_empty() {
        return Vec::new();
    }
    let mut records = load(exec, state_dir).await;
    for (name, p) in &fed {
        records.insert(
            name.clone(),
            WatchedRecord {
                rclone_path: p.clone(),
                newest_unix: Some(now),
                learned_at: now,
                source: format!("device-backup {}", device),
            },
        );
    }
    save(exec, state_dir, &records).await;
    fed.into_iter().map(|(n, _)| n.clone()).collect()
}
