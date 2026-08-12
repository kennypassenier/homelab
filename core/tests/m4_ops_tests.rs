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
            gpu: false,
            vpn: false,
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
        kea: None,
    }
}

fn mock_hostname(exec: &MockExecutor, vmid: u16, stack: &str) {
    exec.respond_always(
        &format!("pct config {}", vmid),
        CmdOutput::ok(&format!("hostname: {}-app-{}\n", vmid, stack)),
    );
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
    mock_hostname(&exec, 108, "test");
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
    mock_hostname(&exec, 108, "test");
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
    mock_hostname(&exec, 108, "test");
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
    mock_hostname(&exec, 108, "test");
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
    mock_hostname(&exec, 108, "test");
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
    mock_hostname(&exec, 108, "test");
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
    mock_hostname(&exec, 108, "test");
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

// ── B4: drift detection building blocks ─────────────────────────────────────

#[test]
fn b4_intent_hash_changes_with_any_file_edit() {
    use homelab_core::manifest::{intent_hash, DeploySpec, FileBlob};
    let base = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    };
    let h1 = intent_hash(&base);
    assert_eq!(h1, intent_hash(&base), "deterministic");
    let mut edited = base.clone();
    edited.files[0].content.push('\n');
    assert_ne!(h1, intent_hash(&edited), "one byte flips the hash");
    let mut env_changed = base.clone();
    env_changed.env.insert("app".into(), "SECRET=x".into());
    assert_ne!(h1, intent_hash(&env_changed), "env is part of intent");
}

// ── H4: hardware passthrough flags ──────────────────────────────────────────

#[tokio::test]
async fn h4_gpu_and_vpn_flags_produce_device_config() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    // qm status must fail (vmid is not a VM), pct config fails (not existing) → create path.
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    exec.seed_file(
        "/etc/pve/lxc/108.conf",
        "arch: amd64\nhostname: 108-app-test\n",
    );
    let mut m = manifest(108, "test");
    m.lxc.gpu = true;
    m.lxc.vpn = true;
    let spec = DeploySpec {
        manifest: m,
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    // GPU: exact targeted dev entries with render/video gids — never chmod.
    let dev = exec.calls_containing("--dev0 /dev/dri/card0,gid=44");
    assert_eq!(dev.len(), 1);
    assert!(dev[0].contains("--dev1 /dev/dri/renderD128,gid=104"));
    assert!(
        exec.calls_containing("chmod").is_empty(),
        "no ansible chmod-recurse bug"
    );
    // VPN: raw lxc lines appended to the container config.
    let conf = exec.file("/etc/pve/lxc/108.conf").unwrap();
    assert!(conf.contains("lxc.cgroup2.devices.allow: c 10:200 rwm"));
    assert!(conf.contains("lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file"));
}

#[tokio::test]
async fn h4_no_flags_no_device_config() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let spec = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec.calls_containing("--dev0").is_empty());
}

// ── A6: exec guard ──────────────────────────────────────────────────────────

#[test]
fn a6_exec_guard_deny_by_default_and_no_touch_always() {
    use homelab_core::safety::exec_guard;
    let cfg = SafetyConfig::default();
    // Disabled (the default): refused even for a managed vmid.
    assert!(exec_guard(false, &cfg, 108).is_err());
    // Enabled: managed vmid allowed.
    assert!(exec_guard(true, &cfg, 108).is_ok());
    // Enabled: no-touch vmids refused regardless.
    for vmid in [101u16, 104, 106] {
        let err = exec_guard(true, &cfg, vmid).unwrap_err();
        assert!(format!("{}", err).contains("no-touch"));
    }
}

// ── D5: mirror push ─────────────────────────────────────────────────────────

#[tokio::test]
async fn d5_mirror_adds_remote_once_and_pushes() {
    use homelab_core::ops::mirror::mirror_push;
    let exec = MockExecutor::new();
    exec.enqueue(
        "remote get-url mirror",
        CmdOutput::failed(2, "no such remote"),
    );
    mirror_push(&exec, "/var/lib/homelab/repo", "git@github.com:k/m.git")
        .await
        .unwrap();
    assert_eq!(
        exec.calls_containing("remote add mirror git@github.com:k/m.git")
            .len(),
        1
    );
    assert_eq!(exec.calls_containing("push --quiet mirror --all").len(), 1);
    // Second run: remote exists, no re-add.
    mirror_push(&exec, "/var/lib/homelab/repo", "git@github.com:k/m.git")
        .await
        .unwrap();
    assert_eq!(
        exec.calls_containing("remote add").len(),
        1,
        "remote added once"
    );
}

