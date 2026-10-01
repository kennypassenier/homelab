//! E8: ZFS snapshots + replication. The tests that matter are the refusals —
//! the script this replaces would destroy a target's whole snapshot history
//! whenever the chain broke.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::OpCtx;
use homelab_core::ops::zfs::*;
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
    // Per dataset since fix-85 (no -R).
    let inc = exec.calls_containing("zfs send -I");
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
    // A full stream of the new snapshot, per dataset since fix-85 (no -R).
    let sends = exec.calls_containing("zfs send 'HDD4TB@");
    assert_eq!(sends.len(), 1, "one full seed: {:?}", sends);
    assert!(
        exec.calls_containing("zfs send -I").is_empty(),
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

/// F177: a replica must never arrive claiming the path its source is
/// mounted at.
///
/// `zfs send -R` carries the source's properties and `receive` applies them.
/// Found live on 2026-09-02 while following the disaster-recovery runbook:
/// `HDD18TB/replica/HDD2TB/paperless-config` and the real
/// `HDD2TB/paperless-config` both had
/// `mountpoint=/appdata/paperwork/paperless-config`, both `canmount=on`, and
/// nothing anywhere decides which of the two wins after a reboot. If the
/// copy wins, paperless serves stale data, writes land in the replica, and
/// the next replication run overwrites them — silently, in both directions.
///
/// covers: F177
#[tokio::test]
async fn a_replica_never_inherits_the_path_its_source_lives_at() {
    // A source and a target that share a snapshot: the incremental path.
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD2TB", CmdOutput::ok("HDD2TB\n"));
    exec.respond_always("-r HDD2TB", CmdOutput::ok("HDD2TB@homelab-20260901-0400\n"));
    exec.respond_always(
        "-r HDD18TB/REPLICA_2TB",
        CmdOutput::ok("HDD18TB/REPLICA_2TB@homelab-20260901-0400\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let _ = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD2TB", "HDD18TB/REPLICA_2TB")],
        &homelab_core::retention::default_tiers(),
    )
    .await;

    let receives = exec.calls_containing("zfs receive");
    assert!(
        !receives.is_empty(),
        "the test must actually reach a receive, or it proves nothing"
    );
    for call in &receives {
        assert!(
            call.contains("-x mountpoint"),
            "every receive must exclude the mountpoint, or the replica claims \
             the live path: {}",
            call
        );
    }
}

// ── fix-85: the replica keeps its own history (2026-09-27) ──────────────────
//
// Expert panel finding zfs-replica-mirrors-mistakes. The nightly run sent
// `zfs send -RI | zfs receive -F`: a replication stream received with -F
// destroys on the target every snapshot and file system that no longer
// exists on the source (zfs-receive(8)). Measured on pve 2026-09-27: the
// replica held exactly the source's snapshots, 0 holds, so a mistaken
// `zfs destroy` on HDD4TB would have reached HDD18TB the next night.
//
// The mock cannot model ZFS deleting things, so these tests pin the two
// things that decide it: no stream is a replication stream (`-R`), and
// nothing the source lost is ever named in a `zfs destroy` on the replica.

/// Every receive whose destination is exactly `target`.
fn receives_into(exec: &MockExecutor, target: &str) -> Vec<String> {
    exec.calls_containing("zfs receive")
        .into_iter()
        .filter(|c| c.trim_end().ends_with(&format!("'{}'", target)))
        .collect()
}

fn no_replication_streams(exec: &MockExecutor) {
    let sends = exec.calls_containing("zfs send");
    assert!(!sends.is_empty(), "the test must reach a send");
    for s in &sends {
        assert!(
            !s.contains("send -R") && !s.contains(" -R ") && !s.contains("-RI"),
            "a replication stream received with -F deletes on the replica what \
             the source lost: {}",
            s
        );
    }
}

