//! Backup (E1) and restore (E2). Restic runs on the HOST against the stack's
//! `/appdata` paths (data survives container recreation). Repo per stack:
//! `<base>:<stack>-config`. Stateful containers are quiesced during the
//! snapshot via the `com.homelab.backup.pause` label (E4). Restore is a
//! first-class, gated operation.
//!
//! Each file holds one responsibility; this module re-exports the public
//! surface, so callers keep writing `ops::backup::<name>`:
//!
//! - `config`, `restic`, `progress`, `repo_init`: the settings and the restic
//!   commands, output readers and repository creation everything shares.
//! - `owners`, `newest`, `night`, `trigger`, `plan`: who owns which repository,
//!   when the last snapshot was, what a night did, and the fixed step plans.
//! - `job` (with `gates`, `repos`, `quiesce`, `snapshot`, `retention`): one
//!   stack's backup, a step per file.
//! - `restore_*`: restore of a stack or one app, with its safety copy.
//! - `host_meta`: the host's own metadata repository.
//! - `snapshot_parse`, `night_pick`, `status`, `browse`, `snapshot_file`,
//!   `snapshot_target`: reading snapshots without restoring them.

mod browse;
mod config;
mod gates;
mod host_meta;
mod job;
mod newest;
mod night;
mod night_pick;
mod owners;
mod plan;
mod progress;
mod quiesce;
mod repo_init;
mod repos;
mod restic;
mod restore_app;
mod restore_copy;
mod restore_data;
mod restore_into;
mod restore_select;
mod retention;
mod shell;
mod snapshot;
mod snapshot_file;
mod snapshot_parse;
mod snapshot_target;
mod status;
mod trigger;

#[cfg(test)]
mod tests_browse;
#[cfg(test)]
mod tests_progress;

pub use browse::{SnapshotEntry, browse_snapshot, parse_snapshot_ls};
pub use config::{BackupCfg, DEFAULT_STAGING_DIR, DEFAULT_STALL_S, stack_tiers};
pub use host_meta::{HOST_META_BACKUP_STEPS, HOST_META_EXTRAS, backup_host_meta};
pub use job::backup;
pub use newest::snapshot_nights_unix;
pub use night::NightBackup;
pub use night::phase_duration_line;
pub use night_pick::{NightChoice, resolve_night};
pub use owners::{conflicting_owner, owner_groups, repository_keeper, stack_repo_owners};
pub use plan::{BACKUP_STEPS, backup_plan_names, restore_plan_names};
pub use progress::{ResticBytes, copy_packs_done, restic_bytes, snapshot_is_empty};
pub use repo_init::{InitOutcome, init_repository, same_password_as_host_meta};
pub use restic::RESTIC_CACHE_DIR;
pub use restore_app::restore_app;
pub use restore_into::{
    restore, restore_confirmed, restore_into, restore_snapshot_into, restore_with,
};
pub use retention::retention_doomed;
pub use snapshot_file::{
    SNAPSHOT_FILE_CAP, SnapshotFile, parse_snapshot_file, read_snapshot_file, snapshot_file_cmd,
    valid_snapshot_path, valid_snapshot_ref,
};
pub use snapshot_parse::{SnapRun, parse_snapshot_runs};
pub use snapshot_target::{resolve_snapshot_path, snapshot_dirs, snapshot_file_target};
pub use status::{
    RepoStatus, backup_status, list_snapshots, repo_status_from_cache, repo_status_of,
    repo_status_unread,
};
pub use trigger::{BackupTrigger, run_tag};

pub(crate) use job::backup_impl;
pub(crate) use newest::newest_snapshot_unix;
pub(crate) use progress::run_restic_copy;
pub(crate) use restic::{restic, restic_cmd};
pub(crate) use snapshot_parse::parse_snapshots_json;
pub(crate) use status::repo_size_bytes;