#[tokio::test]
async fn d5_mirror_push_failure_is_an_error_not_a_panic() {
    use homelab_core::ops::mirror::mirror_push;
    let exec = MockExecutor::new();
    exec.respond_always(
        "push --quiet mirror",
        CmdOutput::failed(128, "could not resolve host"),
    );
    let err = mirror_push(&exec, "/r", "git@x:y.git").await.unwrap_err();
    assert!(format!("{}", err).contains("could not resolve host"));
}

// ── B8: golden template ─────────────────────────────────────────────────────

use homelab_core::ops::template::{build_template, TemplateCfg};

#[tokio::test]
async fn b8_template_build_owns_only_its_temp_vmid() {
    let exec = MockExecutor::new();
    exec.enqueue("pct config 999", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = build_template(&ctx(&exec, &sink, &j), &TemplateCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    // Every mutating pct call targets vmid 999 and nothing else.
    for call in exec.calls_containing("pct ") {
        if call.contains("create")
            || call.contains("stop")
            || call.contains("template")
            || call.contains("start")
            || call.contains("set")
        {
            assert!(call.contains("999"), "stray vmid in: {}", call);
        }
    }
    assert_eq!(exec.calls_containing("pct template 999").len(), 1);
    // Machine identity is scrubbed before conversion.
    assert!(!exec.calls_containing("rm -f /etc/machine-id").is_empty());
}

#[tokio::test]
async fn b8_template_build_refuses_no_touch_and_existing_vmids() {
    let sink = VecSink::new();
    let j = NullJournal;
    // No-touch vmid refused before anything runs.
    let exec = MockExecutor::new();
    let cfg = TemplateCfg {
        temp_vmid: 104,
        ..Default::default()
    };
    let report = build_template(&ctx(&exec, &sink, &j), &cfg).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("no-touch"));
    assert!(exec.calls_containing("pct create").is_empty());
    // Existing vmid refused (would destroy someone's container).
    let exec = MockExecutor::new();
    exec.respond_always("pct config 999", CmdOutput::ok("hostname: something\n"));
    let report = build_template(&ctx(&exec, &sink, &j), &TemplateCfg::default()).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("already exists"));
}

#[tokio::test]
async fn b8_clone_template_provisions_via_pct_clone() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let mut m = manifest(108, "test");
    m.lxc.template = "clone:999".into();
    m.resources.disk_gb = 8;
    let spec = DeploySpec {
        manifest: m,
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(exec.calls_containing("pct clone 999 108").len(), 1);
    assert!(
        exec.calls_containing("pct create").is_empty(),
        "no full create on clone path"
    );
    assert_eq!(exec.calls_containing("pct resize 108 rootfs 8G").len(), 1);
    // Docker probe succeeded → bootstrap skipped the install.
    assert!(exec.calls_containing("get.docker.com").is_empty());
}

// ── C4: hot-apply resources ─────────────────────────────────────────────────

use homelab_core::ops::resize::hot_apply;

fn resize_exec(mem: u32, cores: u32, disk: u32, running: bool) -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok(&format!(
            "hostname: 108-app-test\nmemory: {}\ncores: {}\nrootfs: local-lvm:vm-108-disk-0,size={}G\n",
            mem, cores, disk
        )),
    );
    exec.respond_always(
        "pct status",
        CmdOutput::ok(if running {
            "status: running"
        } else {
            "status: stopped"
        }),
    );
    exec
}

#[tokio::test]
async fn c4_grow_applies_live() {
    let exec = resize_exec(512, 1, 4, true);
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = manifest(108, "test");
    m.resources.memory_mb = 1024;
    m.resources.cores = 2;
    m.resources.disk_gb = 8;
    let report = hot_apply(&ctx(&exec, &sink, &j), &m).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(exec.calls_containing("--memory 1024 --cores 2").len(), 1);
    assert_eq!(exec.calls_containing("pct resize 108 rootfs 8G").len(), 1);
}

