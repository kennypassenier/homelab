//! Tests for the disk reading (gather_host_disk_detail), moved out of the probes file.

use crate::*;

/// redesign-host-4 (3.71.0): the disk facts the Host page shows beside the
/// root volume — how full the thin pool is, what its volumes are promised,
/// and the root disk's kind — gathered with the fix-222 reading.
#[cfg(test)]
mod redesign_host_4_tests {
    use super::*;
    use homelab_core::executor::{CmdOutput, MockExecutor};

    const LVS_JSON: &str = r#"{"report":[{"lv":[
        {"lv_name":"data","lv_size":"794.30","pool_lv":"","data_percent":"41.20","metadata_percent":"2.53"},
        {"lv_name":"root","lv_size":"96.00","pool_lv":"","data_percent":"","metadata_percent":""},
        {"lv_name":"vm-104-disk-0","lv_size":"8.00","pool_lv":"data","data_percent":"55.10","metadata_percent":""},
        {"lv_name":"vm-113-disk-0","lv_size":"500.00","pool_lv":"data","data_percent":"60.00","metadata_percent":""}
    ]}]}"#;

    #[tokio::test]
    async fn redesign_host_4_the_disk_reading_carries_the_pools_use_and_the_disks_kind() {
        let exec = MockExecutor::new();
        exec.respond_always("lv_size pve/root", CmdOutput::ok("  96.00g\n"));
        exec.respond_always("lv_size pve/data", CmdOutput::ok("  794.30g\n"));
        exec.respond_always("--reportformat json", CmdOutput::ok(LVS_JSON));
        exec.respond_always(
            "pv_name --select vg_name=pve",
            CmdOutput::ok("  /dev/sda3\n"),
        );
        exec.respond_always("--getsize64", CmdOutput::ok("1000204886016\n"));
        exec.respond_always("ROTA,TRAN /dev/sda", CmdOutput::ok("   0 sata\n"));
        exec.respond_always("--max-depth=2", CmdOutput::ok("1024\t/var\n"));
        let d = gather_host_disk_detail(&exec, 7).await;
        assert_eq!(d.root_disk_device, "/dev/sda");
        assert_eq!(d.root_disk_kind.as_deref(), Some("SSD"));
        let pool = d.thin_pool.expect("the pool's use is read");
        assert_eq!(pool.data_pct, 41.2);
        assert_eq!(pool.metadata_pct, 2.53);
        assert_eq!(pool.promised_gb, 508.0);
        assert_eq!(pool.volumes, 2);
        assert!(exec.unused_rules().is_empty(), "{:?}", exec.unused_rules());
    }

    #[tokio::test]
    async fn redesign_host_4_an_unreadable_pool_or_disk_is_absent_never_invented() {
        let exec = MockExecutor::new();
        exec.respond_always("--reportformat json", CmdOutput::failed(5, "no such vg"));
        exec.respond_always(
            "pv_name --select vg_name=pve",
            CmdOutput::ok("  /dev/nvme0n1p3\n"),
        );
        exec.respond_always("--getsize64", CmdOutput::ok("1000204886016\n"));
        exec.respond_always("ROTA,TRAN", CmdOutput::failed(32, "not a block device"));
        let d = gather_host_disk_detail(&exec, 7).await;
        assert_eq!(d.thin_pool, None);
        assert_eq!(d.root_disk_kind, None);
    }
}

/// fix-222 round 2 (live, host 3.70.7, 2026-10-03): `status --json` carried
/// the root volume and the thin pool but `root_disk_device` was empty and
/// `root_disk_total_gb` 0. `pvs` takes physical volumes (or tags) as its
/// positional arguments, never a volume group: `pvs ... pve` answers
/// `Failed to find physical volume "pve".` with exit 5 on a real host, so
/// the whole physical-disk half was skipped. The earlier tests matched the
/// mock on the substring `pv_name pve` and so accepted the wrong call. The
/// outputs below have the shape lvm2, util-linux and coreutils print on a
/// Proxmox VE host installed on one NVMe disk.
#[cfg(test)]
mod fix_222_real_host_tests {
    use super::*;
    use homelab_core::executor::{CmdOutput, MockExecutor};

    #[tokio::test]
    async fn fix_222_the_physical_disk_is_found_with_the_call_lvm_really_answers() {
        let exec = MockExecutor::new();
        exec.respond_always("lv_size pve/root", CmdOutput::ok("  96.00g\n"));
        exec.respond_always("lv_size pve/data", CmdOutput::ok("  794.30g\n"));
        // What lvm2 answers when handed a volume group where a PV belongs.
        exec.respond_always(
            "pvs --noheadings -o pv_name pve",
            CmdOutput::failed(5, "  Failed to find physical volume \"pve\".\n"),
        );
        // The volume group selected the way pvs accepts it.
        exec.respond_always(
            "pvs --noheadings -o pv_name --select vg_name=pve",
            CmdOutput::ok("  /dev/nvme0n1p3\n"),
        );
        exec.respond_always(
            "blockdev --getsize64 /dev/nvme0n1",
            CmdOutput::ok("1000204886016\n"),
        );
        exec.respond_always(
            "lsblk -d -n -o ROTA,TRAN /dev/nvme0n1",
            CmdOutput::ok("   0 nvme\n"),
        );
        let d = gather_host_disk_detail(&exec, 7).await;
        assert_eq!(d.root_disk_device, "/dev/nvme0n1");
        assert!(
            (d.root_disk_total_gb - 931.5).abs() < 0.1,
            "{}",
            d.root_disk_total_gb
        );
        assert_eq!(d.root_disk_kind.as_deref(), Some("NVMe SSD"));
    }
}
