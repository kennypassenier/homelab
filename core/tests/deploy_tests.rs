//! The M0 safety and idempotency suite — every scenario here maps to a
//! FEATURES.md test scenario (A1, A2, A3/D10, B1, D1, A5).

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::{deploy::deploy, OpCtx};
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
            template: "local:vztmpl/debian-12-standard_12.12-1_amd64.tar.zst".into(),
            unprivileged: true,
            features: "nesting=1,keyctl=1".into(),
            protection: false,
            gpu: false,
            vpn: false,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![MountSpec {
            host_path: "/appdata/syncthing/syncthing-config".into(),
            mount_point: "/appdata/syncthing/syncthing-config".into(),
            host_owner_uid: Some(101000),
            app: Some("syncthing".into()),
        }],
        apps: vec!["syncthing".into()],
    }
}

fn spec(vmid: u16, stack: &str) -> DeploySpec {
    DeploySpec {
        manifest: manifest(vmid, stack),
        files: vec![FileBlob {
            path: "syncthing/docker-compose.yml".into(),
            content: "services: {}\n".into(),
            mode: None,
        }],
        env: std::collections::BTreeMap::new(),
        gateway_route: Some(GatewayRoute {
            gateway_vmid: 104,
            filename: "110-app-syncthing.yml".into(),
            content: "http: {}\n".into(),
        }),
    }
}

fn sha_hex(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    let out = h
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    format!("{}\n", out)
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
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
    }
}

/// Script the mock as "fresh host, container does not exist yet".
fn script_fresh(exec: &MockExecutor) {
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct status", CmdOutput::ok("status: stopped"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
}

/// O7: a config path must be named `<app>-config`. Kenny chose the literal
/// rule at the mini-round — "uniformiteit en een duidelijke regel hebben hier
/// voorrang" — over the weaker shape that would have tolerated the live
/// `prometheus-data`. The restore looks paths up by name, so a directory that
/// is renamed loses track of its own snapshots; a rule nothing enforces is
/// how that drifted into three different shapes in the first place.
#[test]
fn o7_config_paths_must_be_named_after_their_app() {
    use homelab_core::manifest::validate;
    let mut s = spec(108, "test");
    s.manifest.apps = vec!["syncthing".into()];
    s.manifest.storage[0].host_path = "/appdata/test/syncthing-data".into();
    s.manifest.storage[0].mount_point = "/appdata/test/syncthing-data".into();
    s.manifest.storage[0].app = Some("syncthing".into());
    let err = validate(&s).expect_err("a path not ending in -config must be refused");
    assert!(
        format!("{}", err).contains("syncthing-config"),
        "the error must name what it should have been: {}",
        err
    );

    // The owner and the directory have to agree, or the name says one thing
    // and the backup does another.
    let mut s = spec(108, "test");
    s.manifest.apps = vec!["syncthing".into(), "other".into()];
    s.manifest.storage[0].host_path = "/appdata/test/other-config".into();
    s.manifest.storage[0].mount_point = "/appdata/test/other-config".into();
    s.manifest.storage[0].app = Some("syncthing".into());
    let err = validate(&s).expect_err("owner and directory name must agree");
    assert!(format!("{}", err).contains("syncthing"), "{}", err);

    // The shape that is right stays right.
    validate(&spec(108, "test")).expect("syncthing-config owned by syncthing is valid");
}

/// O5: on an unprivileged container uid 1000 inside is uid 101000 on the
/// host, so the two privilege levels need host_owner_uid values 100000 apart.
/// Nothing checked, and the wrong number produces a directory the service
/// cannot use while the deploy reports success — the app just does not start.
#[test]
fn o5_host_owner_uid_must_match_the_privilege_level() {
    use homelab_core::manifest::validate;
    // Unprivileged stack given a raw uid: that is the privileged number.
    let mut s = spec(108, "test");
    s.manifest.lxc.unprivileged = true;
    s.manifest.storage[0].host_owner_uid = Some(1000);
    let err = validate(&s).expect_err("raw uid on an unprivileged stack must be refused");
    assert!(
        format!("{}", err).contains("101000"),
        "the error must name the number that would work: {}",
        err
    );

    // Privileged stack given a mapped uid: the other way round.
    let mut s = spec(108, "test");
    s.manifest.lxc.unprivileged = false;
    s.manifest.storage[0].host_owner_uid = Some(101000);
    let err = validate(&s).expect_err("mapped uid on a privileged stack must be refused");
    assert!(format!("{}", err).contains("1000"), "{}", err);

    // The combinations that are right stay right.
    let mut ok = spec(108, "test");
    ok.manifest.lxc.unprivileged = true;
    ok.manifest.storage[0].host_owner_uid = Some(101000);
    validate(&ok).expect("mapped uid on an unprivileged stack is correct");
    let mut ok = spec(108, "test");
    ok.manifest.lxc.unprivileged = false;
    ok.manifest.storage[0].host_owner_uid = Some(1000);
    validate(&ok).expect("raw uid on a privileged stack is correct");
}

