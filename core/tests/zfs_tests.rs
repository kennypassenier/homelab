//! E8: ZFS snapshots + replication. The tests that matter are the refusals —
//! the script this replaces would destroy a target's whole snapshot history
//! whenever the chain broke.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::zfs::*;
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

const NOW: u64 = 1_787_849_000; // 2026-08-27, mid-evening

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: NOW,
        kea: None,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        backup: Default::default(),
        registry_cache: None,
    }
}

fn job(src: &str, tgt: &str) -> ZfsJob {
    ZfsJob {
        source: src.into(),
        target: tgt.into(),
    }
}

#[test]
fn e8_job_validation_rejects_self_eating_pairs() {
    assert!(job_problems(&job("HDD2TB", "HDD18TB/REPLICA_2TB")).is_none());
    assert!(
        job_problems(&job("HDD2TB", "HDD2TB")).is_some(),
        "same dataset"
    );
    assert!(
        job_problems(&job("HDD2TB", "HDD2TB/replica")).is_some(),
        "target inside source — a recursive send would eat itself"
    );
    assert!(
        job_problems(&job("HDD18TB/REPLICA_2TB", "HDD18TB")).is_some(),
        "source inside target"
    );
    assert!(job_problems(&job("", "HDD18TB")).is_some(), "empty source");
}

#[test]
fn e8_common_base_picks_the_newest_shared_snapshot() {
    let src: Vec<String> = [
        "homelab-20260801-0400",
        "homelab-20260802-0400",
        "homelab-20260803-0400",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let tgt: Vec<String> = ["homelab-20260801-0400", "homelab-20260802-0400"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        common_base(&src, &tgt).as_deref(),
        Some("homelab-20260802-0400")
    );
    // Nothing shared → None, never a guess.
    assert_eq!(common_base(&src, &[]), None);
}

#[test]
fn e8_parse_snap_names_sees_every_snapshot_on_the_dataset() {
    let out = "HDD2TB@homelab-20260801-0400\nHDD2TB@backup-20260523-1059\nHDD2TB/child@homelab-20260801-0400\n";
    // This dataset's own snapshots, ours AND foreign. Live lesson from the
    // first real run: the retired script's `backup-*` snapshots must count,
    // both as a usable incremental base during the migration and — the part
    // that actually bit — as proof that the target is NOT a blank slate.
    // They are never deleted by us; the prune step only touches `homelab-*`.
    assert_eq!(
        parse_snap_names(out, "HDD2TB"),
        vec!["homelab-20260801-0400", "backup-20260523-1059"]
    );
}

#[tokio::test]
async fn e8_target_with_only_foreign_snapshots_is_not_empty() {
    // The bug this test was written for, caught on the first live run: the
    // target held only `backup-*` snapshots from the retired cron script.
    // Counting just our own made it look empty, so the job attempted a full
    // seed — which ZFS itself refused ("destination has snapshots"). It has
    // to refuse before that, like any other broken chain.
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD2TB", CmdOutput::ok("HDD2TB\n"));
    exec.respond_always("-r HDD2TB", CmdOutput::ok("HDD2TB@homelab-20260827-1845\n"));
    exec.respond_always(
        "-r HDD18TB/REPLICA_2TB",
        CmdOutput::ok("HDD18TB/REPLICA_2TB@backup-20260523-1059\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD2TB", "HDD18TB/REPLICA_2TB")],
        &homelab_core::retention::default_tiers(),
    )
    .await;
    assert!(
        !report.ok,
        "a target with foreign snapshots is not a blank slate"
    );
    assert!(
        exec.calls_containing("zfs receive").is_empty(),
        "no send may even be attempted"
    );
    assert!(exec.calls_containing("zfs destroy").is_empty());
}

#[tokio::test]
async fn e8_rides_the_old_scripts_chain_during_migration() {
    // Both sides still carry the retired script's last snapshot: a perfectly
    // good incremental base, so switching over needs no re-seed and no
    // terabyte re-transfer.
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD4TB", CmdOutput::ok("HDD4TB\n"));
    exec.respond_always(
        "-r HDD4TB",
        CmdOutput::ok("HDD4TB@backup-20260827-1845\nHDD4TB@homelab-20260828-0400\n"),
    );
    exec.respond_always(
        "-r HDD18TB/REPLICA_4TB",
        CmdOutput::ok("HDD18TB/REPLICA_4TB@backup-20260827-1845\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD4TB", "HDD18TB/REPLICA_4TB")],
        &homelab_core::retention::default_tiers(),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let inc = exec.calls_containing("zfs send -RI");
    assert_eq!(inc.len(), 1, "incremental from the old base: {:?}", inc);
    assert!(inc[0].contains("backup-20260827-1845"), "{}", inc[0]);
}

#[test]
fn e8_snap_time_roundtrips_and_survives_garbage() {
    // 2026-08-27 18:45 UTC
    let t = snap_time("homelab-20260827-1845", NOW);
    assert_eq!(t, 1_787_856_300, "civil date → unix");
    // Unparseable names are treated as brand new, so retention keeps them
    // instead of deleting something it does not understand.
    assert_eq!(snap_time("homelab-nonsense", NOW), NOW);
    assert_eq!(snap_time("not-ours", NOW), NOW);
}

#[tokio::test]
async fn e8_refuses_to_reseed_over_an_existing_history() {
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD2TB", CmdOutput::ok("HDD2TB\n"));
    // Source has one snapshot, target has a DIFFERENT one → no common base,
    // but the target is not empty. The old script destroyed everything here.
    exec.respond_always("-r HDD2TB", CmdOutput::ok("HDD2TB@homelab-20260827-1845\n"));
    exec.respond_always(
        "-r HDD18TB/REPLICA_2TB",
        CmdOutput::ok("HDD18TB/REPLICA_2TB@homelab-20260523-1059\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD2TB", "HDD18TB/REPLICA_2TB")],
        &homelab_core::retention::default_tiers(),
    )
    .await;

    assert!(!report.ok, "must refuse, not re-seed");
    assert!(
        exec.calls_containing("zfs destroy").is_empty(),
        "NOTHING may be destroyed on the refusal path: {:?}",
        exec.calls_containing("zfs destroy")
    );
    assert!(
        exec.calls_containing("zfs receive").is_empty(),
        "no full send either"
    );
    let err = report.error.expect("a refusal carries an operator error");
    assert!(
        err.why.contains("share no snapshot") || err.remedy.contains("share no snapshot"),
        "{:?}",
        err
    );
}

#[tokio::test]
async fn e8_empty_target_is_seeded_incremental_otherwise() {
    // First run: target has nothing → a full send is legitimate.
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD4TB", CmdOutput::ok("HDD4TB\n"));
    exec.respond_always("-r HDD4TB", CmdOutput::ok("HDD4TB@homelab-20260827-1845\n"));
    exec.respond_always("-r HDD18TB/REPLICA_4TB", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD4TB", "HDD18TB/REPLICA_4TB")],
        &homelab_core::retention::default_tiers(),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let sends = exec.calls_containing("zfs send -R ");
    assert_eq!(sends.len(), 1, "one full seed: {:?}", sends);
    assert!(
        exec.calls_containing("zfs send -RI").is_empty(),
        "no incremental without a base"
    );
}

#[tokio::test]
async fn e8_no_jobs_is_an_error_not_a_silent_success() {
    // The failure mode of the old script: it "succeeded" while iterating
    // over an empty list of datasets that no longer existed.
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[],
        &homelab_core::retention::default_tiers(),
    )
    .await;
    assert!(!report.ok, "an empty job list must never read as success");
}