#[tokio::test]
async fn c4_shrink_refused_while_running() {
    let exec = resize_exec(2048, 4, 8, true);
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = manifest(108, "test");
    m.resources.memory_mb = 512;
    m.resources.cores = 1;
    let report = hot_apply(&ctx(&exec, &sink, &j), &m).await;
    assert!(!report.ok);
    assert!(report
        .error
        .unwrap()
        .why
        .contains("shrink refused while running"));
    assert!(exec.calls_containing("pct set 108 --memory").is_empty());
}

#[tokio::test]
async fn c4_ram_shrink_allowed_when_stopped_but_disk_never() {
    // Stopped: RAM/cores may shrink…
    let exec = resize_exec(2048, 4, 8, false);
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = manifest(108, "test");
    m.resources.memory_mb = 512;
    m.resources.cores = 1;
    m.resources.disk_gb = 8;
    let report = hot_apply(&ctx(&exec, &sink, &j), &m).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(exec.calls_containing("--memory 512").len(), 1);
    // …but a disk shrink is refused even stopped.
    let exec = resize_exec(512, 1, 8, false);
    let mut m = manifest(108, "test");
    m.resources.disk_gb = 4;
    let report = hot_apply(&ctx(&exec, &sink, &j), &m).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("disk shrink refused"));
}

#[tokio::test]
async fn c4_no_touch_and_hostname_guarded() {
    let sink = VecSink::new();
    let j = NullJournal;
    let exec = resize_exec(512, 1, 4, true);
    let report = hot_apply(&ctx(&exec, &sink, &j), &manifest(104, "evil")).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("no-touch"));
    // Wrong hostname refused too.
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 108-app-other\nmemory: 512\ncores: 1\n"),
    );
    let report = hot_apply(&ctx(&exec, &sink, &j), &manifest(108, "test")).await;
    assert!(!report.ok);
}

// ── H2: Kea DHCP reservations ───────────────────────────────────────────────

fn ctx_with_kea<'a>(
    exec: &'a MockExecutor,
    sink: &'a VecSink,
    journal: &'a NullJournal,
) -> OpCtx<'a> {
    let mut c = ctx(exec, sink, journal);
    c.kea = Some(homelab_core::ops::kea::KeaCfg {
        base_url: "https://10.10.10.1".into(),
        cred_file: "/var/lib/homelab/secrets/opnsense".into(),
    });
    c
}