// ── A1: no-touch list refuses before ANY command runs ───────────────────────

#[tokio::test]
async fn a1_no_touch_vmid_is_refused_with_zero_commands() {
    for vmid in [100u16, 101, 102, 103] {
        let exec = MockExecutor::new();
        let sink = VecSink::new();
        let journal = NullJournal;
        // Craft a "valid-looking" spec pointing at a protected guest.
        let stack = "evil";
        let report = deploy(&ctx(&exec, &sink, &journal), &spec(vmid, stack)).await;
        assert!(!report.ok, "vmid {} must be refused", vmid);
        let err = report.error.unwrap();
        assert!(
            err.why.contains("no-touch"),
            "unexpected error: {}",
            err.why
        );
        assert!(
            exec.calls().is_empty(),
            "vmid {}: commands ran despite refusal: {:?}",
            vmid,
            exec.calls()
        );
    }
}

// ── A2: hostname guard refuses a mismatched existing container ──────────────

#[tokio::test]
async fn a2_hostname_mismatch_refuses_reuse() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: something-else\ncores: 1\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("refusing"));
    // Probes are allowed; mutations are not.
    for forbidden in [
        "pct create",
        "pct push",
        "pct set",
        "pct start",
        "compose up",
    ] {
        assert!(
            exec.calls_containing(forbidden).is_empty(),
            "mutating call {} happened after refusal",
            forbidden
        );
    }
}

// ── A3/D10: invalid input aborts with zero commands ─────────────────────────

#[tokio::test]
async fn d10_path_traversal_is_refused_before_any_command() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing");
    s.files.push(FileBlob {
        path: "../escape/evil.yml".into(),
        content: "x".into(),
        mode: None,
    });
    let report = deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("escapes the stack root"));
    assert!(exec.calls().is_empty());
}

#[tokio::test]
async fn d10_validator_collects_all_problems() {
    let mut s = spec(110, "syncthing");
    s.manifest.stack_name = "Bad Name".into();
    s.manifest.resources.memory_mb = 64;
    let err = homelab_core::manifest::validate(&s).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("stack_name"), "{}", msg);
    assert!(msg.contains("memory_mb"), "{}", msg);
    assert!(msg.contains("hostname"), "{}", msg); // canonical name changed too
}

// ── D1: fresh deploy runs the full ordered sequence ─────────────────────────

#[tokio::test]
async fn d1_fresh_deploy_command_sequence() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let calls = exec.calls();
    let pos = |needle: &str| {
        calls
            .iter()
            .position(|c| c.contains(needle))
            .unwrap_or_else(|| panic!("missing call containing '{}' in {:#?}", needle, calls))
    };
    // Ordered invariants of the pipeline:
    let create = pos("pct create 110");
    let start = pos("pct start 110");
    let push = pos("pct push 110");
    let up = pos("compose up -d");
    let verify = pos("ps --status running --services");
    let route = pos("pct push 104");
    assert!(create < start, "create before start");
    assert!(start < push, "start before file push");
    assert!(push < up, "files pushed before compose up");
    assert!(up < verify, "verify runs after compose up");
    assert!(verify < route, "gateway route only after health is proven");
    // Boot policy (C3) is part of creation.
    assert!(calls[create].contains("--onboot 1"));
    assert!(calls[create].contains("--startup order=50"));
    // State recorded (AR4).
    let state = exec
        .file("/var/lib/homelab/state.json")
        .expect("state written");
    assert!(state.contains("\"syncthing\""));
    assert!(state.contains("110-app-syncthing"));
}

// ── B1: second run is quiet (no create, no restarts, no re-enable) ──────────

