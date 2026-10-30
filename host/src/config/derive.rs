//! Settings built from several keys at once: the safety list (the file can
//! only widen it) and the backup configuration.

use super::*;
use homelab_core::safety::SafetyConfig;

/// F8: the compiled no-touch list is law; the file can only add to it.
pub(crate) fn safety_config(file: &FileConfig) -> SafetyConfig {
    let mut sc = SafetyConfig::default();
    // F8: the file ADDS to the compiled list, it does not replace it.
    // It used to assign, so `no_touch = [200]` in host.toml would
    // have quietly dropped Home Assistant and the router out of
    // protection — a typo away from making the two machines this
    // project may never touch touchable. The list is law and its home
    // is `core/src/safety.rs`; config can only widen it.
    if let Some(list) = file.no_touch.clone() {
        for vmid in list {
            if !sc.no_touch.contains(&vmid) {
                sc.no_touch.push(vmid);
            }
        }
    }
    if let Some(gw) = file.gateway_vmid {
        sc.gateway_vmid = gw;
    }
    if let Some(dir) = file.gateway_routes_dir.clone() {
        sc.gateway_routes_dir = dir;
    }
    // fix-120: the daemon always runs with a policy; without the
    // keys it is what the fleet uses today, so nothing is refused
    // that deployed yesterday.
    sc.privileged_vmids = Some(
        file.privileged_vmids
            .clone()
            .unwrap_or_else(|| homelab_core::safety::FLEET_PRIVILEGED_VMIDS.to_vec()),
    );
    sc.data_mount_roots = Some(file.data_mount_roots.clone().unwrap_or_else(|| {
        homelab_core::safety::FLEET_DATA_MOUNT_ROOTS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }));
    sc
}

/// The restic locations and timeouts, every one defaulting to the fleet's.
pub(crate) fn backup_config(file: &FileConfig) -> homelab_core::ops::backup::BackupCfg {
    let d = homelab_core::ops::backup::BackupCfg::default();
    homelab_core::ops::backup::BackupCfg {
        restic_base: file.restic_base.clone().unwrap_or(d.restic_base),
        password_file: file.restic_password_file.clone().unwrap_or(d.password_file),
        snapshot_timeout_s: file
            .restic_snapshot_timeout_s
            .unwrap_or(d.snapshot_timeout_s),
        restore_timeout_s: file.restic_restore_timeout_s.unwrap_or(d.restore_timeout_s),
        // Unset: the default staging dir. "none" or empty: no staging.
        staging_dir: match file.native_backup_staging_dir.clone() {
            None => d.staging_dir,
            Some(s) if s.trim().is_empty() || s.trim() == "none" => None,
            Some(s) => Some(s),
        },
        staging_cap_mib: file
            .native_backup_staging_cap_mib
            .unwrap_or(d.staging_cap_mib),
        tiers: d.tiers,
        trigger: d.trigger,
        // backup-local-first: the second repository set is where a
        // compose stack's snapshot is written first.
        local_dataset: file.second_copy_dataset.clone(),
        stall_s: file.backup_stall_s.unwrap_or(d.stall_s),
    }
}