#[tokio::test]
async fn h2_deploy_registers_kea_reservation_on_create() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist")); // safety gate
    exec.respond_always(
        "pct config 108",
        CmdOutput::ok(
            "hostname: 108-app-test\nnet0: name=eth0,hwaddr=BC:24:11:AA:BB:CC,ip=10.10.10.8/24\n",
        ),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    exec.respond_always(
        "search_subnet",
        CmdOutput::ok(r#"{"rows":[{"uuid":"sub-1","subnet":"10.10.10.0/24"}]}"#),
    );
    exec.respond_always("search_reservation", CmdOutput::ok(r#"{"rows":[]}"#));
    exec.respond_always("add_reservation", CmdOutput::ok(r#"{"result":"saved"}"#));
    exec.respond_always("reconfigure", CmdOutput::ok(r#"{"status":"ok"}"#));
    let spec = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx_with_kea(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    let adds = exec.calls_containing("add_reservation");
    assert_eq!(adds.len(), 1);
    assert!(adds[0].contains("10.10.10.8"));
    assert!(adds[0].contains("BC:24:11:AA:BB:CC"));
    assert!(adds[0].contains("sub-1"));
    assert_eq!(exec.calls_containing("service/reconfigure").len(), 1);
    // The secret is read inside the shell, never in argv.
    assert!(adds[0].contains("$(cat /var/lib/homelab/secrets/opnsense)"));
}

#[tokio::test]
async fn h2_kea_failure_never_blocks_deploy() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config 108",
        CmdOutput::ok("hostname: 108-app-test\nnet0: name=eth0,hwaddr=BC:24:11:AA:BB:CC\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    // OPNsense down.
    exec.respond_always("search_subnet", CmdOutput::failed(7, "connection refused"));
    let spec = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx_with_kea(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "kea failure must not block: {:?}", report.error);
}

#[tokio::test]
async fn h2_no_kea_config_no_api_calls() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let spec = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok);
    assert!(exec.calls_containing("api/kea").is_empty());
}

// ── V8: undeclared /appdata bind is a validation error ─────────────────────

#[test]
fn v8_validate_rejects_undeclared_appdata_bind() {
    use homelab_core::manifest::{validate, DeploySpec, FileBlob};
    let mut m = manifest(108, "test");
    m.storage.clear(); // nothing declared
    let spec = DeploySpec {
        manifest: m,
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services:\n  app:\n    volumes:\n      - /appdata/test/app-config:/config\n"
                .into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    };
    let err = validate(&spec).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("not declared under storage"), "got: {}", msg);
    // Declaring it fixes the spec.
    let mut ok_spec = spec;
    ok_spec.manifest.storage = vec![homelab_core::manifest::MountSpec {
        host_path: "/appdata/test/app-config".into(),
        mount_point: "/appdata/test/app-config".into(),
        host_owner_uid: Some(101000),
    }];
    validate(&ok_spec).unwrap();
}

// ── A1 property: every no-touch vmid refused in EVERY mutating op ───────────

#[tokio::test]
async fn a1_property_every_no_touch_vmid_refused_in_every_op() {
    use homelab_core::safety::DEFAULT_NO_TOUCH;
    for &vmid in DEFAULT_NO_TOUCH {
        let m = manifest(vmid, "evil");
        let sink = VecSink::new();
        let j = NullJournal;
        // backup
        let exec = MockExecutor::new();
        let r = backup(&ctx(&exec, &sink, &j), &m, &BackupCfg::default()).await;
        assert!(!r.ok, "backup must refuse vmid {}", vmid);
        assert!(exec.calls_containing("restic").is_empty());
        // restore
        let exec = MockExecutor::new();
        let r = restore(&ctx(&exec, &sink, &j), &m, &BackupCfg::default(), "latest").await;
        assert!(!r.ok, "restore must refuse vmid {}", vmid);
        assert!(exec.calls_containing("compose down").is_empty());
        // update
        let exec = MockExecutor::new();
        let r = update(&ctx(&exec, &sink, &j), &m, None, false).await;
        assert!(!r.ok, "update must refuse vmid {}", vmid);
        assert!(exec.calls_containing("compose pull").is_empty());
    }
}

#[tokio::test]
async fn a2_hostname_mismatch_refused_in_backup_restore_update() {
    let sink = VecSink::new();
    let j = NullJournal;
    let m = manifest(108, "test");
    for op in ["backup", "restore", "update"] {
        let exec = MockExecutor::new();
        exec.respond_always("pct config 108", CmdOutput::ok("hostname: 108-app-other\n"));
        let r = match op {
            "backup" => backup(&ctx(&exec, &sink, &j), &m, &BackupCfg::default()).await,
            "restore" => restore(&ctx(&exec, &sink, &j), &m, &BackupCfg::default(), "latest").await,
            _ => update(&ctx(&exec, &sink, &j), &m, None, false).await,
        };
        assert!(!r.ok, "{} must refuse a hostname mismatch", op);
        assert!(r.error.unwrap().why.contains("refusing"));
    }
}

// ── H2 hardening: resume always runs; stale locks cleared ───────────────────

#[tokio::test]
async fn h2_failed_snapshot_still_resumes_containers() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always(
        "restic backup",
        CmdOutput::failed(1, "rclone: upload timeout"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        &BackupCfg::default(),
    )
    .await;
    assert!(!report.ok, "snapshot failure must still fail the op");
    // …but the paused containers were restarted anyway.
    assert!(
        !exec.calls_containing("docker compose up -d").is_empty(),
        "resume must run even when the snapshot fails"
    );
    // And stale locks were cleared up front.
    assert_eq!(exec.calls_containing("restic unlock").len(), 1);
}

// ── E3 auto-restore, D3 garbage collection, H6 host-side routes ─────────────

fn deploy_mocks(exec: &MockExecutor) {
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
}

fn deploy_spec(m: StackManifest) -> homelab_core::manifest::DeploySpec {
    homelab_core::manifest::DeploySpec {
        manifest: m,
        files: vec![homelab_core::manifest::FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
    }
}

#[tokio::test]
async fn e3_empty_dirs_with_snapshot_trigger_restore() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    // ls -A returns empty (dir empty); a snapshot exists.
    exec.respond_always(
        "snapshots --last --json",
        CmdOutput::ok(r#"[{"short_id":"abc"}]"#),
    );
    exec.respond_always("restic restore", CmdOutput::ok("restored"));
    let spec = deploy_spec(manifest(108, "test"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.calls_containing("restic restore latest --target /")
            .len(),
        1
    );
}

#[tokio::test]
async fn e3_nonempty_dirs_skip_restore_and_restic_failure_never_blocks() {
    use homelab_core::ops::deploy::deploy;
    // Non-empty: no restore attempted.
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(manifest(108, "test"))).await;
    assert!(report.ok);
    assert!(exec.calls_containing("restic restore").is_empty());
    // Empty + restic restore fails: loud warning, deploy still green.
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always(
        "snapshots --last --json",
        CmdOutput::ok(r#"[{"short_id":"abc"}]"#),
    );
    exec.respond_always("restic restore", CmdOutput::failed(1, "gdrive down"));
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(manifest(108, "test"))).await;
    assert!(
        report.ok,
        "restic failure must not block: {:?}",
        report.error
    );
    assert!(sink
        .lines()
        .iter()
        .any(|l| l.contains("AUTO-RESTORE FAILED")));
}

#[tokio::test]
async fn e3_env_restored_from_vault_when_client_sends_none() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    exec.seed_file(
        "/var/lib/homelab/secrets/test/app.env",
        "API_KEY=from_the_vault\n",
    );
    let spec = deploy_spec(manifest(108, "test")); // env is empty
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    // The vault env was pushed to the container (via the staged tmp file).
    assert!(
        !exec
            .calls_containing("pct push 108 /var/lib/homelab/push-staging /opt/test/app/.env")
            .is_empty(),
        "vault fallback must push the .env"
    );
}

#[tokio::test]
async fn d3_removed_app_is_stopped_and_deleted_but_config_kept() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    // Previous state knows apps [app, oldapp]; the new spec only has [app].
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"test":{"vmid":108,"hostname":"108-app-test","apps":["app","oldapp"],"applied_at":1}}}"#,
    );
    let spec = deploy_spec(manifest(108, "test"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    let gc = exec.calls_containing("/opt/test/oldapp");
    assert!(
        gc.iter().any(|c| c.contains("docker compose down")),
        "removed app must be composed down"
    );
    assert!(
        gc.iter().any(|c| c.contains("rm -rf")),
        "and its /opt dir removed"
    );
    // Config dirs under /appdata are never touched by GC.
    assert!(!exec
        .calls()
        .iter()
        .any(|c| c.contains("rm") && c.contains("/appdata/")));
    // The surviving app was untouched by GC.
    assert!(!exec
        .calls_containing("rm -rf '/opt/test/app'")
        .iter()
        .any(|_| true));
}

#[tokio::test]
async fn h6_appdata_routes_dir_written_host_side() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    let mut spec = deploy_spec(manifest(108, "test"));
    spec.gateway_route = Some(homelab_core::manifest::GatewayRoute {
        gateway_vmid: 104,
        filename: "108-app-test.yml".into(),
        content: "http:\n  routers: {}\n".into(),
    });
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.safety.gateway_routes_dir = "/appdata/platform/traefik-config/routes".into();
    let report = deploy(&c, &spec).await;
    assert!(report.ok, "{:?}", report.error);
    // Written directly on the host, NOT pushed into the gateway container.
    assert!(exec
        .file("/appdata/platform/traefik-config/routes/108-app-test.yml")
        .is_some());
    assert!(exec.calls_containing("pct push 104").is_empty());
}

// ── H7 state fail-loud, H10 host-meta backup ────────────────────────────────

#[tokio::test]
async fn h7_corrupt_state_fails_loud_and_quarantines() {
    use homelab_core::state::StateStore;
    let exec = MockExecutor::new();
    exec.seed_file("/var/lib/homelab/state.json", "{ this is not json !!");
    let store = StateStore::new(&exec, "/var/lib/homelab");
    let err = store.load().await.unwrap_err();
    assert!(format!("{}", err).contains("does not parse"));
    // The corrupt content was preserved for forensics.
    assert!(exec.file("/var/lib/homelab/state.json.corrupt").is_some());
    // A deploy over corrupt state must fail, not silently erase the fleet.
    use homelab_core::ops::deploy::deploy;
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(manifest(108, "test"))).await;
    assert!(!report.ok, "deploy must refuse to run over corrupt state");
}

#[tokio::test]
async fn h7_newer_schema_refused_missing_file_is_fresh() {
    use homelab_core::state::StateStore;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":99,"stacks":{}}"#,
    );
    let store = StateStore::new(&exec, "/var/lib/homelab");
    assert!(format!("{}", store.load().await.unwrap_err()).contains("newer"));
    let exec = MockExecutor::new();
    let store = StateStore::new(&exec, "/var/lib/homelab");
    assert!(
        store.load().await.unwrap().stacks.is_empty(),
        "missing file = fresh"
    );
}