#[tokio::test]
async fn b1_second_run_is_quiet() {
    use homelab_core::ops::guards;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 110-app-syncthing\ncores: 1\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    // Guards find their exact content already in place → no restarts.
    exec.respond_always(
        "sha256sum '/etc/docker/daemon.json'",
        CmdOutput::ok(&sha_hex(guards::DOCKER_DAEMON_JSON)),
    );
    exec.respond_always(
        "sha256sum '/etc/systemd/journald.conf.d/homelab-limits.conf'",
        CmdOutput::ok(&sha_hex(guards::JOURNALD_LIMITS)),
    );
    exec.respond_always(
        "sha256sum '/etc/logrotate.d/homelab'",
        CmdOutput::ok(&sha_hex(guards::LOGROTATE_POLICY)),
    );
    exec.respond_always(
        "sha256sum '/etc/systemd/system/docker-prune.service'",
        CmdOutput::ok(&sha_hex(guards::PRUNE_SERVICE)),
    );
    exec.respond_always(
        "sha256sum '/etc/systemd/system/docker-prune.timer'",
        CmdOutput::ok(&sha_hex(guards::PRUNE_TIMER)),
    );
    exec.respond_always(
        "sha256sum '/etc/apt/apt.conf.d/60homelab-clean'",
        CmdOutput::ok(&sha_hex(guards::APT_AUTOCLEAN)),
    );
    exec.respond_always(
        "sha256sum '/etc/apt/apt.conf.d/50unattended-upgrades'",
        CmdOutput::ok(&sha_hex(guards::UNATTENDED_UPGRADES)),
    );
    // Files already at destination content → pushes skipped too.
    let s = spec(110, "syncthing");
    exec.respond_always(
        "sha256sum '/opt/syncthing/syncthing/docker-compose.yml'",
        CmdOutput::ok(&sha_hex(&s.files[0].content)),
    );
    exec.respond_always(
        "sha256sum '/opt/traefik-config/routes/110-app-syncthing.yml'",
        CmdOutput::ok(&sha_hex(s.gateway_route.as_ref().unwrap().content.as_str())),
    );
    // Intent repo already committed → git commit exits non-zero (nothing to do).
    exec.respond_always(
        "git -C /var/lib/homelab/repo commit",
        CmdOutput::failed(1, "nothing to commit, working tree clean"),
    );

    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(report.ok, "second run failed: {:?}", report.error);

    for forbidden in [
        "pct create",
        "pct start",
        "pct push",
        "systemctl restart docker",
        "systemctl restart systemd-journald",
        "enable --now docker-prune.timer",
    ] {
        assert!(
            exec.calls_containing(forbidden).is_empty(),
            "second run was not quiet: found {}",
            forbidden
        );
    }
}

// ── A5: secrets never land in the intent repo; vault copy is 0600 ───────────

#[tokio::test]
async fn a5_env_goes_to_vault_never_to_repo() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing");
    s.env
        .insert("syncthing".into(), "SECRET_TOKEN=supersecret\n".into());
    let report = deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(report.ok, "{:?}", report.error);

    let vault_path = "/var/lib/homelab/secrets/syncthing/syncthing.env";
    assert_eq!(
        exec.file(vault_path).as_deref(),
        Some("SECRET_TOKEN=supersecret\n")
    );
    assert_eq!(exec.file_mode(vault_path), Some(0o600));
    // Nothing under the repo tree may contain the secret.
    assert!(
        exec.file("/var/lib/homelab/repo/stacks/syncthing/syncthing/.env")
            .is_none(),
        "env file leaked into the intent repo"
    );
    // And the transcript never logs the value.
    for line in sink.lines() {
        assert!(
            !line.contains("supersecret"),
            "secret leaked into transcript: {}",
            line
        );
    }
}

// ── Gateway route constraint (H1) ───────────────────────────────────────────

#[tokio::test]
async fn h1_gateway_route_only_to_the_gateway() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing");
    s.gateway_route = Some(GatewayRoute {
        gateway_vmid: 106, // not the gateway!
        filename: "x.yml".into(),
        content: "http: {}".into(),
    });
    let report = deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(!report.ok);
    assert!(report
        .error
        .unwrap()
        .why
        .contains("gateway routes may only target"));
    assert!(exec.calls_containing("pct push 106").is_empty());
}
