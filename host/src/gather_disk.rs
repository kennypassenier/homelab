//! The disk facts the Host page shows beside the root volume: the thin pool, the root disk and its biggest directories.

use crate::*;

/// fix-222 (Kenny, 2026-10-02: "root disk van HOST is 48% gebruikt, is dat
/// de 1TB SSD die erin zit? is die al halfvol? met wat?"): which disk "root"
/// (`pve/root`, the lv the Host page's `disk_pct` measures) and "local-lvm"
/// (`pve/data`, the thin pool every LXC/VM disk is carved from) actually
/// are, and the root filesystem's biggest directories — `du -x
/// --max-depth=2 /` is the one potentially slow command here, bounded by
/// its own 20 s timeout and, in `build_fleet_state`, only ever run again
/// once the cache is stale (never on every status poll).
pub(crate) async fn gather_host_disk_detail(
    exec: &dyn Executor,
    now: u64,
) -> homelab_proto::HostDiskDetail {
    use homelab_core::ops::fleetcheck::{
        parse_blockdev_bytes, parse_du_top, parse_lv_size_gb, parse_pv_name, whole_disk_device,
    };
    let mut out = homelab_proto::HostDiskDetail {
        measured_at: now,
        ..Default::default()
    };
    if let Ok(o) = exec
        .run(&Cmd::new(
            "lvs",
            &["--noheadings", "--units", "g", "-o", "lv_size", "pve/root"],
            15,
        ))
        .await
        && let Some(gb) = parse_lv_size_gb(&o.stdout)
    {
        out.root_lv_size_gb = gb;
    }
    if let Ok(o) = exec
        .run(&Cmd::new(
            "lvs",
            &["--noheadings", "--units", "g", "-o", "lv_size", "pve/data"],
            15,
        ))
        .await
        && let Some(gb) = parse_lv_size_gb(&o.stdout)
    {
        out.thin_pool_size_gb = gb;
    }
    // redesign-host-4: how full that pool is and what its volumes are
    // promised (every volume whose pool is `data`, summed), one `lvs`.
    if let Ok(o) = exec
        .run(&Cmd::new(
            "lvs",
            &[
                "--reportformat",
                "json",
                "--units",
                "g",
                "--nosuffix",
                "-o",
                "lv_name,lv_size,pool_lv,data_percent,metadata_percent",
                "pve",
            ],
            15,
        ))
        .await
        && let Some(r) = homelab_core::ops::fleetcheck::parse_thin_pool_report(&o.stdout, "data")
    {
        out.thin_pool = Some(homelab_proto::ThinPoolUse {
            data_pct: r.data_pct,
            metadata_pct: r.metadata_pct,
            promised_gb: r.promised_gb,
            volumes: r.volumes,
        });
    }
    if let Ok(o) = exec
        .run(&Cmd::new(
            "pvs",
            // fix-222 round 2: pvs's positional arguments are PVs, not a
            // volume group (`pvs ... pve` fails on every real host); the
            // group is selected.
            &["--noheadings", "-o", "pv_name", "--select", "vg_name=pve"],
            15,
        ))
        .await
        && let Some(pv) = parse_pv_name(&o.stdout)
    {
        let disk = whole_disk_device(&pv);
        if let Ok(o) = exec
            .run(&Cmd::new("blockdev", &["--getsize64", &disk], 15))
            .await
            && let Some(gb) = parse_blockdev_bytes(&o.stdout)
        {
            out.root_disk_device = disk.clone();
            out.root_disk_total_gb = gb;
        }
        // redesign-host-4: the disk's kind (SSD, HDD, NVMe SSD).
        if let Ok(o) = exec
            .run(&Cmd::new(
                "lsblk",
                &["-d", "-n", "-o", "ROTA,TRAN", &disk],
                15,
            ))
            .await
        {
            out.root_disk_kind = homelab_core::ops::fleetcheck::parse_disk_kind(&o.stdout);
        }
    }
    if let Ok(o) = exec
        .run(&Cmd::new("du", &["-x", "--max-depth=2", "/"], 20))
        .await
    {
        out.top_dirs = parse_du_top(&o.stdout, 10);
    }
    out
}
