//! The backup configuration and the retention it resolves to.

use super::trigger::BackupTrigger;

/// Rule 20 / coordinator 2026-10-01: staging is on by default, on the same
/// pool as the data it copies (not the root disk), capped by
/// `native_backup_staging_cap_mib` and emptied after every run.
pub const DEFAULT_STAGING_DIR: &str = "/appdata/.backup-staging";

#[derive(Clone)]
pub struct BackupCfg {
    pub restic_base: String,
    /// Path to the restic password file on the host (from the secret store).
    pub password_file: String,
    /// Tiered retention (G8) — computed by us, not restic's --keep-* flags.
    pub tiers: Vec<crate::retention::RetentionTier>,
    /// Snapshot timeout. Hardening H2: the old fixed 1800 s was too small
    /// for a first multi-GB upload over residential rclone/gdrive.
    pub snapshot_timeout_s: u64,
    /// Restore timeout. Was a hardcoded 1800 s while the backup side had
    /// already been raised to four hours for exactly the same reason — so a
    /// large restore over Google Drive died at thirty minutes, on the one
    /// operation you least want to find broken (deployment project, F38).
    pub restore_timeout_s: u64,
    /// fix-113 ADDENDUM (owner + chassis-rs, 2026-10-01): where a
    /// `backup_pause: chassis` native backup tars its data dirs LOCALLY
    /// before restic uploads them, so the chassis pause only has to last as
    /// long as a disk-to-disk copy — never a slow upload. `None`: always tar
    /// straight to restic under a renewed pause (no staging; the only
    /// behaviour possible before this field existed). A directory on a data
    /// pool, not the root disk — `native_backup_staging_dir` in host.toml.
    pub staging_dir: Option<String>,
    /// Largest one staged tar may be, in MiB, BEFORE the 20% safety margin
    /// `fits_staging` adds on top. A copy that would not fit, after the
    /// margin, against either this cap or the directory's free space skips
    /// staging for that run rather than shrinking the margin to fit.
    pub staging_cap_mib: u64,
    /// fix-223: why THIS run is happening — tagged onto every snapshot it
    /// takes (`trigger:<kind>`). The caller sets this before invoking
    /// `backup`/`backup_native`/`backup_impl`; it is not resolved from
    /// context here because the same `BackupCfg` is shared across a batch of
    /// stacks whose call sites already know which trigger they are.
    pub trigger: BackupTrigger,
    /// backup-local-first (Kenny, 2026-10-22: "Eerst lokaal, dan uploaden";
    /// "Bestaande map omdraaien"): the ZFS dataset (`second_copy_dataset`)
    /// whose repositories a compose stack's snapshot is written to FIRST,
    /// while its apps are paused; the copy to `restic_base` follows with the
    /// apps running again. `None`: straight to `restic_base`, as before.
    pub local_dataset: Option<String>,
    /// backup-stall (correction 2026-10-22): how long a snapshot or upload
    /// may go without progress before it is stopped (`executor::Stall`).
    pub stall_s: u64,
}

/// backup-stall: ten minutes without progress (Kenny's "Klopt" on the
/// correction's 10 minutes).
pub const DEFAULT_STALL_S: u64 = 600;

impl Default for BackupCfg {
    fn default() -> Self {
        Self {
            restic_base: "rclone:gdrive:homelab-backups".into(),
            password_file: "/var/lib/homelab/secrets/restic.pw".into(),
            tiers: crate::retention::default_tiers(),
            snapshot_timeout_s: 4 * 3600,
            restore_timeout_s: 4 * 3600,
            staging_dir: Some(DEFAULT_STAGING_DIR.to_string()),
            staging_cap_mib: 10 * 1024,
            trigger: BackupTrigger::default(),
            local_dataset: None,
            stall_s: DEFAULT_STALL_S,
        }
    }
}

/// W2 / fix-113 (native-tar-no-quiesce, 2026-09-27): the retention a stack's
/// repositories are kept by — the stack file's own policy when it states one,
/// else the fleet-wide tiers. `backup` resolved this for compose stacks only;
/// native units always got the fleet-wide tiers.
pub fn stack_tiers(
    state: &crate::state::HostState,
    stack: &str,
    fleet: &[crate::retention::RetentionTier],
) -> Vec<crate::retention::RetentionTier> {
    state
        .stacks
        .get(stack)
        .and_then(|s| s.manifest.as_ref())
        .and_then(|m| m.retention.clone())
        .unwrap_or_else(|| fleet.to_vec())
}
