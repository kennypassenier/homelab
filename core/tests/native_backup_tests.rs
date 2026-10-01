//! fix-113 (native-tar-no-quiesce, 2026-09-27): a native backup streamed a
//! tar of live data with no way to quiesce the service, no stale-lock
//! cleanup, and the fleet-wide retention whatever the stack file said.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::native::{validate_native, BackupPause, NativeServiceManifest};
use homelab_core::ops::backup::BackupCfg;
use homelab_core::ops::native::{backup_native, decide_chassis_pause, ChassisPauseOutcome};
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

const NOW: u64 = 1_790_000_000;

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
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    }
}

fn almanac(pause: BackupPause) -> NativeServiceManifest {
    NativeServiceManifest {
        restore_note: None,
        stack_name: "almanac".into(),
        vmid: 112,
        hostname: "112-app-almanac".into(),
        unit: "almanac".into(),
        binary: "/opt/almanac/bin/almanac".into(),
        env_file: None,
        data_dirs: vec!["/appdata/almanac/almanac-config".into()],
        update_cmd: None,
        stateless: false,
        release_repo: None,
        release_asset: None,
        backup_from_newest: None,
        backup_pause: pause,
        update_policy: Default::default(),
        metrics: None,
    }
}

fn harness() -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config 112",
        CmdOutput::ok("hostname: 112-app-almanac\n"),
    );
    // The data directory holds files, so the fix-63 guard stands aside.
    exec.respond_always(
        "-type f",
        CmdOutput::ok("/appdata/almanac/almanac-config/db\n"),
    );
    exec.respond_always("snapshots --json", CmdOutput::ok("[]"));
    exec.respond_always("is-active", CmdOutput::ok("active\n"));
    exec
}

fn position(calls: &[String], needle: &str) -> usize {
    calls
        .iter()
        .position(|c| c.contains(needle))
        .unwrap_or_else(|| panic!("no call containing {:?}: {:?}", needle, calls))
}

#[tokio::test]
async fn a_stale_lock_is_cleared_before_the_snapshot() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(BackupPause::Off),
        &BackupCfg::default(),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls();
    assert!(position(&calls, "restic unlock") < position(&calls, "tar -cf -"));
}

#[tokio::test]
async fn a_paused_service_is_stopped_for_the_tar_and_started_again() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(BackupPause::Unit),
        &BackupCfg::default(),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls();
    let stop = position(&calls, "systemctl stop almanac.service");
    let tar = position(&calls, "tar -cf -");
    let start = position(&calls, "systemctl start almanac.service");
    assert!(stop < tar && tar < start, "{:?}", calls);
}

#[tokio::test]
async fn a_failed_snapshot_still_starts_the_paused_service() {
    let exec = harness();
    exec.respond_first(
        "tar -cf -",
        CmdOutput::failed(1, "restic: repository is locked"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(BackupPause::Unit),
        &BackupCfg::default(),
    )
    .await;
    assert!(!r.ok);
    assert_eq!(
        exec.calls_containing("systemctl start almanac.service")
            .len(),
        1,
        "a backup must never leave the service it paused stopped: {:?}",
        exec.calls()
    );
}

#[tokio::test]
async fn an_unpaused_service_is_never_stopped() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(BackupPause::Off),
        &BackupCfg::default(),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    assert!(exec.calls_containing("systemctl stop").is_empty());
}

#[tokio::test]
async fn the_stack_files_own_retention_wins_over_the_fleet_tiers() {
    let exec = harness();
    // Two snapshots two days apart: the fleet tiers keep both (daily for a
    // week); the stack's own 30-day tier keeps only the newer.
    exec.respond_first(
        "snapshots --json",
        CmdOutput::ok(
            r#"[{"short_id":"older001","time":"2026-09-19T02:00:00Z"},
                {"short_id":"newer002","time":"2026-09-21T02:00:00Z"}]"#,
        ),
    );
    let mut st = homelab_core::state::HostState::default();
    let manifest: homelab_core::manifest::StackManifest = serde_yaml::from_str(
        r#"
stack_name: almanac
vmid: 112
hostname: 112-app-almanac
native_only: true
network: {ip: 10.10.10.12/24, gateway: 10.10.10.1, bridge: vmbr0, vlan: 10}
resources: {cores: 1, memory_mb: 512, swap_mb: 0, disk_gb: 4, storage: local-lvm}
lxc: {template: "clone:998", unprivileged: true, features: "nesting=1", protection: true, gpu: false, vpn: false}
boot: {onboot: true}
storage: []
apps: []
natives: [almanac]
retention:
  - every_days: 30
"#,
    )
    .expect("a native stack file with its own retention");
    st.stacks.insert(
        "almanac".into(),
        homelab_core::state::StackState {
            applied_source: None,
            extra_route_files: Vec::new(),
            vmid: 112,
            hostname: "112-app-almanac".into(),
            apps: vec![],
            applied_at: NOW,
            last_backup: NOW,
            applied_hash: String::new(),
            manifest: Some(manifest),
            natives: vec![almanac(BackupPause::Off)],
            enabled: true,
            incomplete_step: None,
            route_file: None,
        },
    );
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::to_string(&st).unwrap(),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(BackupPause::Off),
        &BackupCfg::default(),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let forget = exec.calls_containing("restic forget");
    assert_eq!(forget.len(), 1, "{:?}", exec.calls());
    assert!(forget[0].contains("older001") && !forget[0].contains("newer002"));
}