#[tokio::test]
async fn h10_host_meta_backup_snapshots_vault_state_tls() {
    use homelab_core::ops::backup::backup_host_meta;
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup_host_meta(&ctx(&exec, &sink, &j), &BackupCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let snap = exec.calls_containing("restic backup");
    assert_eq!(snap.len(), 1);
    for path in ["/var/lib/homelab/secrets", "state.json", "tls-key.pem"] {
        assert!(
            snap[0].contains(path),
            "missing {} in host-meta snapshot",
            path
        );
    }
    assert!(snap[0].contains("host-meta"), "dedicated repo");
}

// ── H14: journal wiring actually asserted ───────────────────────────────────

use std::sync::Mutex as StdMutex;

struct RecJournal(StdMutex<Vec<(String, String, String)>>);
impl homelab_core::runner::Journal for RecJournal {
    fn record(&self, op: &str, step: &str, status: &str) {
        self.0
            .lock()
            .unwrap()
            .push((op.into(), step.into(), status.into()));
    }
}

#[tokio::test]
async fn h14_every_destroy_step_is_journaled_running_then_done() {
    let exec = MockExecutor::new();
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test\n"));
    let sink = VecSink::new();
    let j = RecJournal(StdMutex::new(Vec::new()));
    let report = destroy(
        &OpCtx {
            exec: &exec,
            sink: &sink,
            journal: &j,
            safety: SafetyConfig::default(),
            state_dir: "/var/lib/homelab".into(),
            now_unix: 1_760_000_000,
            kea: None,
        },
        "test",
        108,
        "test",
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let records = j.0.lock().unwrap();
    assert!(!records.is_empty(), "destroy must journal its steps");
    // Every step appears as running BEFORE done, per step, in order.
    for pair in records.windows(2) {
        if pair[0].1 == pair[1].1 {
            assert_eq!(pair[0].2, "running");
            assert!(pair[1].2 == "done" || pair[1].2 == "failed");
        }
    }
    assert!(records
        .iter()
        .any(|r| r.1 == "destroy container" && r.2 == "done"));
}

#[tokio::test]
async fn h14_failed_step_leaves_running_then_failed_trail() {
    let exec = MockExecutor::new();
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test\n"));
    exec.respond_always("pct destroy", CmdOutput::failed(1, "device busy"));
    let sink = VecSink::new();
    let j = RecJournal(StdMutex::new(Vec::new()));
    let report = destroy(
        &OpCtx {
            exec: &exec,
            sink: &sink,
            journal: &j,
            safety: SafetyConfig::default(),
            state_dir: "/var/lib/homelab".into(),
            now_unix: 1_760_000_000,
            kea: None,
        },
        "test",
        108,
        "test",
    )
    .await;
    assert!(!report.ok);
    let records = j.0.lock().unwrap();
    let destroy_records: Vec<_> = records
        .iter()
        .filter(|r| r.1 == "destroy container")
        .collect();
    assert_eq!(destroy_records.len(), 2);
    assert_eq!(destroy_records[0].2, "running");
    assert_eq!(destroy_records[1].2, "failed");
    // AR13 parses exactly this trail after a crash: running without done.
}
