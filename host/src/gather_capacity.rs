//! Gathering the host's capacity numbers and disk detail.

use crate::*;

/// H16: parse capacity numbers (C6) from `free -m`, `nproc` and
/// /proc/loadavg + committed RAM from the stored manifests. Pure, testable.
pub(crate) fn capacity_numbers(
    free_out: &str,
    nproc_out: &str,
    loadavg: &str,
    hs: &homelab_core::state::HostState,
) -> (u32, u32, u32, u16, u32) {
    let mem_line: Vec<u64> = free_out
        .lines()
        .find(|l| l.starts_with("Mem:"))
        .map(|l| {
            l.split_whitespace()
                .skip(1)
                .filter_map(|v| v.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let total = mem_line.first().copied().unwrap_or(0) as u32;
    let used = mem_line.get(1).copied().unwrap_or(0) as u32;
    let committed: u32 = hs
        .stacks
        .values()
        .filter_map(|s| s.manifest.as_ref())
        .map(|m| m.resources.memory_mb)
        .sum();
    let cores = nproc_out.trim().parse::<u16>().unwrap_or(0);
    let load1 = loadavg
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| (v * 100.0).round() as u32)
        .unwrap_or(0);
    (total, used, committed, cores, load1)
}

use homelab_core::ops::fleetcheck::{HostCapacityFact, HostCapacityMetric};

/// The journal against its 2 GiB cap (hostunits::JOURNALD_CAP:
/// SystemMaxUse=2G). fix-181: whether that drop-in is actually present is
/// itself a fact to check, not an assumption — `homelab doctor` already treats
/// its absence as drift (hostunits::drift), and a rebuilt or hand-edited host
/// can be without it between a doctor run and the next self-update.
async fn journald(exec: &dyn Executor) -> Vec<HostCapacityFact> {
    let Ok(o) = exec
        .run(&Cmd::new("journalctl", &["--disk-usage"], 15))
        .await
    else {
        return Vec::new();
    };
    let Some(used_mib) = homelab_core::ops::fleetcheck::parse_journal_disk_usage_mib(&o.stdout)
    else {
        return Vec::new();
    };
    let cap_configured = exec
        .run(&Cmd::new(
            "test",
            &["-f", homelab_core::hostunits::JOURNALD_CAP],
            10,
        ))
        .await
        .is_ok_and(|o| o.success());
    let cap_mib: u64 = 2048;
    let pct = ((used_mib * 100) / cap_mib).min(255) as u8;
    vec![fact(
        HostCapacityMetric::Journald,
        "pve",
        pct,
        format!("{used_mib} MiB of its {cap_mib} MiB cap ({pct}%)"),
        cap_configured,
    )]
}

/// Prometheus' own TSDB against its configured retention size.
async fn prometheus_tsdb(
    exec: &dyn Executor,
    prometheus_url: Option<&str>,
    cap_mib: Option<u64>,
) -> Vec<HostCapacityFact> {
    let (Some(base), Some(cap_mib)) = (prometheus_url, cap_mib) else {
        return Vec::new();
    };
    let q = format!(
        "{}/api/v1/query?query=sum(prometheus_tsdb_storage_blocks_bytes)",
        base.trim_end_matches('/')
    );
    let Ok(o) = exec
        .run(&Cmd::new("curl", &["-s", "-m", "10", &q], 20))
        .await
    else {
        return Vec::new();
    };
    let Some(bytes) = homelab_core::ops::fleetcheck::parse_prometheus_scalar(&o.stdout) else {
        return Vec::new();
    };
    let used_mib = (bytes / 1024.0 / 1024.0).round() as u64;
    let pct = ((used_mib * 100) / cap_mib.max(1)).min(255) as u8;
    vec![fact(
        HostCapacityMetric::PrometheusTsdb,
        "Prometheus",
        pct,
        format!("{used_mib} MiB of its {cap_mib} MiB retention.size ({pct}%)"),
        true,
    )]
}

/// rule-20 (coordinator, 2026-10-01): the native-backup staging directory is
/// emptied after every run (backup/config.rs, DEFAULT_STAGING_DIR) — so anything
/// found here at all is worth a look well before it could ever fill its own
/// cap, which is why its thresholds are far lower than every other reading.
async fn native_staging(
    exec: &dyn Executor,
    dir: Option<&str>,
    cap_mib: u64,
) -> Vec<HostCapacityFact> {
    let Some(dir) = dir else {
        return Vec::new();
    };
    let Ok(o) = exec.run(&Cmd::new("du", &["-sm", dir], 30)).await else {
        return Vec::new();
    };
    let Some(used_mib) = homelab_core::ops::fleetcheck::parse_du_sm(&o.stdout) else {
        return Vec::new();
    };
    let cap = cap_mib.max(1);
    let pct = ((used_mib * 100) / cap).min(255) as u8;
    vec![fact(
        HostCapacityMetric::NativeBackupStaging,
        dir,
        pct,
        format!("{used_mib} MiB of its {cap} MiB cap ({pct}%) — it should be empty between runs"),
        true,
    )]
}

/// rule-20 (disk-audit, 2026-10-01): pve's root filesystem, the local-lvm
/// thin pool (data and metadata), every ZFS pool, pve's own journald
/// against its own cap (`hostunits::JOURNALD_CAP`, 2G), and — only when
/// both are configured — Prometheus' TSDB against the cap Kenny declared
/// for it. Every command runs directly on pve (this daemon's own host, not
/// a container); a command that fails or does not parse simply contributes
/// no fact, same as an unasked question.
pub(crate) async fn gather_host_capacity(
    exec: &dyn Executor,
    prometheus_url: Option<&str>,
    tsdb_retention_size_mib: Option<u64>,
    native_backup_staging_dir: Option<&str>,
    native_backup_staging_cap_mib: u64,
) -> Vec<HostCapacityFact> {
    let mut out = pve_root(exec).await;
    out.extend(thin_pool(exec).await);
    out.extend(zfs_pools(exec).await);
    out.extend(journald(exec).await);
    out.extend(prometheus_tsdb(exec, prometheus_url, tsdb_retention_size_mib).await);
    out.extend(
        native_staging(
            exec,
            native_backup_staging_dir,
            native_backup_staging_cap_mib,
        )
        .await,
    );
    out
}
