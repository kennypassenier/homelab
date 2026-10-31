//! Host-level capacity: pve's root, the thin pool, ZFS, journald, TSDB.

use serde::{Deserialize, Serialize};

use super::{Finding, HostCapacityThresholds, Severity};

// ── rule-20: host-level capacity thresholds ────────────────────────────────
//
// Disk-growth audit (2026-10-01): measurable today but unalarmed — pve's own
// root filesystem, the local-lvm thin pool's data and metadata percentages,
// every ZFS pool, and pve's journald against its own 2G cap
// (`hostunits::JOURNALD_CAP`). One evaluator for all of them, generic over
// which metric a fact names, the same shape as `evaluate_pools` beside it:
// the shell measures, this only judges.

/// Which host-level capacity reading a `HostCapacityFact` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostCapacityMetric {
    /// `/` on pve itself.
    PveRoot,
    /// local-lvm thin pool, data%.
    ThinPoolData,
    /// local-lvm thin pool, metadata% — harder to recover from than data%,
    /// so it gets its own, lower, pair of thresholds.
    ThinPoolMeta,
    /// One ZFS pool, named in `subject`.
    ZfsPool,
    /// pve's own journald, against its own `SystemMaxUse` cap
    /// (`hostunits::JOURNALD_CAP`) — distinct from a managed container's own
    /// small cap, which `GrowthFact::journal_mb` already covers.
    Journald,
    /// Prometheus' TSDB against its configured `--storage.tsdb.retention.size`.
    PrometheusTsdb,
    /// The homelab daemon's own native-backup staging directory
    /// (`native_backup_staging_dir`, default `/appdata/.backup-staging`)
    /// against `native_backup_staging_cap_mib` — it is emptied after every
    /// run, so anything here at all past a run is itself the finding.
    NativeBackupStaging,
}

impl HostCapacityMetric {
    fn label(self) -> &'static str {
        match self {
            Self::PveRoot => "pve's root filesystem",
            Self::ThinPoolData => "the local-lvm thin pool (data)",
            Self::ThinPoolMeta => "the local-lvm thin pool (metadata)",
            Self::ZfsPool => "ZFS pool",
            Self::Journald => "pve's journald",
            Self::PrometheusTsdb => "Prometheus' TSDB",
            Self::NativeBackupStaging => "the native-backup staging directory",
        }
    }
    fn remedy(self) -> &'static str {
        match self {
            Self::PveRoot => {
                "free space on pve itself — apt cache, old kernels, /var/lib/vz ISOs and \
                 templates — or grow pve/root"
            }
            Self::ThinPoolData => {
                "an LXC or VM disk write can fail once this pool is full — free space (prune \
                 old container disks/snapshots) or extend the pool"
            }
            Self::ThinPoolMeta => {
                "metadata exhaustion is harder to recover from than data: `lvextend \
                 --poolmetadatasize` ahead of time, not after"
            }
            Self::ZfsPool => "`zfs list -o name,used,avail` on the host — free space or add a vdev",
            Self::Journald => {
                "journald rotates its own oldest entries past this, but it is close to the cap \
                 all the time — `journalctl --vacuum-size` or raise SystemMaxUse in \
                 hostunits.rs"
            }
            Self::PrometheusTsdb => {
                "retention.size is the ceiling, not a target — either it is doing its job and \
                 this is expected, or retention.time is now too generous for the ceiling"
            }
            Self::NativeBackupStaging => {
                "it is emptied after every run — this means the last run did not finish \
                 cleanly; check the nightly log, then clear it by hand if it is stale"
            }
        }
    }
}

/// One host-level capacity reading, judged against `HostCapacityThresholds`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCapacityFact {
    pub metric: HostCapacityMetric,
    /// What to call it in the report — "pve", a ZFS pool's name, "Prometheus
    /// (CT 113)". `metric` already says WHAT kind of reading this is; this
    /// says WHICH one.
    pub subject: String,
    pub used_pct: u8,
    /// Extra detail for the message, e.g. "27.66% of 794.3G pool" — numbers
    /// Kenny can act on beside the bare percentage.
    pub detail: String,
    /// fix-181: only meaningful for `HostCapacityMetric::Journald` — whether
    /// a `SystemMaxUse` cap is actually in force (the
    /// `hostunits::JOURNALD_CAP` drop-in is present on disk). A capped
    /// journal sits near its cap *by design* (journald rotates its oldest
    /// entries to stay under it), so `used_pct` close to or at 100% of the
    /// cap is the healthy steady state, not drift — the percentage-threshold
    /// judgment below only applies while no cap is proven to exist. Every
    /// other metric always has a "cap" (the filesystem, the pool, the
    /// retention setting) so this is simply `true` for them.
    pub cap_configured: bool,
}

/// rule-20: a finding per host-level capacity reading past its warn or
/// critical threshold. Pure — the shell measures, this only judges.
pub fn evaluate_host_capacity(
    facts: &[HostCapacityFact],
    lim: HostCapacityThresholds,
) -> Vec<Finding> {
    facts
        .iter()
        .filter_map(|f| {
            if f.metric == HostCapacityMetric::Journald {
                journald_finding(f)
            } else {
                threshold_finding(f, lim)
            }
        })
        .collect()
}

/// fix-181: a capped journal sits near 100% of its cap all the time — that
/// is journald rotating, not drift. Judge it on whether the cap exists and
/// holds, not on a percentage scale built for things that are broken long
/// before they are full.
fn journald_finding(f: &HostCapacityFact) -> Option<Finding> {
    if !f.cap_configured {
        Some(Finding {
            severity: Severity::Broken,
            subject: f.subject.clone(),
            what: format!(
                "{} has no SystemMaxUse cap configured — it is {}",
                f.metric.label(),
                f.detail
            ),
            remedy: "the host's own journald cap drop-in \
                     (hostunits::JOURNALD_CAP) is missing — \
                     `homelab self-update` or `homelab doctor` \
                     should have put it back"
                .into(),
        })
    } else if f.used_pct > 100 {
        Some(Finding {
            severity: Severity::Broken,
            subject: f.subject.clone(),
            what: format!("{} is over its own cap — {}", f.metric.label(), f.detail),
            remedy: "a cap is set but journald is not rotating within it — \
                     `journalctl --vacuum-size` now, and check that \
                     systemd-journald actually restarted after the cap \
                     was written"
                .into(),
        })
    } else {
        None
    }
}

/// Broken at the critical percentage, Drift at the warn one.
fn threshold_finding(f: &HostCapacityFact, lim: HostCapacityThresholds) -> Option<Finding> {
    let (warn, critical) = lim.pair(f.metric);
    let severity = if f.used_pct >= critical {
        Severity::Broken
    } else if f.used_pct >= warn {
        Severity::Drift
    } else {
        return None;
    };
    Some(Finding {
        severity,
        subject: f.subject.clone(),
        what: format!("{} is {}", f.metric.label(), f.detail),
        remedy: f.metric.remedy().into(),
    })
}
