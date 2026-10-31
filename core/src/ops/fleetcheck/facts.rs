//! What the shell reads off the machine so the comparison can stay pure.

use super::{
    BigLogFact, BootFact, CoverageFact, FirewallFact, GrowthFact, HostCapacityFact, OwnerFact,
    PatchFact, PoolFact, RouteFact, StackDigest, StackSnapshotFact, WatchedBackupFact,
};

/// What the shell reads off the machine so the comparison can stay pure.
#[derive(Debug, Clone, Default)]
pub struct LiveFacts {
    /// vmid → hostname, for every container that exists on the hypervisor.
    pub containers: Vec<(u16, String)>,
    /// checks-automate: each registered probe, read in its container.
    pub probe_readings: Vec<crate::ops::probes::ProbeReading>,
    /// Route file name → the address it forwards to, and whether anything
    /// answered there.
    pub routes: Vec<RouteFact>,
    /// Stack directories found in the repository, with the vmid they claim.
    pub stack_files: Vec<(String, u16)>,
    /// G3: what every managed container's resources look like right now.
    pub growth: Vec<GrowthFact>,
    /// Whether each managed stack's safety nets are actually attached.
    pub coverage: Vec<CoverageFact>,
    /// W3: what `pct config` says about boot policy and resources, per
    /// managed container.
    pub boot: Vec<BootFact>,
    /// F184: what the HOST itself has, so a per-container remedy cannot
    /// advise something the machine cannot give. `(total_mb, committed_mb,
    /// swap_used_mb, swap_total_mb)`, or None when it could not be read —
    /// which is not the same as a healthy host and is treated as unknown.
    pub host_memory: Option<(u32, u32, u32, u32)>,
    /// O1: backups made OUTSIDE this suite that it nevertheless watches —
    /// today the router's own nightly upload to Google Drive. Empty = none
    /// declared, which is what every fleet had before this existed.
    pub watched_backups: Vec<WatchedBackupFact>,
    /// R13: how full the pools are that the stacks keep their data on. Empty
    /// when no stack declares a data mount, which is what every fleet looked
    /// like before this existed.
    pub pools: Vec<PoolFact>,
    /// fix-24: log files on the declared data mounts above the size a
    /// rotated log never reaches. Empty when none, and when none was read.
    pub big_logs: Vec<BigLogFact>,
    /// fix-26: storage directories whose owner on disk is not the declared
    /// `host_owner_uid`. Only mismatches are kept.
    pub owners: Vec<OwnerFact>,
    /// fix-92: every file name in the gateway's routes directory, whatever
    /// its extension. Empty when none, and when it could not be read.
    pub route_files: Vec<String>,
    /// fix-88: each recorded stack's `/etc/pve/firewall/<vmid>.fw`.
    pub firewalls: Vec<FirewallFact>,
    /// fix-150: the patch state of every managed container that answered.
    pub patch: Vec<PatchFact>,
    /// fix-96: the configured `second_copy_dataset`, or None when no second
    /// copy is configured (and then nothing is said about one).
    pub second_copy_dataset: Option<String>,
    /// fix-142: what the client's stack files say, one digest per stack.
    /// Empty from a client older than this field, and in the nightly round,
    /// which has no repository; the repository comparison is then skipped.
    pub digests: Vec<StackDigest>,
    /// fix-142: the host's intent-history copy of each stack the client sent,
    /// stack → container-bound path → sha256. A stack with no copy is absent.
    pub intent_files:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    /// rule-20 (disk-audit, 2026-10-01): host-level capacity readings that
    /// are neither a managed container's own rootfs (`growth`) nor a
    /// stack's declared data pool (`pools`) — pve's own root filesystem,
    /// the local-lvm thin pool, pve's journald against its own cap, and
    /// every ZFS pool on the host. Empty when none were read.
    pub host_capacity: Vec<HostCapacityFact>,
    /// fix-110: `config/host.toml` as the client's (or the dashboard's)
    /// working copy reads it, non-secret keys only. `None` from an older
    /// client, the nightly round, or a repository that does not carry the
    /// file yet — the comparison is then skipped, same as `digests` being
    /// empty skips the stack-file comparison.
    pub declared_host_config: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    /// fix-110: the same non-secret keys as the host's OWN host.toml sets
    /// them right now — gathered host-side, the way `GetHostConfig` reads
    /// them, so the comparison is file-to-file rather than against a
    /// resolved-with-defaults view nobody else sees. Empty when the host
    /// sets none of them (every key then uses its compiled default).
    pub live_host_config: std::collections::BTreeMap<String, serde_json::Value>,
    /// fix-142 (nightly hash comparison, Kenny's go 2026-10-01): every
    /// recorded stack's `/opt/<stack>/` hashed inside its container right
    /// now, stack → manifest path → sha256 — gathered host-side, nightly
    /// only (like `digests` from the repository side, this is empty on a
    /// `homelab check` the host did not run itself). Compared against
    /// `StackState::pushed_file_hashes` by `evaluate_container_drift`.
    pub container_file_hashes:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    /// fix-218: each recorded stack's REAL backup freshness, read off the
    /// same `SnapshotCache` `homelab snapshots` answers from — not
    /// `StackState::last_backup`, which a vacuous "no services" nightly
    /// success can advance with no repository ever created (measured live,
    /// host 3.70.4: doctor said "last backup 11h ago" for stack inbox while
    /// `homelab snapshots stacks/inbox` answered "no repositories"). Empty
    /// from a caller that never gathered it (a nightly round reading only
    /// what it needs) — then `evaluate` falls back to silence for backup
    /// age, same as `digests` being empty skips the stack-file comparison.
    pub backup_snapshots: Vec<StackSnapshotFact>,
}
