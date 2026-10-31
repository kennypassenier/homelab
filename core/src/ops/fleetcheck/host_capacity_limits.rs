//! Warn/critical thresholds for the host-level capacity readings.

use serde::{Deserialize, Serialize};

use super::HostCapacityMetric;

/// Warn/critical pairs, one per `HostCapacityMetric`. A host.toml key
/// (`capacity_thresholds`), editable from the dashboard like every other
/// fleet default. Defaults are the disk-growth audit's own numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCapacityThresholds {
    pub pve_root_warn_pct: u8,
    pub pve_root_critical_pct: u8,
    pub thin_data_warn_pct: u8,
    pub thin_data_critical_pct: u8,
    pub thin_meta_warn_pct: u8,
    pub thin_meta_critical_pct: u8,
    pub zfs_pool_warn_pct: u8,
    pub zfs_pool_critical_pct: u8,
    /// Of pve journald's own `SystemMaxUse` cap (2G, `hostunits::JOURNALD_CAP`).
    pub journald_warn_pct: u8,
    pub journald_critical_pct: u8,
    /// Of Prometheus' configured `--storage.tsdb.retention.size`.
    pub tsdb_warn_pct: u8,
    pub tsdb_critical_pct: u8,
    /// Of `native_backup_staging_cap_mib` — low on purpose: the directory is
    /// emptied after every run, so ANY leftover is already worth a look,
    /// well before it could ever fill the cap.
    pub native_backup_staging_warn_pct: u8,
    pub native_backup_staging_critical_pct: u8,
}

impl Default for HostCapacityThresholds {
    fn default() -> Self {
        Self {
            pve_root_warn_pct: 70,
            pve_root_critical_pct: 85,
            thin_data_warn_pct: 70,
            thin_data_critical_pct: 85,
            // Lower and narrower than data: metadata exhaustion takes the
            // pool down in a way data filling up does not.
            thin_meta_warn_pct: 50,
            thin_meta_critical_pct: 70,
            zfs_pool_warn_pct: 80,
            zfs_pool_critical_pct: 90,
            journald_warn_pct: 80,
            journald_critical_pct: 95,
            tsdb_warn_pct: 70,
            tsdb_critical_pct: 90,
            // 10% of a 10 GiB default cap is already 1 GiB sitting where
            // nothing should be between runs.
            native_backup_staging_warn_pct: 10,
            native_backup_staging_critical_pct: 50,
        }
    }
}

impl HostCapacityThresholds {
    pub(super) fn pair(self, metric: HostCapacityMetric) -> (u8, u8) {
        match metric {
            HostCapacityMetric::PveRoot => (self.pve_root_warn_pct, self.pve_root_critical_pct),
            HostCapacityMetric::ThinPoolData => {
                (self.thin_data_warn_pct, self.thin_data_critical_pct)
            }
            HostCapacityMetric::ThinPoolMeta => {
                (self.thin_meta_warn_pct, self.thin_meta_critical_pct)
            }
            HostCapacityMetric::ZfsPool => (self.zfs_pool_warn_pct, self.zfs_pool_critical_pct),
            HostCapacityMetric::Journald => (self.journald_warn_pct, self.journald_critical_pct),
            HostCapacityMetric::PrometheusTsdb => (self.tsdb_warn_pct, self.tsdb_critical_pct),
            HostCapacityMetric::NativeBackupStaging => (
                self.native_backup_staging_warn_pct,
                self.native_backup_staging_critical_pct,
            ),
        }
    }
}
