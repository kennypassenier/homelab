//! Capacity facts for the filesystems and pools: pve root, the thin pool and the ZFS pools.

use crate::*;
use homelab_core::ops::fleetcheck::{HostCapacityFact, HostCapacityMetric, parse_df_pcent};

pub(crate) fn fact(
    metric: HostCapacityMetric,
    subject: impl Into<String>,
    used_pct: u8,
    detail: String,
    cap_configured: bool,
) -> HostCapacityFact {
    HostCapacityFact {
        metric,
        subject: subject.into(),
        used_pct,
        detail,
        cap_configured,
    }
}

/// pve's root filesystem.
pub(crate) async fn pve_root(exec: &dyn Executor) -> Vec<HostCapacityFact> {
    match exec
        .run(&Cmd::new("df", &["--output=pcent", "/"], 15))
        .await
    {
        Ok(o) => parse_df_pcent(&o.stdout)
            .map(|pct| {
                fact(
                    HostCapacityMetric::PveRoot,
                    "pve",
                    pct,
                    format!("{pct}% full"),
                    true,
                )
            })
            .into_iter()
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// The default Proxmox thin pool: volume group `pve`, logical volume `data`. A
/// host configured with a different pool name is not measured here — that is
/// a fact to add, not a guess to make.
pub(crate) async fn thin_pool(exec: &dyn Executor) -> Vec<HostCapacityFact> {
    let args = [
        "--noheadings",
        "-o",
        "data_percent,metadata_percent",
        "pve/data",
    ];
    let Ok(o) = exec.run(&Cmd::new("lvs", &args, 15)).await else {
        return Vec::new();
    };
    let Some((data, meta)) = homelab_core::ops::fleetcheck::parse_thin_pool_percents(&o.stdout)
    else {
        return Vec::new();
    };
    vec![
        fact(
            HostCapacityMetric::ThinPoolData,
            "local-lvm",
            data,
            format!("{data}% full (data)"),
            true,
        ),
        fact(
            HostCapacityMetric::ThinPoolMeta,
            "local-lvm",
            meta,
            format!("{meta}% full (metadata)"),
            true,
        ),
    ]
}

/// Every ZFS pool's fill.
pub(crate) async fn zfs_pools(exec: &dyn Executor) -> Vec<HostCapacityFact> {
    let Ok(o) = exec
        .run(&Cmd::new(
            "zpool",
            &["list", "-H", "-o", "name,capacity"],
            15,
        ))
        .await
    else {
        return Vec::new();
    };
    homelab_core::ops::fleetcheck::parse_zpool_capacities(&o.stdout)
        .into_iter()
        .map(|(pool, pct)| {
            fact(
                HostCapacityMetric::ZfsPool,
                pool,
                pct,
                format!("{pct}% full"),
                true,
            )
        })
        .collect()
}