#[test]
fn pausing_a_stateless_service_is_refused() {
    let m = NativeServiceManifest {
        data_dirs: vec![],
        stateless: true,
        ..almanac(BackupPause::Unit)
    };
    let problems = validate_native(&m).unwrap_err();
    assert!(
        problems.iter().any(|p| p.contains("backup_pause")),
        "{:?}",
        problems
    );
}

#[test]
fn chassis_pausing_a_stateless_service_is_also_refused() {
    let m = NativeServiceManifest {
        data_dirs: vec![],
        stateless: true,
        ..almanac(BackupPause::Chassis)
    };
    let problems = validate_native(&m).unwrap_err();
    assert!(
        problems.iter().any(|p| p.contains("backup_pause")),
        "{:?}",
        problems
    );
}

// fix-113 (owner decision, 2026-10-01): `decide_chassis_pause` is the pure
// decision behind `backup_pause: chassis`, driven here per exit code so each
// branch is a table row rather than something only a live container proves.
mod chassis_pause_decision {
    use super::*;

    #[test]
    fn exit_0_is_paused_with_the_first_word_of_stdout() {
        assert_eq!(
            decide_chassis_pause(0, "paused 1790003600 writes\n", false),
            ChassisPauseOutcome::Paused("paused 1790003600 writes".into())
        );
        assert_eq!(
            decide_chassis_pause(0, "stopped almanac 1790003600", true),
            ChassisPauseOutcome::Paused("stopped almanac 1790003600".into())
        );
        assert_eq!(
            decide_chassis_pause(0, "not-running almanac", false),
            ChassisPauseOutcome::Paused("not-running almanac".into())
        );
    }

    #[test]
    fn exit_1_fails_loudly() {
        match decide_chassis_pause(1, "could not reach the listener", false) {
            ChassisPauseOutcome::Failed(why) => {
                assert!(why.contains("exit 1"));
                assert!(why.contains("could not reach the listener"));
            }
            other => panic!("expected Failed, got {:?}", other),
        }
    }

    #[test]
    fn exit_2_predates_the_subcommand_and_falls_back_to_stop() {
        // clap's own "unrecognized subcommand" exit — unit_active is not
        // even read for this code.
        assert_eq!(
            decide_chassis_pause(2, "error: unrecognized subcommand 'backup-pause'", true),
            ChassisPauseOutcome::FallBackToStop
        );
        assert_eq!(
            decide_chassis_pause(2, "error: unrecognized subcommand 'backup-pause'", false),
            ChassisPauseOutcome::FallBackToStop
        );
    }

    #[test]
    fn exit_3_falls_back_to_stop_only_when_the_unit_is_still_active() {
        assert_eq!(
            decide_chassis_pause(3, "no listener and the unit cannot be stopped", true),
            ChassisPauseOutcome::FallBackToStop
        );
        assert_eq!(
            decide_chassis_pause(3, "no listener and the unit cannot be stopped", false),
            ChassisPauseOutcome::NothingRunning
        );
    }

    #[test]
    fn an_unrecognised_exit_code_fails_loudly_rather_than_guessing() {
        match decide_chassis_pause(4, "busy", false) {
            ChassisPauseOutcome::Failed(why) => assert!(why.contains('4')),
            other => panic!("expected Failed, got {:?}", other),
        }
    }
}

mod chassis_pause_backup_native {
    use super::*;

    fn almanac_chassis() -> NativeServiceManifest {
        almanac(BackupPause::Chassis)
    }

    #[tokio::test]
    async fn exit_0_paused_runs_the_tar_between_pause_and_resume_with_no_systemctl_stop() {
        let exec = harness();
        exec.respond_first(
            "backup-pause --for",
            CmdOutput::ok("paused 1790003600 writes"),
        );
        exec.respond_first("backup-resume", CmdOutput::ok("resumed"));
        let sink = VecSink::new();
        let j = NullJournal;
        let r = backup_native(
            &ctx(&exec, &sink, &j),
            &almanac_chassis(),
            &BackupCfg::default(),
        )
        .await;
        assert!(r.ok, "{:?}", r.error);
        let calls = exec.calls();
        assert!(
            exec.calls_containing("systemctl stop").is_empty(),
            "a listener-paused service must never be stopped: {:?}",
            calls
        );
        let pause = position(&calls, "backup-pause --for");
        let tar = position(&calls, "tar -cf -");
        let resume = position(&calls, "backup-resume");
        assert!(pause < tar && tar < resume, "{:?}", calls);
    }

