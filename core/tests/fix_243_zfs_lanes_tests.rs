//! fix-243 (2026-10-03, found while answering where else the fix-238 fault
//! sits): ZFS retention pruned `homelab-YYYYMMDD-HHMM` snapshots with one
//! `forget_list` per dataset, and the names carried no trigger. A
//! `homelab zfs-replicate` run by hand on a day the nightly already ran put
//! two snapshots in one daily bucket and destroyed the nightly one, on the
//! source and on the replica. Snapshots taken on demand now say so in their
//! name, and retention thins each lane on its own (fix-238's two lanes).

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::OpCtx;
use homelab_core::ops::backup::BackupTrigger;
use homelab_core::ops::zfs::*;
use homelab_core::retention::default_tiers;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

/// 2026-10-02 20:05 UTC (22:05 in Brussels): the evening a stack was
/// prepared by hand for an upgrade, after that night's 04:06 run.
const NOW: u64 = 1_790_971_500;

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: NOW,
        metrics_targets_dir: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        default_log_rotation: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    }
}

/// covers: fix-243
#[tokio::test]
async fn fix_243_an_on_demand_replicate_never_destroys_the_same_days_nightly() {
    // The live shape: last night's scheduled snapshot (02:06 UTC), already on
    // the replica, and this evening's run by hand. Both listings are as they
    // read after the run's own snapshot and send.
    let nightly = "homelab-20261002-0206";
    let manual = "homelab-20261002-2005-manual";
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD2TB", CmdOutput::ok("HDD2TB\n"));
    exec.respond_always(
        "-r HDD2TB",
        CmdOutput::ok(&format!("HDD2TB@{nightly}\nHDD2TB@{manual}\n")),
    );
    exec.respond_always(
        "-r HDD18TB/replica/HDD2TB",
        CmdOutput::ok(&format!(
            "HDD18TB/replica/HDD2TB@{nightly}\nHDD18TB/replica/HDD2TB@{manual}\n"
        )),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[ZfsJob {
            source: "HDD2TB".into(),
            target: "HDD18TB/replica/HDD2TB".into(),
        }],
        &default_tiers(),
        BackupTrigger::Manual,
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let destroyed = exec.calls_containing("zfs destroy");
    assert!(
        destroyed.iter().all(|c| !c.contains(nightly)),
        "the nightly snapshot was destroyed by a run by hand: {:?}",
        destroyed
    );
    assert_eq!(
        exec.calls_containing(&format!("zfs snapshot -r HDD2TB@{manual}"))
            .len(),
        1,
        "a run by hand names itself on demand: {:?}",
        exec.calls_containing("zfs snapshot")
    );
}

/// covers: fix-243
#[tokio::test]
async fn fix_243_a_nightly_run_keeps_the_name_every_existing_snapshot_has() {
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD2TB", CmdOutput::ok("HDD2TB\n"));
    exec.respond_always("-r HDD2TB", CmdOutput::ok("HDD2TB@homelab-20261002-2005\n"));
    exec.respond_always("-r HDD18TB/replica/HDD2TB", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[ZfsJob {
            source: "HDD2TB".into(),
            target: "HDD18TB/replica/HDD2TB".into(),
        }],
        &default_tiers(),
        BackupTrigger::Nightly,
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.calls_containing("zfs snapshot -r HDD2TB@homelab-20261002-2005")
            .len(),
        1
    );
    assert!(exec.calls_containing("-manual").is_empty());
}

/// covers: fix-243
#[test]
fn fix_243_names_without_the_marker_count_as_scheduled_and_both_parse() {
    assert!(snap_is_scheduled("homelab-20261002-0206"));
    assert!(!snap_is_scheduled("homelab-20261002-2005-manual"));
    assert_eq!(
        snap_time("homelab-20261002-2005-manual", 0),
        snap_time("homelab-20261002-2005", 0)
    );
    assert_eq!(snap_time("homelab-20261002-2005", 0), NOW);
    // Garbage still reads as brand new, so retention keeps it.
    assert_eq!(snap_time("homelab-20261002-2005-other", 7), 7);
}

/// The other direction of the same lane rule: two runs by hand on one day
/// thin each other, and leave the nightly alone.
///
/// covers: fix-243
#[test]
fn fix_243_two_runs_by_hand_thin_each_other_and_not_the_nightly() {
    const DAY: u64 = 86_400;
    let snaps = vec![
        ("n1".to_string(), NOW - 16 * 3600, true),
        ("m1".to_string(), NOW - 2 * 3600, false),
        ("m2".to_string(), NOW, false),
        ("n0".to_string(), NOW - DAY - 16 * 3600, true),
    ];
    let forget = replica_forget(&snaps, &default_tiers(), NOW);
    assert!(!forget.contains(&"n1".to_string()), "{:?}", forget);
    assert!(!forget.contains(&"n0".to_string()), "{:?}", forget);
    assert!(!forget.contains(&"m2".to_string()), "newest of its lane");
}
