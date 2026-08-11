//! M4 operation tests: gated destroy (C2) and backup/restore (E1/E2).

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::backup::{backup, restore, BackupCfg};
use homelab_core::ops::destroy::destroy;
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

fn manifest(vmid: u16, stack: &str) -> StackManifest {
    StackManifest {
        stack_name: stack.into(),
        vmid,
        hostname: format!("{}-app-{}", vmid, stack),
        network: NetworkSpec {
            ip: format!("10.10.10.{}/24", vmid - 100),
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: Some(10),
        },
        resources: ResourceSpec {
            cores: 1,
            memory_mb: 512,
            swap_mb: 256,
            disk_gb: 4,
            storage: "local-lvm".into(),
        },
        lxc: LxcSpec {
            template: "debian-12".into(),
            unprivileged: true,
            features: "nesting=1".into(),
            protection: true,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![MountSpec {
            host_path: "/appdata/test/test-config".into(),
            mount_point: "/appdata/test/test-config".into(),
            host_owner_uid: Some(101000),
        }],
        apps: vec!["app".into()],
    }
}

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
    }
}

// ── C2: gated destroy ───────────────────────────────────────────────────────

#[tokio::test]
async fn c2_destroy_refuses_wrong_typed_name() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(&ctx(&exec, &sink, &j), "test", 108, "wrong").await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("does not match"));
    assert!(
        exec.calls().is_empty(),
        "no commands before name confirmation"
    );
}

#[tokio::test]
async fn c2_destroy_refuses_no_touch_vmid() {
    for vmid in [101u16, 104, 106] {
        let exec = MockExecutor::new();
        let sink = VecSink::new();
        let j = NullJournal;
        let report = destroy(&ctx(&exec, &sink, &j), "evil", vmid, "evil").await;
        assert!(!report.ok, "vmid {} must be refused", vmid);
        assert!(report.error.unwrap().why.contains("no-touch"));
        assert!(exec.calls_containing("pct destroy").is_empty());
    }
}

#[tokio::test]
async fn c2_destroy_refuses_hostname_mismatch() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 108-app-somethingelse\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(&ctx(&exec, &sink, &j), "test", 108, "test").await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("refusing to destroy"));
    assert!(exec.calls_containing("pct destroy").is_empty());
}

#[tokio::test]
async fn c2_destroy_happy_path_lifts_protection_then_destroys() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 108-app-test\nprotection: 1\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(&ctx(&exec, &sink, &j), "test", 108, "test").await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    // Protection is lifted before destroy; state updated after.
    assert!(pos("--protection 0") < pos("pct destroy 108"));
    assert!(exec.calls_containing("pct destroy 108 --purge").len() == 1);
    // State no longer lists the stack.
    let state = exec.file("/var/lib/homelab/state.json").unwrap_or_default();
    assert!(!state.contains("\"test\""));
}

// ── E1/E2: backup and restore ───────────────────────────────────────────────

#[tokio::test]
async fn e1_backup_runs_init_quiesce_snapshot_resume_retention() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let cfg = BackupCfg::default();
    let report = backup(&ctx(&exec, &sink, &j), &manifest(108, "test"), &cfg).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(
        pos("restic init") < pos("backup.pause=true"),
        "init before quiesce"
    );
    assert!(
        pos("backup.pause=true") < pos("restic backup"),
        "quiesce before snapshot"
    );
    assert!(
        pos("restic backup") < pos("restic forget"),
        "snapshot before retention"
    );
    // Snapshot targets the /appdata path.
    assert!(exec
        .calls_containing("/appdata/test/test-config")
        .iter()
        .any(|c| c.contains("restic backup")));
    // Retention uses the configured policy.
    assert!(exec.calls_containing("--keep-daily 7").len() == 1);
}

#[tokio::test]
async fn e2_restore_validates_quiesces_restores_resumes_verifies() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "snapshots --last",
        CmdOutput::ok("id  time  host\nabc123  today\n"),
    );
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let cfg = BackupCfg::default();
    let report = restore(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        &cfg,
        "latest",
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(
        pos("snapshots --last") < pos("compose down"),
        "validate before quiesce"
    );
    assert!(
        pos("compose down") < pos("restic restore"),
        "quiesce before restore"
    );
    assert!(
        pos("restic restore latest --target /") < pos("compose up -d"),
        "restore before resume"
    );
}

#[tokio::test]
async fn e2_restore_fails_when_app_not_running_after() {
    let exec = MockExecutor::new();
    exec.respond_always("snapshots --last", CmdOutput::ok("id\nabc\n"));
    // verify returns empty → app not running.
    exec.respond_always("ps --status running --services", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = restore(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        &BackupCfg::default(),
        "latest",
    )
    .await;
    assert!(!report.ok);
    assert!(report
        .error
        .unwrap()
        .why
        .contains("not running after restore"));
}

// ── D9/B6: managed updates with rollback ────────────────────────────────────

use homelab_core::ops::update::update;

#[tokio::test]
async fn d9_update_capture_pull_verify_order() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "docker inspect --format",
        CmdOutput::ok("sha256:aaa myimg:latest\n"),
    );
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update(&ctx(&exec, &sink, &j), &manifest(108, "test"), None, false).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(
        pos("docker inspect --format") < pos("compose pull"),
        "capture before pull"
    );
    assert!(
        pos("compose pull") < pos("ps --status running"),
        "pull before verify"
    );
    // No rollback happened on the happy path.
    assert!(exec.calls_containing("docker tag").is_empty());
}

#[tokio::test]
async fn d9_auto_update_skips_non_auto_policy() {
    let exec = MockExecutor::new();
    // Policy label query returns "manual".
    exec.respond_always("com.homelab.update.policy", CmdOutput::ok("manual\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update(&ctx(&exec, &sink, &j), &manifest(108, "test"), None, true).await;
    assert!(report.ok);
    assert!(
        exec.calls_containing("compose pull").is_empty(),
        "manual-policy app must not be pulled on a scheduled run"
    );
}

#[tokio::test]
async fn b6_failed_update_rolls_back_to_captured_image() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "docker inspect --format",
        CmdOutput::ok("sha256:oldimg myimg:latest\n"),
    );
    // First verify after update: not running. Second verify (post-rollback): running.
    exec.enqueue("ps --status running --services", CmdOutput::ok(""));
    exec.enqueue("ps --status running --services", CmdOutput::ok("app\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update(&ctx(&exec, &sink, &j), &manifest(108, "test"), None, false).await;
    // The op FAILS (the new image is bad) but the system is healthy again.
    assert!(!report.ok);
    let why = report.error.unwrap().why;
    assert!(why.contains("ROLLED BACK"), "got: {}", why);
    // The rollback re-tagged the captured image and force-recreated.
    let tag_calls = exec.calls_containing("docker tag sha256:oldimg myimg:latest");
    assert_eq!(tag_calls.len(), 1);
    assert!(tag_calls[0].contains("--force-recreate"));
}

#[tokio::test]
async fn d9_update_unknown_app_refused() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        Some("nope"),
        false,
    )
    .await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("not part of stack"));
}
