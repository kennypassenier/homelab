//! fix-91 (expert panel, routes-outside-repo-unvalidated; Kenny 2026-09-27:
//! "alles-in-repo-plus-controle"): the route files that were written by hand
//! on the gateway are declared in the repository, and a deploy writes them
//! under the name they already have.
//!
//! Four files on CT 104 carried five public hostnames that no stack file
//! named: `112-app-almanac.yml`, `manual-kyu.yml`, `manual-homeassistant.yml`
//! and `manual-routes.yml` (opn, prox). Three of those names are not the
//! `<vmid>-app-<stack>.yml` a `gateway_route` must use, and renaming them
//! would leave the old file routing the same hostname beside the new one
//! (the F115 shape) until someone removed it by hand. So a stack may declare
//! `extra_routes`: route files kept under their own name, written by the
//! deploy, recorded in state, retired only when the stack that recorded them
//! stops declaring them — fix-41's guarantee, extended, not loosened.
//!
//! Each test here was written before the code and failed on it first.

use std::collections::BTreeMap;

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::destroy::forget;
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;
use homelab_core::state::{HostState, StackState, StateStore};

const NOW: u64 = 1_760_000_000;
const STATE: &str = "/var/lib/homelab";
const ROUTES: &str = "/appdata/gateway/traefik-config/routes";

/// Byte for byte what `manual-kyu.yml` routes today (read from CT 104 on
/// 2026-09-27), comments left out: what matters here is that the deploy does
/// not rewrite it.
const KYU_ROUTE: &str = "http:\n  routers:\n    kyu:\n      rule: \"Host(`kyu.kp-soft.dev`)\"\n      entryPoints: [web]\n      service: kyu\n\n  services:\n    kyu:\n      loadBalancer:\n        servers:\n          - url: \"http://10.10.10.9:8080\"\n";

fn manifest(vmid: u16, stack: &str) -> StackManifest {
    StackManifest {
        homepage_widgets: Default::default(),
        home_address_whitelist: None,
        generated_dashboards_command: None,
        tiles: Default::default(),
        log_files: Vec::new(),
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        on_demand: false,
        syslog_receivers: vec![],
        natives: Vec::new(),
        firewall: None,
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
            timezone: "host".into(),
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
            host_path: format!("/appdata/{}/{}-config", stack, stack),
            mount_point: format!("/appdata/{}/{}-config", stack, stack),
            no_data: false,
            no_backup: None,
            host_owner_uid: Some(101000),
            app: Some(stack.into()),
        }],
        apps: vec![stack.into()],
    }
}

fn extra(filename: &str, content: &str) -> GatewayRoute {
    GatewayRoute {
        gateway_vmid: 104,
        filename: filename.into(),
        content: content.into(),
    }
}

/// A stack with no `gateway_route` and one route file under its own name —
/// kyu's shape after fix-91.
fn spec(vmid: u16, stack: &str, extras: Vec<GatewayRoute>) -> DeploySpec {
    DeploySpec {
        secret_files: Vec::new(),
        source: None,
        native_manifests: Default::default(),
        native_binaries: Default::default(),
        manifest: manifest(vmid, stack),
        files: vec![FileBlob {
            path: format!("{}/docker-compose.yml", stack),
            content: "services: {}\n".into(),
            mode: None,
        }],
        env: BTreeMap::new(),
        gateway_route: None,
        extra_routes: extras,
        checks: Default::default(),
    }
}

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig {
            gateway_routes_dir: ROUTES.into(),
            ..Default::default()
        },
        state_dir: STATE.into(),
        now_unix: NOW,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    }
}

fn record(m: &StackManifest) -> StackState {
    StackState {
        applied_source: None,
        vmid: m.vmid,
        hostname: m.hostname.clone(),
        apps: m.apps.clone(),
        applied_at: 1,
        last_backup: 4242,
        applied_hash: String::new(),
        manifest: Some(m.clone()),
        enabled: true,
        natives: Vec::new(),
        incomplete_step: None,
        route_file: None,
        extra_route_files: Vec::new(),
    }
}

async fn seed_state(exec: &MockExecutor, st: HostState) {
    StateStore::new(exec, STATE).save(st).await.unwrap();
}

async fn load_state(exec: &MockExecutor) -> HostState {
    StateStore::new(exec, STATE).load().await.unwrap()
}

/// The container exists, answers as `<vmid>-app-<stack>` and runs its app.
fn script_existing(exec: &MockExecutor, vmid: u16, stack: &str) {
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok(&format!(
            "hostname: {vmid}-app-{stack}\nprotection: 1\n\
             mp0: /appdata/{stack}/{stack}-config,mp=/appdata/{stack}/{stack}-config\n\
             onboot: 1\nstartup: order=50\n"
        )),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok(&format!("{}\n", stack)),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always(
        &format!("ls -A '/appdata/{stack}/{stack}-config'"),
        CmdOutput::ok("config.xml\n"),
    );
}

/// Every `rm -f` run on the gateway whose command names `needle`.
fn removed_on_gateway(exec: &MockExecutor, needle: &str) -> Vec<String> {
    exec.calls_containing(needle)
        .into_iter()
        .filter(|c| c.starts_with("pct exec 104") && c.contains("rm -f"))
        .collect()
}

