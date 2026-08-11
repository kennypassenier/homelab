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
        pos("restic backup") < pos("snapshots --json"),
        "snapshot before retention listing"
    );
    // Snapshot targets the /appdata path.
    assert!(exec
        .calls_containing("/appdata/test/test-config")
        .iter()
        .any(|c| c.contains("restic backup")));
    // Tiered retention: with no snapshots listed, nothing is forgotten
    // (fail-safe: malformed/empty listing keeps everything).
    assert!(exec.calls_containing("restic forget").is_empty());
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

// ── H5: host self-update ────────────────────────────────────────────────────

use homelab_core::ops::selfupdate::{self_update, SelfUpdateCfg};

#[tokio::test]
async fn h5_selfupdate_verifies_before_touching_current() {
    let exec = MockExecutor::new();
    // Candidate fails selfcheck (wrong arch / truncated upload).
    exec.respond_always("--selfcheck", CmdOutput::failed(1, "exec format error"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = self_update(&ctx(&exec, &sink, &j), &SelfUpdateCfg::default()).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("failed selfcheck"));
    // The running binary was never backed up, replaced, or restarted.
    assert!(exec.calls_containing("cp -a").is_empty());
    assert!(exec.calls_containing("install").is_empty());
    assert!(exec.calls_containing("systemctl").is_empty());
}

#[tokio::test]
async fn h5_selfupdate_happy_path_arms_marker_before_restart() {
    let exec = MockExecutor::new();
    exec.respond_always("--selfcheck", CmdOutput::ok("2.1.0\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = self_update(&ctx(&exec, &sink, &j), &SelfUpdateCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(pos("--selfcheck") < pos("cp -a"), "verify before backup");
    assert!(
        pos("cp -a") < pos("install -m 755"),
        "backup before install"
    );
    assert!(
        pos("install -m 755") < pos("systemd-run"),
        "install before restart"
    );
    // Marker exists and records the new version before the restart fires.
    let marker = exec.file("/var/lib/homelab/selfupdate.pending").unwrap();
    assert!(marker.contains("2.1.0"));
    // Restart is delayed so the RPC reply can flush.
    assert!(exec.calls_containing("--on-active=2").len() == 1);
}

// ── H6: fleet patching ──────────────────────────────────────────────────────

use homelab_core::ops::patch::patch_fleet;

#[tokio::test]
async fn h6_patch_runs_apt_in_each_managed_stack_sequentially() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let targets = vec![("alpha".to_string(), 108u16), ("beta".to_string(), 109u16)];
    let report = patch_fleet(&ctx(&exec, &sink, &j), &targets).await;
    assert!(report.ok, "{:?}", report.error);
    let apt = exec.calls_containing("dist-upgrade");
    assert_eq!(apt.len(), 2);
    assert!(apt[0].contains("pct exec 108"));
    assert!(apt[1].contains("pct exec 109"));
}

#[tokio::test]
async fn h6_patch_never_touches_no_touch_vmids() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    // Poisoned state: a no-touch vmid somehow ended up in the target list.
    let targets = vec![("evil".to_string(), 101u16), ("ok".to_string(), 108u16)];
    let report = patch_fleet(&ctx(&exec, &sink, &j), &targets).await;
    assert!(report.ok);
    let apt = exec.calls_containing("dist-upgrade");
    assert_eq!(apt.len(), 1, "only the managed vmid gets patched");
    assert!(apt[0].contains("pct exec 108"));
}

#[tokio::test]
async fn h6_patch_fails_closed_on_apt_error() {
    let exec = MockExecutor::new();
    exec.enqueue(
        "dist-upgrade",
        CmdOutput::failed(100, "Could not get lock /var/lib/dpkg/lock"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let targets = vec![("alpha".to_string(), 108u16), ("beta".to_string(), 109u16)];
    let report = patch_fleet(&ctx(&exec, &sink, &j), &targets).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("apt failed in alpha"));
    // beta was never attempted (sequential, fail-closed).
    assert!(exec.calls_containing("pct exec 109").is_empty());
}

#[tokio::test]
async fn g8_tiered_retention_forgets_by_explicit_id() {
    let exec = MockExecutor::new();
    // Two snapshots on the same old day (age ~100d) → older one forgotten.
    exec.respond_always(
        "snapshots --json",
        CmdOutput::ok(
            r#"[{"short_id":"old1","time":"2026-05-03T04:00:00Z"},
                {"short_id":"old2","time":"2026-05-03T09:00:00Z"},
                {"short_id":"new1","time":"2026-08-11T04:00:00Z"}]"#,
        ),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.now_unix = 1_786_428_000; // 2026-08-11
    let report = backup(&c, &manifest(108, "test"), &BackupCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let forgets = exec.calls_containing("restic forget");
    assert_eq!(forgets.len(), 1);
    assert!(
        forgets[0].contains("old1"),
        "older same-bucket snapshot forgotten"
    );
    assert!(!forgets[0].contains("new1"), "newest never forgotten");
    assert!(forgets[0].contains("--prune"));
}
