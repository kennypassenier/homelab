//! fix-118 (compose-update-verify-weak, 2026-09-27): the compose update's
//! verify asked "did one service start", read once right after `up -d`. A
//! two-service app with a crashed Postgres, or a container in a restart loop
//! that is `running` part of each cycle, passed as a good update. F300 fixed
//! exactly this for native units; this ports it.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::update::{settle_script, update};
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

const CAPTURE: &str = "{{.Image}} {{.Config.Image}}";

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
    }
}

fn paperwork() -> StackManifest {
    StackManifest {
        homepage_widgets: Default::default(),
        home_address_whitelist: None,
        generated_dashboards_command: None,
        tiles: Default::default(),
        firewall: None,
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        on_demand: false,
        syslog_receivers: vec![],
        natives: Vec::new(),
        stack_name: "paperwork".into(),
        vmid: 114,
        hostname: "114-app-paperwork".into(),
        network: NetworkSpec {
            ip: "10.10.10.14/24".into(),
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
        storage: vec![],
        apps: vec!["paperless".into()],
    }
}

/// An app whose image changes with the update (`new` after `up`) or not.
fn harness(new_image: bool) -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config 114",
        CmdOutput::ok("hostname: 114-app-paperwork\n"),
    );
    exec.respond_first("busy-check", CmdOutput::ok("<no value>\n"));
    exec.enqueue(CAPTURE, CmdOutput::ok("sha256:old img:latest\n"));
    exec.respond_always(
        CAPTURE,
        CmdOutput::ok(if new_image {
            "sha256:new img:latest\n"
        } else {
            "sha256:old img:latest\n"
        }),
    );
    exec.respond_always(
        "--services --status running",
        CmdOutput::ok("paperless\npostgres\n"),
    );
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("paperless\npostgres\n"),
    );
    exec
}

#[test]
fn the_settle_check_ports_f300() {
    let s = settle_script(
        "paperwork",
        "paperless",
        &["paperless".to_string(), "postgres".to_string()],
        "",
    );
    // Every service that ran before must run, and keep running.
    assert!(s.contains("paperless postgres"), "{}", s);
    // A counter, not a sample: a restart loop moves it however lucky the
    // timing of the samples.
    assert!(s.contains("RestartCount"), "{}", s);
    // A declared healthcheck must say healthy.
    assert!(s.contains("Health.Status"), "{}", s);
    for token in [
        "NOT_RUNNING",
        "DIED_IN_WINDOW",
        "RESTART_LOOP",
        "UNHEALTHY",
        "NEVER_HEALTHY",
    ] {
        assert!(s.contains(token), "missing diagnosis {}: {}", token, s);
    }
}

#[tokio::test]
async fn a_restart_loop_after_the_update_is_rolled_back() {
    let exec = harness(true);
    exec.respond_first("RestartCount", CmdOutput::failed(1, "RESTART_LOOP"));
    exec.enqueue("RestartCount", CmdOutput::failed(1, "RESTART_LOOP"));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update(&ctx(&exec, &sink, &j), &paperwork(), None, false).await;
    assert!(!r.ok, "a restart loop is not a good update");
    assert!(
        !exec
            .calls_containing("docker tag sha256:old img:latest")
            .is_empty(),
        "rolled back to the captured image: {:?}",
        exec.calls()
    );
}

#[tokio::test]
async fn a_new_image_that_settles_is_kept() {
    let exec = harness(true);
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update(&ctx(&exec, &sink, &j), &paperwork(), None, false).await;
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(exec.calls_containing("RestartCount").len(), 1);
    assert!(exec.calls_containing("docker tag").is_empty());
}

#[tokio::test]
async fn an_unchanged_image_does_not_wait_a_settle_window() {
    let exec = harness(false);
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update(&ctx(&exec, &sink, &j), &paperwork(), None, false).await;
    assert!(r.ok, "{:?}", r.error);
    assert!(
        exec.calls_containing("RestartCount").is_empty(),
        "a night with nothing new must not cost a minute per app: {:?}",
        exec.calls()
    );
}