/// A dataset destroyed on the source is not destroyed on the replica, and
/// retention leaves its last copies alone.
///
/// covers: fix-85
#[tokio::test]
async fn a_dataset_destroyed_on_the_source_survives_on_the_replica() {
    // `zfs destroy -r HDD4TB/backups` by mistake: the source subtree no
    // longer has it; the replica still does, with a dense old history that
    // retention would thin if it treated the orphan like a live dataset.
    let old = [
        "homelab-20260501-0215",
        "homelab-20260502-0215",
        "homelab-20260503-0215",
        "homelab-20260504-0215",
        "homelab-20260505-0215",
        "homelab-20260506-0215",
        "homelab-20260507-0215",
        "homelab-20260508-0215",
        "homelab-20260509-0215",
        "homelab-20260510-0215",
    ];
    let recent = ["homelab-20260825-0215", "homelab-20260826-0215"];
    let label = "homelab-20260827-1643"; // what `zfs snapshot -r` made at NOW
    let mut src = String::new();
    for s in old.iter().chain(recent.iter()).chain([label].iter()) {
        src.push_str(&format!("HDD4TB@{s}\nHDD4TB/subvol-103-disk-0@{s}\n"));
    }
    let r = "HDD18TB/replica/HDD4TB";
    let mut tgt = String::new();
    for s in old.iter().chain(recent.iter()) {
        for ds in [
            r.to_string(),
            format!("{r}/backups"),
            format!("{r}/subvol-103-disk-0"),
        ] {
            tgt.push_str(&format!("{ds}@{s}\n"));
        }
    }
    let datasets = format!("{r}\n{r}/backups\n{r}/subvol-103-disk-0\n");
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD4TB", CmdOutput::ok("HDD4TB\n"));
    exec.respond_always("-r HDD4TB", CmdOutput::ok(&src));
    exec.respond_always(
        &format!("filesystem,volume -r {r}"),
        CmdOutput::ok(&datasets),
    );
    exec.respond_always(&format!("-r {r}"), CmdOutput::ok(&tgt));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD4TB", r)],
        &homelab_core::retention::default_tiers(),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);

    no_replication_streams(&exec);
    assert!(
        receives_into(&exec, &format!("{r}/backups")).is_empty(),
        "nothing is received into the orphan"
    );
    assert_eq!(receives_into(&exec, r).len(), 1);
    assert_eq!(
        receives_into(&exec, &format!("{r}/subvol-103-disk-0")).len(),
        1
    );
    let orphan_destroys = exec.calls_containing(&format!("zfs destroy {r}/backups"));
    assert!(
        orphan_destroys.is_empty(),
        "the last copies of a dataset the source lost are never pruned: {:?}",
        orphan_destroys
    );
    assert!(
        !exec
            .calls_containing(&format!("zfs destroy {r}/subvol-103-disk-0@"))
            .is_empty(),
        "retention still thins the live datasets, or this proves nothing"
    );
    let plan = plan_replication(&job("HDD4TB", r), label, &src, &datasets, &tgt);
    assert_eq!(plan.orphans, vec![format!("{r}/backups")]);
}