    #[tokio::test]
    async fn exit_2_predates_the_subcommand_and_falls_back_to_stop_start() {
        let exec = harness();
        exec.respond_first(
            "backup-pause --for",
            CmdOutput::failed(2, "error: unrecognized subcommand 'backup-pause'"),
        );
        let sink = VecSink::new();
        let j = NullJournal;
        let r = backup_native(
            &ctx(&exec, &sink, &j),
            &almanac_chassis(),
            &BackupCfg::default(),
        )
        .await;
        assert!(r.ok, "{:?}", r.error);
        let calls = exec.calls();
        let stop = position(&calls, "systemctl stop almanac.service");
        let tar = position(&calls, "tar -cf -");
        let start = position(&calls, "systemctl start almanac.service");
        assert!(stop < tar && tar < start, "{:?}", calls);
        assert!(
            exec.calls_containing("backup-resume").is_empty(),
            "the fallback resumes with systemctl, not the binary: {:?}",
            calls
        );
    }

    #[tokio::test]
    async fn exit_3_with_the_unit_active_falls_back_to_stop_start() {
        let exec = harness();
        exec.respond_first(
            "backup-pause --for",
            CmdOutput::failed(3, "no listener and the unit cannot be stopped"),
        );
        let sink = VecSink::new();
        let j = NullJournal;
        let r = backup_native(
            &ctx(&exec, &sink, &j),
            &almanac_chassis(),
            &BackupCfg::default(),
        )
        .await;
        assert!(r.ok, "{:?}", r.error);
        let calls = exec.calls();
        assert!(!exec
            .calls_containing("systemctl stop almanac.service")
            .is_empty());
        assert!(!exec
            .calls_containing("systemctl start almanac.service")
            .is_empty());
        assert!(
            exec.calls_containing("backup-resume").is_empty(),
            "{:?}",
            calls
        );
    }

    #[tokio::test]
    async fn exit_3_with_the_unit_not_active_proceeds_with_nothing_to_resume() {
        let exec = harness();
        exec.respond_first(
            "backup-pause --for",
            CmdOutput::failed(3, "no listener and the unit cannot be stopped"),
        );
        exec.respond_first("is-active", CmdOutput::ok("inactive\n"));
        let sink = VecSink::new();
        let j = NullJournal;
        let r = backup_native(
            &ctx(&exec, &sink, &j),
            &almanac_chassis(),
            &BackupCfg::default(),
        )
        .await;
        assert!(r.ok, "{:?}", r.error);
        assert!(exec.calls_containing("systemctl stop").is_empty());
        assert!(exec.calls_containing("systemctl start").is_empty());
        assert!(exec.calls_containing("backup-resume").is_empty());
    }

    #[tokio::test]
    async fn exit_1_fails_the_backup_and_archives_nothing() {
        let exec = harness();
        exec.respond_first(
            "backup-pause --for",
            CmdOutput::failed(1, "the unit started again on its own"),
        );
        let sink = VecSink::new();
        let j = NullJournal;
        let r = backup_native(
            &ctx(&exec, &sink, &j),
            &almanac_chassis(),
            &BackupCfg::default(),
        )
        .await;
        assert!(!r.ok);
        assert!(
            exec.calls_containing("tar -cf -").is_empty(),
            "exit 1 must refuse before the tar runs: {:?}",
            exec.calls()
        );
    }
}

// fix-113 ADDENDUM (owner + chassis-rs, 2026-10-01): "no guessed N" — the
// pause is a short, renewed window, and when a staging directory fits, the
// pause covers only a LOCAL copy; resume happens the moment that copy is
// done, not after restic's own (possibly slow) upload.
mod chassis_pause_addendum {
    use super::*;
    use homelab_core::ops::native::fits_staging;

    #[test]
    fn fits_staging_adds_a_20_pct_margin_against_free_space_and_the_cap() {
        // 1000 bytes needs 1200 after the margin.
        assert!(fits_staging(1000, 1200, u64::MAX));
        assert!(!fits_staging(1000, 1199, u64::MAX));
        // The cap is MiB; 1 MiB needs 1.2 MiB after the margin.
        let one_mib = 1024 * 1024;
        assert!(fits_staging(one_mib, u64::MAX, 2));
        assert!(!fits_staging(2 * one_mib, u64::MAX, 2));
    }

