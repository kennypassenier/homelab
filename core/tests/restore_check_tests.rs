//! fix-147 (restore-check-failure, Kenny 2026-09-27, form "Keuzes helpers"):
//! when a deploy finds a data directory empty and cannot check its backup
//! for any reason other than "no repository" (fix-54 made that a warning),
//! the deploy still goes on, and the stack gets a Broken fleet-check finding
//! that stays until a later snapshot check of that directory succeeds.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::deploy::deploy;
use homelab_core::ops::fleetcheck::{evaluate_restore_checks, Severity};
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;
use homelab_core::state::{HostState, RestoreCheckFailure};

const NOW: u64 = 1_760_000_000;
const KEY: &str = "test:/appdata/test/test-config";

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

fn spec() -> DeploySpec {
    DeploySpec {
        secret_files: Vec::new(),
        source: None,
        extra_routes: Vec::new(),
        native_binaries: Default::default(),
        native_manifests: Default::default(),
        manifest: StackManifest {
            home_address_whitelist: None,
            tiles: Default::default(),
            log_files: Vec::new(),
            firewall: None,
            registry_login: None,
            retention: None,
            data_mounts: Vec::new(),
            native_only: false,
            on_demand: false,
            syslog_receivers: vec![],
            natives: Vec::new(),
            stack_name: "test".into(),
            vmid: 108,
            hostname: "108-app-test".into(),
            network: NetworkSpec {
                ip: "10.10.10.8/24".into(),
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
                timezone: "host".into(),
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
                no_data: false,
                no_backup: None,
                host_owner_uid: Some(101000),
                app: None,
            }],
            apps: vec!["app".into()],
        },
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    }
}

fn deploy_mocks(exec: &MockExecutor) {
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    exec.respond_always("pct config 999", CmdOutput::ok("unprivileged: 1"));
}

fn state(exec: &MockExecutor) -> HostState {
    serde_json::from_str(&exec.file("/var/lib/homelab/state.json").unwrap_or_default())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_check_that_fails_is_remembered_after_the_deploy_goes_on() {
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always(
        "snapshots --last --json",
        CmdOutput::failed(1, "Fatal: unable to open repository: rclone: couldn't list"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let r = deploy(&ctx(&exec, &sink, &j), &spec()).await;
    assert!(
        r.ok,
        "backup-target trouble never blocks a deploy: {:?}",
        r.error
    );
    let st = state(&exec);
    let rec = st
        .restore_check_failures
        .get(KEY)
        .unwrap_or_else(|| panic!("remembered: {:?}", st.restore_check_failures));
    assert_eq!(rec.stack, "test");
    assert_eq!(rec.at, NOW);
    assert!(rec.why.contains("unable to open repository"), "{}", rec.why);
}

#[tokio::test]
async fn a_later_check_that_succeeds_clears_it() {
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    let mut before = HostState::default();
    before.restore_check_failures.insert(
        KEY.into(),
        RestoreCheckFailure {
            stack: "test".into(),
            what: "/appdata/test/test-config".into(),
            at: NOW - 86_400,
            why: "rc=1 :: unable to open repository".into(),
        },
    );
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::to_string(&before).unwrap(),
    );
    exec.respond_always("snapshots --last --json", CmdOutput::ok("[]"));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = deploy(&ctx(&exec, &sink, &j), &spec()).await;
    assert!(r.ok, "{:?}", r.error);
    assert!(
        state(&exec).restore_check_failures.is_empty(),
        "{:?}",
        state(&exec).restore_check_failures
    );
}

#[test]
fn a_remembered_failure_is_a_broken_finding_on_its_stack() {
    let mut st = HostState::default();
    st.restore_check_failures.insert(
        KEY.into(),
        RestoreCheckFailure {
            stack: "test".into(),
            what: "/appdata/test/test-config".into(),
            at: NOW,
            why: "rc=1 :: unable to open repository".into(),
        },
    );
    let f = evaluate_restore_checks(&st);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Broken);
    assert_eq!(f[0].subject, "test");
    assert!(
        f[0].what.contains("/appdata/test/test-config"),
        "{}",
        f[0].what
    );
    assert!(
        f[0].what.contains("unable to open repository"),
        "{}",
        f[0].what
    );
}