/// A snapshot destroyed on the source by mistake stays on the replica.
///
/// covers: fix-85
#[tokio::test]
async fn a_snapshot_destroyed_on_the_source_by_mistake_stays_on_the_replica() {
    let label = "homelab-20260827-1643";
    let r = "HDD18TB/replica/HDD2TB";
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD2TB", CmdOutput::ok("HDD2TB\n"));
    // HDD2TB@homelab-20260825-0215 was destroyed by hand on the source.
    exec.respond_always(
        "-r HDD2TB",
        CmdOutput::ok(&format!(
            "HDD2TB@homelab-20260824-0215\nHDD2TB@homelab-20260826-0215\nHDD2TB@{label}\n"
        )),
    );
    exec.respond_always(
        &format!("filesystem,volume -r {r}"),
        CmdOutput::ok(&format!("{r}\n")),
    );
    exec.respond_always(
        &format!("-r {r}"),
        CmdOutput::ok(&format!(
            "{r}@homelab-20260824-0215\n{r}@homelab-20260825-0215\n{r}@homelab-20260826-0215\n"
        )),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD2TB", r)],
        &homelab_core::retention::default_tiers(),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    no_replication_streams(&exec);
    let rx = receives_into(&exec, r);
    assert_eq!(rx.len(), 1, "{:?}", rx);
    assert!(
        rx[0].contains("-I 'HDD2TB@homelab-20260826-0215'"),
        "incremental from the replica's newest snapshot: {}",
        rx[0]
    );
    assert!(
        exec.calls_containing(&format!("zfs destroy {r}@homelab-20260825-0215"))
            .is_empty()
    );
}

/// Source retention pruned old snapshots the replica still keeps: the next
/// incremental rides the newest shared snapshot and the replica keeps the
/// older ones.
///
/// covers: fix-85
#[tokio::test]
async fn the_replica_keeps_history_the_source_already_pruned() {
    let label = "homelab-20260827-1643";
    let r = "HDD18TB/replica/HDD4TB";
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD4TB", CmdOutput::ok("HDD4TB\n"));
    exec.respond_always(
        "-r HDD4TB",
        CmdOutput::ok(&format!("HDD4TB@homelab-20260826-0215\nHDD4TB@{label}\n")),
    );
    exec.respond_always(
        &format!("filesystem,volume -r {r}"),
        CmdOutput::ok(&format!("{r}\n")),
    );
    exec.respond_always(
        &format!("-r {r}"),
        CmdOutput::ok(&format!(
            "{r}@homelab-20260601-0215\n{r}@homelab-20260801-0215\n{r}@homelab-20260826-0215\n"
        )),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD4TB", r)],
        &homelab_core::retention::default_tiers(),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    no_replication_streams(&exec);
    let rx = receives_into(&exec, r);
    assert_eq!(rx.len(), 1, "{:?}", rx);
    assert!(
        rx[0].contains("-I 'HDD4TB@homelab-20260826-0215'"),
        "{}",
        rx[0]
    );
    assert!(
        exec.calls_containing(&format!("zfs destroy {r}@"))
            .is_empty(),
        "sparse old history on the replica is kept: {:?}",
        exec.calls_containing("zfs destroy")
    );
}

/// The source lost the snapshot that is the replica's newest. The only
/// incremental left starts at an older shared snapshot, and receiving it
/// would roll the replica back and destroy everything after that point. The
/// run refuses instead and names the snapshot, and nothing on the replica
/// is touched.
///
/// covers: fix-85
#[tokio::test]
async fn a_replica_whose_newest_snapshot_left_the_source_is_refused_not_rolled_back() {
    let label = "homelab-20260827-1643";
    let r = "HDD18TB/replica/HDD4TB";
    let exec = MockExecutor::new();
    exec.respond_always("zfs list -H -o name HDD4TB", CmdOutput::ok("HDD4TB\n"));
    exec.respond_always(
        "-r HDD4TB",
        CmdOutput::ok(&format!("HDD4TB@homelab-20260824-0215\nHDD4TB@{label}\n")),
    );
    exec.respond_always(
        &format!("filesystem,volume -r {r}"),
        CmdOutput::ok(&format!("{r}\n")),
    );
    exec.respond_always(
        &format!("-r {r}"),
        CmdOutput::ok(&format!(
            "{r}@homelab-20260824-0215\n{r}@homelab-20260826-0215\n"
        )),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = replicate(
        &ctx(&exec, &sink, &j),
        &[job("HDD4TB", r)],
        &homelab_core::retention::default_tiers(),
    )
    .await;
    assert!(
        exec.calls_containing("zfs receive").is_empty(),
        "no receive may roll the replica back: {:?}",
        exec.calls_containing("zfs receive")
    );
    assert!(exec.calls_containing("zfs destroy").is_empty());
    assert!(
        !report.ok,
        "a stuck chain is a failed night, not a quiet one"
    );
    let err = report.error.expect("a refusal carries an operator error");
    assert!(
        format!("{} {}", err.why, err.remedy).contains("homelab-20260826-0215"),
        "{:?}",
        err
    );
}

/// The plan, without a mock: one entry per source dataset, parents first,
/// and the target datasets whose source is gone listed as orphans.
///
/// covers: fix-85
#[test]
fn the_plan_sends_each_dataset_on_its_own_and_leaves_orphans_alone() {
    let j = job("HDD2TB", "HDD18TB/replica/HDD2TB");
    let label = "homelab-20260827-1643";
    let src = format!(
        "HDD2TB@homelab-20260826-0215\nHDD2TB/a@homelab-20260826-0215\n\
         HDD2TB@{label}\nHDD2TB/a@{label}\nHDD2TB/new@{label}\n"
    );
    let tds = "HDD18TB/replica/HDD2TB\nHDD18TB/replica/HDD2TB/a\n\
               HDD18TB/replica/HDD2TB/gone\nHDD18TB/replica/HDD2TB/gone/child\n";
    let tsn = "HDD18TB/replica/HDD2TB@homelab-20260826-0215\n\
               HDD18TB/replica/HDD2TB/a@homelab-20260826-0215\n\
               HDD18TB/replica/HDD2TB/gone@homelab-20260826-0215\n";
    let plan = plan_replication(&j, label, &src, tds, tsn);
    let inc = DatasetStep::Incremental {
        base: "homelab-20260826-0215".into(),
    };
    assert_eq!(
        plan.datasets,
        vec![
            DatasetPlan {
                source: "HDD2TB".into(),
                target: "HDD18TB/replica/HDD2TB".into(),
                step: inc.clone(),
            },
            DatasetPlan {
                source: "HDD2TB/a".into(),
                target: "HDD18TB/replica/HDD2TB/a".into(),
                step: inc,
            },
            DatasetPlan {
                source: "HDD2TB/new".into(),
                target: "HDD18TB/replica/HDD2TB/new".into(),
                step: DatasetStep::Seed,
            },
        ]
    );
    assert_eq!(
        plan.orphans,
        vec![
            "HDD18TB/replica/HDD2TB/gone".to_string(),
            "HDD18TB/replica/HDD2TB/gone/child".to_string()
        ]
    );
}

/// The replica's retention is its own and never shorter than the source's:
/// a snapshot survives on the replica if either policy keeps it.
///
/// covers: fix-85
#[test]
fn the_replica_keeps_at_least_what_either_retention_policy_keeps() {
    use homelab_core::retention::{default_tiers, forget_list};
    const DAY: u64 = 86_400;
    let snaps: Vec<(String, u64)> = (0..400u64)
        .map(|d| (format!("s{d}"), NOW - d * DAY))
        .collect();
    let kept = |forget: &[String]| -> std::collections::BTreeSet<String> {
        snaps
            .iter()
            .map(|(id, _)| id.clone())
            .filter(|id| !forget.contains(id))
            .collect()
    };
    let replica = kept(&replica_forget(&snaps, &default_tiers(), NOW));
    let source = kept(&forget_list(&snaps, &default_tiers(), NOW));
    let own = kept(&forget_list(&snaps, &replica_tiers(), NOW));
    assert!(
        replica.is_superset(&source),
        "never shorter than the source"
    );
    assert!(replica.is_superset(&own), "its own tiers hold too");
    assert!(
        replica.len() > source.len(),
        "a longer history than the source: {} vs {}",
        replica.len(),
        source.len()
    );
    assert!(replica.contains("s0"), "the newest is the next base");
}
