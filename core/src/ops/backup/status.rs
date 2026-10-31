//! Repository status for the Backups page, `homelab snapshots` and doctor.

use crate::error::CoreError;
use crate::executor::Executor;
use crate::manifest::StackManifest;

use super::config::BackupCfg;
use super::owners::owner_groups;
use super::restic::restic_cmd;
use super::snapshot_parse::{SnapRun, parse_snapshot_runs};

// ── feat-backup-1/2/3: status, snapshot list, snapshot browse ─────────────

/// feat-backup-1: one repository's status on the Backups page — D25's
/// owning-app repository, its newest snapshot, how many it holds, its size
/// on the remote, and the last restore-drill verdict recorded for it
/// (`state::DrillRecord`, keyed the same way the drill itself keys it: by
/// repository name).
// fix-64: Deserialize added so the CLI (`homelab snapshots`) can parse the
// host's GetBackups reply; the dashboard, which only ever serialized this
// to JSON for its own JS, did not need it before.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepoStatus {
    /// D25: the owning app (compose) or unit (native) — the restic
    /// repository is named `<base>/<owner>-config`.
    pub owner: String,
    /// None when the repository does not exist yet (a stack never backed
    /// up) or could not be read — `error` then says why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newest_snapshot: Option<SnapRun>,
    pub snapshot_count: usize,
    /// feat-backup-2: every snapshot of this repository, newest first — the
    /// restore dialog's own picker reads this (one round trip, no separate
    /// "list snapshots" call); the Backups page itself only ever shows
    /// `newest_snapshot`.
    pub snapshots: Vec<SnapRun>,
    /// `restic stats latest --json`'s `total_size`; None when that call
    /// failed (the status as a whole is not failed for it — a size is a
    /// nicety, not a safety fact).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drill: Option<crate::state::DrillRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// fix-180: unix seconds this status was actually read from restic,
    /// `None` when it came from `repo_status_of`'s own live read (every
    /// call site but the host's cached `GetBackups` answer, which fills
    /// this from the snapshot cache's `measured_at` — the UI's "read N min
    /// ago").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured_at: Option<u64>,
}

/// feat-backup-1: `restic snapshots --json` for one repository, parsed and
/// newest-first — the data both the status card and the restore snapshot
/// picker read.
pub async fn list_snapshots(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    owner: &str,
) -> Result<Vec<SnapRun>, CoreError> {
    let out = exec
        .run(&restic_cmd(cfg, owner, &["snapshots", "--json"], 120))
        .await?;
    // exit 10: repository does not exist (never backed up) — an empty list,
    // not an error; anything else that fails IS an error, same rule
    // `restore_empty_unit` already uses for this exact distinction.
    if !out.success() {
        if out.code == 10 {
            return Ok(Vec::new());
        }
        return Err(CoreError::Other(format!(
            "restic snapshots for '{}' failed (exit {}): {}",
            owner,
            out.code,
            out.stderr.trim()
        )));
    }
    let mut snaps = parse_snapshot_runs(&out.stdout);
    snaps.sort_by_key(|s| std::cmp::Reverse(s.time));
    Ok(snaps)
}

/// feat-backup-1: one repository's size on the remote, from `restic stats`.
/// `None` on any failure — a status page shows "unknown" rather than
/// refusing to show the rest of the row.
///
/// `pub(crate)` since fix-180: `snapshot_cache` folds this into the same
/// cached entry as `list_snapshots`, so a cache hit never needs a second
/// restic call just for the size.
pub(crate) async fn repo_size_bytes(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    owner: &str,
) -> Option<u64> {
    let out = exec
        .run(&restic_cmd(cfg, owner, &["stats", "latest", "--json"], 120))
        .await
        .ok()?;
    if !out.success() {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(out.stdout.trim())
        .ok()?
        .get("total_size")
        .and_then(|n| n.as_u64())
}

/// feat-backup-1: every repository a stack's manifest declares (D25's
/// `owner_groups`), with its status. One repository's failure to answer
/// does not hide the others — their `error` field says so individually.
pub async fn backup_status(
    exec: &dyn Executor,
    m: &StackManifest,
    cfg: &BackupCfg,
    drills: &std::collections::BTreeMap<String, crate::state::DrillRecord>,
) -> Vec<RepoStatus> {
    let mut out = Vec::new();
    for (owner, _paths) in owner_groups(m) {
        out.push(repo_status_of(exec, cfg, drills, owner).await);
    }
    out
}

/// feat-backup-1: the same, for one already-known repository name (a native
/// unit, whose "manifest" is `NativeServiceManifest`, not `StackManifest`,
/// so it cannot go through `owner_groups`).
pub async fn repo_status_of(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    drills: &std::collections::BTreeMap<String, crate::state::DrillRecord>,
    owner: String,
) -> RepoStatus {
    match list_snapshots(exec, cfg, &owner).await {
        Ok(snaps) => RepoStatus {
            newest_snapshot: snaps.first().cloned(),
            snapshot_count: snaps.len(),
            size_bytes: repo_size_bytes(exec, cfg, &owner).await,
            drill: drills.get(&owner).cloned(),
            error: None,
            owner,
            snapshots: snaps,
            measured_at: None,
        },
        Err(e) => RepoStatus {
            newest_snapshot: None,
            snapshot_count: 0,
            size_bytes: None,
            drill: drills.get(&owner).cloned(),
            error: Some(e.to_string()),
            owner,
            snapshots: Vec::new(),
            measured_at: None,
        },
    }
}

/// fix-180: one repository's status straight from the host's snapshot
/// cache, no restic call — `None` (a cache miss) means "not read yet", for
/// the caller to say so and kick a background [`SnapshotCache::refresh`].
pub fn repo_status_from_cache(
    cache: &crate::ops::snapshot_cache::CachedRepo,
    drills: &std::collections::BTreeMap<String, crate::state::DrillRecord>,
    owner: String,
) -> RepoStatus {
    RepoStatus {
        newest_snapshot: cache.snapshots.first().cloned(),
        snapshot_count: cache.snapshots.len(),
        size_bytes: cache.size_bytes,
        drill: drills.get(&owner).cloned(),
        error: cache.error.clone(),
        snapshots: cache.snapshots.clone(),
        measured_at: Some(cache.measured_at),
        owner,
    }
}

/// fix-180: a repository the cache has never read — "not read yet" rather
/// than blocking the caller on a restic call that can take minutes.
pub fn repo_status_unread(
    drills: &std::collections::BTreeMap<String, crate::state::DrillRecord>,
    owner: String,
) -> RepoStatus {
    RepoStatus {
        newest_snapshot: None,
        snapshot_count: 0,
        size_bytes: None,
        drill: drills.get(&owner).cloned(),
        error: Some("not read yet".into()),
        snapshots: Vec::new(),
        measured_at: None,
        owner,
    }
}