    #[test]
    fn fits_staging_an_empty_copy_always_fits() {
        assert!(fits_staging(0, 0, 0));
    }

    fn staging_cfg() -> BackupCfg {
        BackupCfg {
            staging_dir: Some("/HDD4TB/backup-staging".into()),
            staging_cap_mib: 10 * 1024,
            ..BackupCfg::default()
        }
    }

    #[tokio::test]
    async fn a_copy_that_fits_is_staged_resumed_and_uploaded_from_the_file() {
        let exec = harness();
        exec.respond_first(
            "backup-pause --for",
            CmdOutput::ok("paused 1790003600 writes"),
        );
        exec.respond_first("backup-resume", CmdOutput::ok("resumed"));
        exec.respond_first("du -scb", CmdOutput::ok("1000\n"));
        exec.respond_first("--output=avail", CmdOutput::ok("1000000000\n"));
        let sink = VecSink::new();
        let j = NullJournal;
        let r = backup_native(
            &ctx(&exec, &sink, &j),
            &almanac(BackupPause::Chassis),
            &staging_cfg(),
        )
        .await;
        assert!(r.ok, "{:?}", r.error);
        let calls = exec.calls();
        assert!(
            exec.calls_containing("systemctl stop").is_empty(),
            "a staged copy must never stop the unit: {:?}",
            calls
        );
        let pause = position(&calls, "backup-pause --for");
        // The staging tar writes to the file (redirected with `>`), not
        // piped into restic directly — "tar -cf -" is unique to that one
        // call here (the leftover-clear and final cleanup are both `rm -f`).
        let tar = position(&calls, "tar -cf -");
        let resume = position(&calls, "backup-resume");
        let upload = position(&calls, "cat ");
        assert!(
            pause < tar && tar < resume && resume < upload,
            "expected pause < local tar < resume < upload: {:?}",
            calls
        );
        // Cleaned up after the snapshot (rule 20), not left for next time.
        let cleanups: Vec<_> = exec
            .calls_containing("rm -f")
            .into_iter()
            .filter(|c| c.contains("almanac-stage.tar"))
            .collect();
        assert!(
            cleanups.len() >= 2,
            "expected a leftover-clear before staging and a delete after the snapshot: {:?}",
            calls
        );
    }

    #[tokio::test]
    async fn a_copy_that_does_not_fit_skips_staging_and_backs_up_live() {
        let exec = harness();
        exec.respond_first(
            "backup-pause --for",
            CmdOutput::ok("paused 1790003600 writes"),
        );
        exec.respond_first("du -scb", CmdOutput::ok("1000000000000\n"));
        exec.respond_first("--output=avail", CmdOutput::ok("1\n"));
        let sink = VecSink::new();
        let j = NullJournal;
        let r = backup_native(
            &ctx(&exec, &sink, &j),
            &almanac(BackupPause::Chassis),
            &staging_cfg(),
        )
        .await;
        assert!(r.ok, "{:?}", r.error);
        let calls = exec.calls();
        assert!(
            exec.calls_containing("systemctl stop").is_empty(),
            "{:?}",
            calls
        );
        // No staged file: the tar pipes straight into restic, same shape as
        // the no-staging-configured path, and resume comes after it.
        let tar = position(&calls, "tar -cf -");
        let resume = position(&calls, "backup-resume");
        assert!(tar < resume, "{:?}", calls);
        assert!(
            exec.calls_containing("almanac-stage.tar").is_empty(),
            "nothing should have been staged: {:?}",
            calls
        );
    }

    #[tokio::test]
    async fn a_failed_local_copy_resumes_and_cleans_up_without_archiving() {
        let exec = harness();
        exec.respond_first(
            "backup-pause --for",
            CmdOutput::ok("paused 1790003600 writes"),
        );
        exec.respond_first("backup-resume", CmdOutput::ok("resumed"));
        exec.respond_first("du -scb", CmdOutput::ok("1000\n"));
        exec.respond_first("--output=avail", CmdOutput::ok("1000000000\n"));
        exec.respond_first(
            "almanac-stage.tar",
            CmdOutput::failed(1, "pct exec: container is locked"),
        );
        let sink = VecSink::new();
        let j = NullJournal;
        let r = backup_native(
            &ctx(&exec, &sink, &j),
            &almanac(BackupPause::Chassis),
            &staging_cfg(),
        )
        .await;
        assert!(!r.ok, "a failed local copy must not report success");
        assert!(
            !exec.calls_containing("backup-resume").is_empty(),
            "the service must be resumed even when the local copy failed: {:?}",
            exec.calls()
        );
        assert!(
            exec.calls_containing("cat ").is_empty(),
            "nothing is uploaded from a copy that never finished: {:?}",
            exec.calls()
        );
    }
}
