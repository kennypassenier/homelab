//! fix-117 (compose-policy-first-container, 2026-09-27): the automatic update
//! read `com.homelab.update.policy` from the app's FIRST container and applied
//! it to the whole app. `stacks/paperwork/paperless-db` labels postgres
//! `manual` and redis `auto`: depending on container order, Postgres got the
//! nightly recreate its label forbids, or redis never updated.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::update::{auto_scope, update, AutoScope};
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

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
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
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
        apps: vec!["paperless-db".into()],
    }
}

fn harness(policies: &str) -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config 114",
        CmdOutput::ok("hostname: 114-app-paperwork\n"),
    );
    exec.respond_first("busy-check", CmdOutput::ok("<no value>\n"));
    exec.respond_first("com.homelab.update.policy", CmdOutput::ok(policies));
    exec.respond_always(
        "docker inspect --format",
        CmdOutput::ok("sha256:old img:latest\n"),
    );
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("postgres\nredis\n"),
    );
    exec
}

fn pulls(exec: &MockExecutor) -> Vec<String> {
    exec.calls_containing("compose pull")
}

#[test]
fn the_scope_is_read_per_service() {
    let p = |v: &[(&str, &str)]| -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    assert_eq!(
        auto_scope(&p(&[("auto", "redis"), ("auto", "postgres")])),
        AutoScope::All
    );
    assert_eq!(
        auto_scope(&p(&[("manual", "postgres"), ("auto", "redis")])),
        AutoScope::Only(vec!["redis".into()])
    );
    assert_eq!(auto_scope(&p(&[("manual", "postgres")])), AutoScope::Skip);
    assert_eq!(auto_scope(&[]), AutoScope::Skip);
}

#[tokio::test]
async fn a_mixed_app_updates_only_its_auto_services() {
    let exec = harness("manual|postgres\nauto|redis\n");
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update(&ctx(&exec, &sink, &j), &paperwork(), None, true).await;
    assert!(r.ok, "{:?}", r.error);
    let p = pulls(&exec);
    assert_eq!(p.len(), 1, "{:?}", exec.calls());
    assert!(p[0].trim_end().ends_with("pull -q redis"), "{}", p[0]);
    let ups = exec.calls_containing("compose up -d");
    assert_eq!(ups.len(), 1, "{:?}", exec.calls());
    assert!(
        ups[0].contains("up -d --no-deps redis") && !ups[0].contains("postgres"),
        "postgres keeps its manual label: {}",
        ups[0]
    );
}

#[tokio::test]
async fn the_order_of_the_containers_decides_nothing() {
    // The same app with postgres listed second: before fix-117 the first
    // container's `auto` updated postgres too.
    let exec = harness("auto|redis\nmanual|postgres\n");
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update(&ctx(&exec, &sink, &j), &paperwork(), None, true).await;
    assert!(r.ok, "{:?}", r.error);
    let ups = exec.calls_containing("compose up -d");
    assert!(
        ups.iter()
            .all(|u| !u.contains("postgres") && u.contains("redis")),
        "{:?}",
        ups
    );
}

#[tokio::test]
async fn an_all_auto_app_is_updated_whole_as_before() {
    let exec = harness("auto|redis\nauto|postgres\n");
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update(&ctx(&exec, &sink, &j), &paperwork(), None, true).await;
    assert!(r.ok, "{:?}", r.error);
    let p = pulls(&exec);
    assert!(p[0].trim_end().ends_with("compose pull -q"), "{}", p[0]);
    assert_eq!(
        exec.calls_containing("up -d --remove-orphans").len(),
        1,
        "{:?}",
        exec.calls()
    );
}