/// The deploy writes a declared extra route under the name it already has
/// on the gateway, with exactly the content in the repository — so bringing
/// a hand-written file into the repository changes no byte of it.
/// covers: fix-91
#[tokio::test]
async fn a_deploy_writes_each_extra_route_under_its_own_name_byte_for_byte() {
    let exec = MockExecutor::new();
    script_existing(&exec, 109, "kyu");
    let sink = VecSink::new();
    let j = NullJournal;
    let sp = spec(109, "kyu", vec![extra("manual-kyu.yml", KYU_ROUTE)]);
    let report = deploy(&ctx(&exec, &sink, &j), &sp).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.file(&format!("{}/manual-kyu.yml", ROUTES)).as_deref(),
        Some(KYU_ROUTE),
        "written under its own name, unchanged: {:?}",
        exec.file_paths()
    );
    assert!(
        exec.file(&format!("{}/109-app-kyu.yml", ROUTES)).is_none(),
        "no second file for the same hostname"
    );
}

/// The deploy records which extra route files it wrote, so a later deploy
/// or a destroy can remove exactly those and nothing else.
/// covers: fix-91
#[tokio::test]
async fn a_deploy_records_the_extra_route_files_it_wrote() {
    let exec = MockExecutor::new();
    script_existing(&exec, 109, "kyu");
    let sink = VecSink::new();
    let j = NullJournal;
    let sp = spec(109, "kyu", vec![extra("manual-kyu.yml", KYU_ROUTE)]);
    assert!(deploy(&ctx(&exec, &sink, &j), &sp).await.ok);
    let st = load_state(&exec).await;
    assert_eq!(st.stacks["kyu"].extra_route_files, vec!["manual-kyu.yml"]);
    assert_eq!(st.stacks["kyu"].route_file, None);
}

/// An extra route this stack's deploy wrote, and that the stack no longer
/// declares, goes — the same rule fix-41 gives `gateway_route`.
/// covers: fix-91
#[tokio::test]
async fn an_extra_route_the_stack_stops_declaring_is_retired() {
    let exec = MockExecutor::new();
    script_existing(&exec, 109, "kyu");
    let mut st = HostState::default();
    let mut rec = record(&manifest(109, "kyu"));
    rec.extra_route_files = vec!["manual-kyu.yml".into(), "manual-old.yml".into()];
    st.stacks.insert("kyu".into(), rec);
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let sp = spec(109, "kyu", vec![extra("manual-kyu.yml", KYU_ROUTE)]);
    let report = deploy(&ctx(&exec, &sink, &j), &sp).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        removed_on_gateway(&exec, "manual-old.yml").len(),
        1,
        "{:?}",
        exec.calls()
    );
    assert!(
        removed_on_gateway(&exec, "manual-kyu.yml").is_empty(),
        "the one still declared stays"
    );
    assert_eq!(
        load_state(&exec).await.stacks["kyu"].extra_route_files,
        vec!["manual-kyu.yml"]
    );
}

/// fix-41's guarantee holds for the new kind too: a route file on the
/// gateway that no deploy of this stack recorded is never removed, even
/// when the stack declares no routes at all.
/// covers: fix-91
#[tokio::test]
async fn a_route_file_no_deploy_recorded_is_never_removed() {
    let exec = MockExecutor::new();
    script_existing(&exec, 104, "gateway");
    let mut st = HostState::default();
    st.stacks
        .insert("gateway".into(), record(&manifest(104, "gateway")));
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(104, "gateway", vec![])).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        removed_on_gateway(&exec, ROUTES).is_empty(),
        "nothing in the routes directory is removed: {:?}",
        exec.calls()
    );
}

/// A forgotten (or destroyed — they share `unregister`) stack takes its
/// recorded extra routes with it. Without this, allowing a name other than
/// `<vmid>-app-<stack>.yml` would bring F115 back: a router left answering
/// for a stack that is gone.
/// covers: fix-91
#[tokio::test]
async fn forget_removes_the_extra_route_files_the_stack_recorded() {
    let exec = MockExecutor::new();
    let mut st = HostState::default();
    let mut rec = record(&manifest(109, "kyu"));
    rec.extra_route_files = vec!["manual-kyu.yml".into()];
    st.stacks.insert("kyu".into(), rec);
    seed_state(&exec, st).await;
    exec.respond_always(
        "pct list",
        CmdOutput::ok("VMID       Status     Lock         Name\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = forget(&ctx(&exec, &sink, &j), "kyu").await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        removed_on_gateway(&exec, &format!("{}/manual-kyu.yml", ROUTES)).len(),
        1,
        "{:?}",
        exec.calls()
    );
}

/// The host refuses an extra route name that could escape the routes
/// directory, and one that duplicates the stack's own `gateway_route` —
/// two writes of one file in one deploy, the second silently winning.
/// covers: fix-91
#[test]
fn validation_refuses_a_bad_or_duplicate_extra_route_name() {
    let bad = spec(109, "kyu", vec![extra("../x.yml", KYU_ROUTE)]);
    let err = validate(&bad).unwrap_err().to_string();
    assert!(err.contains("../x.yml"), "{}", err);

    let mut dup = spec(109, "kyu", vec![extra("109-app-kyu.yml", KYU_ROUTE)]);
    dup.gateway_route = Some(extra("109-app-kyu.yml", KYU_ROUTE));
    let err = validate(&dup).unwrap_err().to_string();
    assert!(err.contains("109-app-kyu.yml"), "{}", err);

    let twice = spec(
        109,
        "kyu",
        vec![
            extra("manual-kyu.yml", KYU_ROUTE),
            extra("manual-kyu.yml", KYU_ROUTE),
        ],
    );
    assert!(validate(&twice).is_err());

    assert!(validate(&spec(109, "kyu", vec![extra("manual-kyu.yml", KYU_ROUTE)])).is_ok());
}
