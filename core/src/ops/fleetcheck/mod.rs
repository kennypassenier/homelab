//! Y4: hold the repository against reality and report every difference.
//!
//! Everything found by hand on 2026-08-30 had one thing in common: none of it
//! was a failure. kyu's stack record still described a container that had been
//! renamed weeks earlier, so its nightly run failed the hostname guard and the
//! stack quietly auto-disabled. A settings key in `host.toml` replaced the
//! compiled no-touch list rather than adding to it, so a code change had no
//! effect. Three stack files claimed vmids that live containers were using. A
//! Traefik route pointed at an empty container and another at a workstation.
//! Uptime Kuma watched exactly one target. Every one of those looked healthy,
//! and the only reason any of them surfaced is that somebody spent a day
//! looking.
//!
//! This module makes the looking a function. The comparison is pure so it can
//! be exercised without a fleet; gathering the facts is the shell's job.

mod backup_age;
mod big_logs;
mod boot;
mod container_drift;
mod coverage;
mod drift_text;
mod evaluate;
mod facts;
mod firewall;
mod firewall_live;
mod growth;
mod host_capacity;
mod host_capacity_limits;
mod host_config_drift;
mod owners;
mod parse_disk;
mod parse_host;
mod patch;
mod pools;
mod recorded;
mod repo_drift;
mod report;
mod routes;
mod stack_record;
mod thin_pool;
mod types;
mod watched_backups;

#[cfg(test)]
mod tests_snapshot_words;

pub use backup_age::{DEFAULT_BACKUP_MAX_AGE_S, StackSnapshotFact};
pub use big_logs::{BIG_LOG_BYTES, BigLogFact, big_log_facts, evaluate_big_logs};
pub use boot::{BootFact, evaluate_boot};
pub use container_drift::evaluate_container_drift;
pub use coverage::{CoverageFact, evaluate_coverage};
pub use drift_text::{
    FILE_DIFF_MAX_LINES, REPO_DRIFT_WHAT_PREFIX, is_repo_file_drift_text, is_secret_path,
    repo_drift_stacks_in_rendered_text,
};
pub use evaluate::evaluate;
pub use facts::LiveFacts;
pub use firewall::evaluate_firewalls;
pub use firewall_live::{FirewallFact, FirewallLiveStatus, firewall_live_status};
pub use growth::{GrowthFact, GrowthLimits, evaluate_growth};
pub use host_capacity::{HostCapacityFact, HostCapacityMetric, evaluate_host_capacity};
pub use host_capacity_limits::HostCapacityThresholds;
pub use host_config_drift::evaluate_host_config_drift;
pub use owners::{OwnerFact, evaluate_owners, owner_facts};
pub use parse_disk::{
    parse_blockdev_bytes, parse_disk_kind, parse_du_top, parse_lv_size_gb, parse_pv_name,
    whole_disk_device,
};
pub use parse_host::{
    parse_df_pcent, parse_du_sm, parse_journal_disk_usage_mib, parse_proc_uptime,
    parse_prometheus_scalar, parse_thin_pool_percents, parse_zpool_capacities,
};
pub use patch::{PATCH_THRESHOLD_S, PatchFact, evaluate_patch_state};
pub use pools::{PoolFact, evaluate_pools, pool_facts_from_df};
pub use recorded::{
    HOST_META_MAX_AGE_S, evaluate_host_meta, evaluate_incomplete, evaluate_notify,
    evaluate_restore_checks,
};
pub use repo_drift::{StackDigest, evaluate_repo_drift};
pub use report::{
    NIGHTLY_REPORT_REPEAT_S, alarming, check_passes, nightly_report_due, render, report_fingerprint,
};
pub use routes::{
    RouteFact, dead_route_findings, evaluate_route_owners, probe_hostport, unowned_route_files,
};
pub use thin_pool::{ThinPoolReading, parse_thin_pool_report};
pub use types::{Finding, Severity};
pub use watched_backups::{WatchedBackupFact, evaluate_watched_backups};
