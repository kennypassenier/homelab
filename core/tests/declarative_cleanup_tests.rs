//! step-22 / ask-8 (Kenny, 2026-09-27): everything a stack registers is
//! declarative. What the files declare is added; what leaves them is removed.
//!
//! Each test here was written before the code it describes and failed on the
//! code as it stood: the metadata half (step-22) covers what a deploy,
//! destroy or forget left behind in registries nothing else cleans, and the
//! data half (ask-8) covers what Kenny decided on the cleanup form — orphan
//! files and units removed by the deploy, a dropped native unit stopped with
//! its data kept, a dropped mount detached with its host directory kept, and
//! a vault copy kept with the date its app left.

use std::collections::BTreeMap;

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::native::NativeServiceManifest;
use homelab_core::ops::destroy::{destroy, destroy_recorded, forget};
use homelab_core::ops::fleetcheck::{evaluate, GrowthLimits, LiveFacts, Severity};
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::{PipelineEvent, VecSink};
use homelab_core::state::{
    HostState, ManualCheckRecord, RetiredKind, RetiredRecord, StackState, StateStore,
};

const NOW: u64 = 1_760_000_000;
const STATE: &str = "/var/lib/homelab";
const HOMEPAGE: &str = "/appdata/home/homepage-config/services.yaml";
const KUMA: &str = "/appdata/uptime/kuma-seeder-config/host-monitors.json";

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
        firewall: None,
        natives: Vec::new(),
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

fn spec(vmid: u16, stack: &str) -> DeploySpec {
    DeploySpec {
        secret_files: Vec::new(),
        source: None,
        native_binaries: Default::default(),
        native_manifests: Default::default(),
        manifest: manifest(vmid, stack),
        files: vec![FileBlob {
            path: format!("{}/docker-compose.yml", stack),
            content: "services: {}\n".into(),
            mode: None,
        }],
        env: BTreeMap::new(),
        extra_routes: Vec::new(),
        gateway_route: Some(GatewayRoute {
            gateway_vmid: 104,
            filename: format!("{}-app-{}.yml", vmid, stack),
            content: "http: {}\n".into(),
        }),
        checks: Default::default(),
    }
}

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
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

fn check(stack: &str, app: &str, text: &str) -> (String, ManualCheckRecord) {
    (
        homelab_core::ops::manualchecks::id_for(stack, app, text),
        ManualCheckRecord {
            stack: stack.into(),
            app: app.into(),
            text: text.into(),
            registered_at: 1,
            answered_at: None,
            ok: None,
            note: String::new(),
            answered_hash: None,
            accepted_until: None,
            once: false,
            url: None,
        },
    )
}

async fn seed_state(exec: &MockExecutor, st: HostState) {
    StateStore::new(exec, STATE).save(st).await.unwrap();
}

async fn load_state(exec: &MockExecutor) -> HostState {
    StateStore::new(exec, STATE).load().await.unwrap()
}

fn lines(sink: &VecSink) -> Vec<String> {
    sink.events()
        .into_iter()
        .filter_map(|e| match e {
            PipelineEvent::Line { msg, .. } => Some(msg),
            _ => None,
        })
        .collect()
}

/// The container exists, answers as `<vmid>-app-<stack>` and runs its app.
fn script_existing(exec: &MockExecutor, vmid: u16, stack: &str, extra_config: &str) {
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok(&format!(
            "hostname: {vmid}-app-{stack}\nprotection: 1\n\
             mp0: /appdata/{stack}/{stack}-config,mp=/appdata/{stack}/{stack}-config\n\
             onboot: 1\nstartup: order=50\n{extra_config}"
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

/// The container does not exist yet.
fn script_fresh(exec: &MockExecutor, stack: &str) {
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct status", CmdOutput::ok("status: stopped"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok(&format!("{}\n", stack)),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
}

fn pos(calls: &[String], needle: &str) -> Option<usize> {
    calls.iter().position(|c| c.contains(needle))
}

// ── 1 · forget unregisters what destroy unregisters ─────────────────────────

/// `homelab forget` dropped the state record and nothing else, so a stack
/// forgotten after its container was lost kept its route, its scrape target,
/// its dashboard, its manual checks, its tile and its host monitor.
/// covers: step-22
#[tokio::test]
async fn forget_unregisters_everything_a_destroy_unregisters() {
    let exec = MockExecutor::new();
    let mut st = HostState::default();
    st.stacks
        .insert("drill".into(), record(&manifest(119, "drill")));
    st.stacks
        .insert("syncthing".into(), record(&manifest(110, "syncthing")));
    st.manual_checks.extend([
        check("drill", "drill", "q1"),
        check("syncthing", "syncthing", "q2"),
    ]);
    seed_state(&exec, st).await;
    // Only syncthing still has a container.
    exec.respond_always(
        "pct list",
        CmdOutput::ok("VMID       Status     Lock         Name\n110        running                 110-app-syncthing\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.metrics_targets_dir = Some("/appdata/metrics/prometheus-config/targets".into());
    c.grafana_dashboards_dir =
        Some("/opt/gateway/grafana/provisioning/dashboards-generated".into());
    c.homepage_services_file = Some(HOMEPAGE.into());
    c.kuma_monitors_file = Some(KUMA.into());

    let report = forget(&c, "drill").await;
    assert!(report.ok, "{:?}", report.error);

    let on_gateway = |needle: &str| {
        exec.calls_containing(needle)
            .into_iter()
            .any(|c| c.starts_with("pct exec 104") && c.contains("rm -f"))
    };
    assert!(
        on_gateway("119-app-drill.yml"),
        "the route goes: {:?}",
        exec.calls()
    );
    assert!(on_gateway("homelab-drill.json"), "the dashboard goes");
    assert!(
        !exec
            .calls_containing("rm -f /appdata/metrics/prometheus-config/targets/drill.json")
            .is_empty(),
        "the scrape target goes"
    );
    let after = load_state(&exec).await;
    assert!(!after.stacks.contains_key("drill"));
    assert!(after.stacks.contains_key("syncthing"));
    assert!(
        after.manual_checks.values().all(|r| r.stack != "drill"),
        "the stack's manual checks go with it"
    );
    assert_eq!(after.manual_checks.len(), 1, "another stack's checks stay");
    let kuma = exec.file(KUMA).expect("host monitors regenerated");
    assert!(kuma.contains("host · syncthing") && !kuma.contains("host · drill"));
    assert!(
        exec.file(HOMEPAGE).is_some(),
        "the front page is regenerated"
    );
}

/// The one guard forget always had stays: a record whose container still
/// answers is live, and nothing about it is unregistered.
/// covers: step-22
#[tokio::test]
async fn forget_still_refuses_a_record_whose_container_is_live() {
    let exec = MockExecutor::new();
    let mut st = HostState::default();
    st.stacks
        .insert("drill".into(), record(&manifest(119, "drill")));
    seed_state(&exec, st).await;
    exec.respond_always(
        "pct list",
        CmdOutput::ok("VMID Status Lock Name\n119 running 119-app-drill\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = forget(&ctx(&exec, &sink, &j), "drill").await;
    assert!(!report.ok);
    assert!(exec.calls_containing("rm -f").is_empty());
    assert!(load_state(&exec).await.stacks.contains_key("drill"));
}

// ── 2 · destroy drops manual checks, regenerates homepage + monitors ────────

/// covers: step-22
#[tokio::test]
async fn destroy_drops_the_manual_checks_and_regenerates_the_fleet_files() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 119-app-drill\nprotection: 1\n"),
    );
    let mut st = HostState::default();
    st.stacks
        .insert("drill".into(), record(&manifest(119, "drill")));
    st.stacks
        .insert("syncthing".into(), record(&manifest(110, "syncthing")));
    st.manual_checks.extend([
        check("drill", "drill", "q1"),
        check("syncthing", "syncthing", "q2"),
    ]);
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.homepage_services_file = Some(HOMEPAGE.into());
    c.kuma_monitors_file = Some(KUMA.into());

    let report = destroy(&c, &manifest(119, "drill"), "drill", true).await;
    assert!(report.ok, "{:?}", report.error);
    let after = load_state(&exec).await;
    assert!(after.manual_checks.values().all(|r| r.stack != "drill"));
    assert_eq!(after.manual_checks.len(), 1);
    let kuma = exec
        .file(KUMA)
        .expect("host monitors regenerated after destroy");
    assert!(kuma.contains("host · syncthing") && !kuma.contains("host · drill"));
    assert!(
        exec.file(HOMEPAGE).is_some(),
        "the front page is regenerated"
    );
    // After the route is gone, so the front page no longer lists it.
    let calls = exec.calls();
    let route = pos(&calls, "119-app-drill.yml").unwrap();
    let page = pos(&calls, &format!("write_file {}", HOMEPAGE)).unwrap();
    assert!(route < page);
}

// ── 3 · a dropped gateway_route or a changed vmid removes the old route ─────

/// covers: step-22
#[tokio::test]
async fn a_stack_that_drops_its_gateway_route_loses_the_route_file() {
    let exec = MockExecutor::new();
    script_existing(&exec, 110, "syncthing", "");
    let mut st = HostState::default();
    let mut rec = record(&manifest(110, "syncthing"));
    rec.route_file = Some("110-app-syncthing.yml".into());
    st.stacks.insert("syncthing".into(), rec);
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.gateway_route = None;
    let report = deploy(&ctx(&exec, &sink, &j), &sp).await;
    assert!(report.ok, "{:?}", report.error);
    let removed: Vec<String> = exec
        .calls_containing("110-app-syncthing.yml")
        .into_iter()
        .filter(|c| c.starts_with("pct exec 104") && c.contains("rm -f"))
        .collect();
    assert_eq!(removed.len(), 1, "{:?}", exec.calls());
}

/// covers: step-22
#[tokio::test]
async fn a_stack_that_moves_to_another_vmid_loses_the_old_route_file() {
    let exec = MockExecutor::new();
    script_fresh(&exec, "syncthing");
    let mut st = HostState::default();
    let mut rec = record(&manifest(110, "syncthing"));
    rec.route_file = Some("110-app-syncthing.yml".into());
    st.stacks.insert("syncthing".into(), rec);
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(111, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        exec.calls_containing("110-app-syncthing.yml")
            .iter()
            .any(|c| c.starts_with("pct exec 104") && c.contains("rm -f")),
        "the route under the old vmid goes: {:?}",
        exec.calls()
    );
    assert!(
        !exec
            .calls_containing("111-app-syncthing.yml")
            .iter()
            .any(|c| c.contains("rm -f")),
        "the new one stays"
    );
}

/// fix-41: the route retirement removed `<vmid>-app-<stack>.yml` for any
/// stack without a `gateway_route`, including a route written by hand on the
/// gateway: almanac's `112-app-almanac.yml` would have gone on its next
/// deploy, and almanac.kp-soft.dev with it (found by the network reviewer,
/// 2026-09-27). Only a route file a deploy of this stack wrote, recorded in
/// state, is retired.
/// covers: fix-41
#[tokio::test]
async fn a_hand_written_route_file_is_never_retired() {
    let exec = MockExecutor::new();
    script_existing(&exec, 112, "almanac", "");
    let mut st = HostState::default();
    st.stacks
        .insert("almanac".into(), record(&manifest(112, "almanac")));
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let mut sp = spec(112, "almanac");
    sp.gateway_route = None;
    let report = deploy(&ctx(&exec, &sink, &j), &sp).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        !exec
            .calls_containing("112-app-almanac.yml")
            .iter()
            .any(|c| c.contains("rm -f")),
        "a route no deploy of this stack wrote must stay: {:?}",
        exec.calls()
    );
}

/// fix-41: a deploy records the route file it wrote, so a later drop can
/// retire exactly that file.
/// covers: fix-41
#[tokio::test]
async fn a_deploy_records_the_route_file_it_wrote() {
    let exec = MockExecutor::new();
    script_fresh(&exec, "syncthing");
    let sink = VecSink::new();
    let j = NullJournal;
    assert!(
        deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing"))
            .await
            .ok
    );
    let st = load_state(&exec).await;
    assert_eq!(
        st.stacks["syncthing"].route_file.as_deref(),
        Some("110-app-syncthing.yml")
    );
}

// ── 4 · the intent repo copy loses files the stack dropped ──────────────────

/// covers: step-22
#[tokio::test]
async fn the_intent_repo_copy_loses_files_the_stack_no_longer_has() {
    let exec = MockExecutor::new();
    script_existing(&exec, 110, "syncthing", "");
    exec.respond_always(
        "/var/lib/homelab/repo/stacks/syncthing' 2>/dev/null && find",
        CmdOutput::ok("syncthing/docker-compose.yml\nsyncthing/old.conf\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let rm = pos(
        &calls,
        "rm -f /var/lib/homelab/repo/stacks/syncthing/syncthing/old.conf",
    )
    .expect("the dropped file leaves the intent repo");
    let add = pos(&calls, "git -C /var/lib/homelab/repo add -A").unwrap();
    assert!(rm < add, "removed before the commit records it");
    assert!(
        exec.calls_containing(
            "rm -f /var/lib/homelab/repo/stacks/syncthing/syncthing/docker-compose.yml"
        )
        .is_empty(),
        "a file the stack still has is not touched"
    );
}

// ── 5 · the fleet check reports a stack in state with no stack file ─────────

/// covers: step-22, ask-8
#[test]
fn the_fleet_check_reports_a_stack_in_state_without_a_stack_file() {
    let mut st = HostState::default();
    st.stacks
        .insert("drill".into(), record(&manifest(119, "drill")));
    st.stacks
        .insert("syncthing".into(), record(&manifest(110, "syncthing")));
    let live = LiveFacts {
        containers: vec![
            (110, "110-app-syncthing".into()),
            (119, "119-app-drill".into()),
        ],
        stack_files: vec![("stacks/syncthing".into(), 110)],
        ..Default::default()
    };
    let findings = evaluate(
        &st,
        &live,
        NOW,
        u64::MAX,
        GrowthLimits::default(),
        None,
        u64::MAX,
        u64::MAX,
    );
    let hit: Vec<_> = findings
        .iter()
        .filter(|f| f.subject == "drill" && f.what.contains("no stack file"))
        .collect();
    assert_eq!(hit.len(), 1, "{:?}", findings);
    assert_eq!(hit[0].severity, Severity::Drift);
    assert!(hit[0].remedy.contains("homelab apply"));
    assert!(
        !findings
            .iter()
            .any(|f| f.subject == "syncthing" && f.what.contains("no stack file")),
        "a stack with its file is not reported"
    );

    // No stack files sent = the client could not look; nothing is claimed.
    let blind = LiveFacts {
        stack_files: Vec::new(),
        ..live
    };
    assert!(!evaluate(
        &st,
        &blind,
        NOW,
        u64::MAX,
        GrowthLimits::default(),
        None,
        u64::MAX,
        u64::MAX
    )
    .iter()
    .any(|f| f.what.contains("no stack file")));
}

// ── 6 · destroy works from the manifest recorded in state ───────────────────

/// `homelab apply` destroys a stack whose directory is gone, so destroy has
/// to work without the directory: from `StackState.manifest`, behind the
/// same typed name, no-touch list and hostname guard.
/// covers: ask-8
#[tokio::test]
async fn destroy_works_from_the_manifest_recorded_in_state() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 119-app-drill\nprotection: 1\n"),
    );
    let mut st = HostState::default();
    st.stacks
        .insert("drill".into(), record(&manifest(119, "drill")));
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy_recorded(&ctx(&exec, &sink, &j), "drill", "drill", true).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(exec.calls_containing("pct destroy 119 --purge").len(), 1);
    assert!(!load_state(&exec).await.stacks.contains_key("drill"));
}

/// covers: ask-8
#[tokio::test]
async fn destroy_from_state_keeps_every_safety_gate() {
    // A wrong typed name runs nothing.
    let exec = MockExecutor::new();
    let mut st = HostState::default();
    st.stacks
        .insert("drill".into(), record(&manifest(119, "drill")));
    let mut hand = record(&manifest(119, "evil"));
    hand.vmid = 101;
    hand.manifest.as_mut().unwrap().vmid = 101;
    hand.manifest.as_mut().unwrap().hostname = "101-app-evil".into();
    st.stacks.insert("evil".into(), hand);
    let mut adopted = record(&manifest(118, "inbox"));
    adopted.manifest = None;
    st.stacks.insert("inbox".into(), adopted);
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let c = ctx(&exec, &sink, &j);
    assert!(!destroy_recorded(&c, "drill", "dril", true).await.ok);
    // The no-touch list.
    let r = destroy_recorded(&c, "evil", "evil", true).await;
    assert!(!r.ok && r.error.unwrap().why.contains("no-touch"));
    // The hostname guard.
    exec.respond_always("pct config 119", CmdOutput::ok("hostname: 119-app-other\n"));
    assert!(!destroy_recorded(&c, "drill", "drill", true).await.ok);
    // A record without a manifest cannot be destroyed from state.
    let r = destroy_recorded(&c, "inbox", "inbox", true).await;
    assert!(!r.ok && r.error.unwrap().why.contains("no manifest"));
    // Unknown stack.
    assert!(!destroy_recorded(&c, "ghost", "ghost", true).await.ok);
    assert!(exec.ran("pct", &["destroy"]) == 0);
    assert_eq!(load_state(&exec).await.stacks.len(), 3);
}

// ── 7 · orphan files and dropped rootfs files are removed by the deploy ─────

/// D84 reported orphans and left them for `homelab prune-orphans`; ask-8
/// (`Automatisch bij deploy`) has the deploy remove them, one line each.
/// covers: ask-8
#[tokio::test]
async fn the_deploy_removes_files_the_stack_no_longer_declares() {
    let exec = MockExecutor::new();
    script_existing(&exec, 110, "syncthing", "");
    exec.respond_always(
        "cd '/opt/syncthing' 2>/dev/null && find",
        CmdOutput::ok("syncthing/docker-compose.yml\nsyncthing/stale.conf\nsyncthing/.env\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.calls_containing("pct exec 110 -- rm -f /opt/syncthing/syncthing/stale.conf")
            .len(),
        1
    );
    assert!(exec
        .calls_containing("rm -f /opt/syncthing/syncthing/.env")
        .is_empty());
    assert!(exec
        .calls_containing("rm -f /opt/syncthing/syncthing/docker-compose.yml")
        .is_empty());
    let said: Vec<String> = lines(&sink)
        .into_iter()
        .filter(|l| l.contains("removed") && l.contains("/opt/syncthing/syncthing/stale.conf"))
        .collect();
    assert_eq!(said.len(), 1, "one transcript line per removed file");
}

/// Files the orchestrator writes into a container on behalf of OTHER stacks
/// (generated dashboards on the gateway) are not the gateway's orphans.
/// covers: ask-8
#[tokio::test]
async fn generated_dashboards_on_the_gateway_are_never_orphans() {
    let exec = MockExecutor::new();
    script_existing(&exec, 104, "gateway", "");
    exec.respond_always(
        "cd '/opt/gateway' 2>/dev/null && find",
        CmdOutput::ok(
            "gateway/docker-compose.yml\n\
             grafana/provisioning/dashboards-generated/homelab-media.json\n",
        ),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.grafana_dashboards_dir =
        Some("/opt/gateway/grafana/provisioning/dashboards-generated".into());
    let mut sp = spec(104, "gateway");
    sp.gateway_route = None;
    let report = deploy(&c, &sp).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        exec.calls_containing(
            "rm -f /opt/gateway/grafana/provisioning/dashboards-generated/homelab-media.json"
        )
        .is_empty(),
        "another stack's generated dashboard is not an orphan"
    );
}

/// A `rootfs/` file the stack used to place (known from the intent repo's
/// previous copy) is removed from the container; a unit or timer is
/// disabled and stopped first, and systemd is reloaded afterwards.
/// covers: ask-8
#[tokio::test]
async fn a_dropped_rootfs_unit_is_disabled_then_removed_then_reloaded() {
    let exec = MockExecutor::new();
    script_existing(&exec, 110, "syncthing", "");
    exec.respond_always(
        "/var/lib/homelab/repo/stacks/syncthing' 2>/dev/null && find",
        CmdOutput::ok(
            "syncthing/docker-compose.yml\n\
             rootfs/etc/systemd/system/old-backup.timer\n\
             rootfs/usr/local/bin/old-script\n",
        ),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let disable = calls
        .iter()
        .position(|c| c.contains("systemctl disable --now") && c.contains("old-backup.timer"))
        .expect("the timer is disabled and stopped");
    let rm = pos(
        &calls,
        "pct exec 110 -- rm -f /etc/systemd/system/old-backup.timer",
    )
    .expect("then removed");
    let reload = calls
        .iter()
        .rposition(|c| c.contains("systemctl daemon-reload"))
        .expect("then systemd reloads");
    assert!(disable < rm && rm < reload, "{} {} {}", disable, rm, reload);
    assert!(pos(&calls, "pct exec 110 -- rm -f /usr/local/bin/old-script").is_some());
    assert!(
        !calls
            .iter()
            .any(|c| c.contains("systemctl disable") && c.contains("old-script")),
        "a script is not a unit"
    );
    let said = lines(&sink);
    assert!(said
        .iter()
        .any(|l| l.contains("removed") && l.contains("/etc/systemd/system/old-backup.timer")));
    assert!(said
        .iter()
        .any(|l| l.contains("removed") && l.contains("/usr/local/bin/old-script")));
}

// ── 8 · a native unit dropped from natives: ─────────────────────────────────

fn native(stack: &str, vmid: u16, unit: &str) -> NativeServiceManifest {
    serde_json::from_value(serde_json::json!({
        "stack_name": stack, "vmid": vmid, "hostname": format!("{}-app-{}", vmid, stack),
        "unit": unit, "binary": format!("/usr/local/bin/{}", unit),
        "env_file": null, "data_dirs": [format!("/var/lib/{}", unit)],
        "update_cmd": null, "stateless": false
    }))
    .unwrap()
}

/// `Stoppen, data bewaren`: stopped, disabled, unit file and program
/// removed, out of state; data dirs and the restic repository stay.
/// covers: ask-8
#[tokio::test]
async fn a_native_unit_dropped_from_the_stack_is_stopped_and_unregistered_with_its_data_kept() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\nprotection: 1\nonboot: 1\nstartup: order=50\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always("systemctl is-active kyu", CmdOutput::ok("active\n"));
    let mut m = manifest(109, "kyu");
    m.apps = vec![];
    m.storage = vec![];
    m.native_only = true;
    m.natives = vec!["kyu".into(), "oldsvc".into()];
    let mut rec = record(&m);
    rec.natives = vec![native("kyu", 109, "kyu"), native("kyu", 109, "oldsvc")];
    let mut st = HostState::default();
    st.stacks.insert("kyu".into(), rec);
    seed_state(&exec, st).await;

    let mut sp = spec(109, "kyu");
    sp.manifest = m.clone();
    sp.manifest.natives = vec!["kyu".into()];
    sp.gateway_route = None;
    sp.files = vec![FileBlob {
        path: "kyu/kyu.service".into(),
        content: "[Unit]\nDescription=kyu\n".into(),
        mode: None,
    }];
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &sp).await;
    assert!(report.ok, "{:?}", report.error);

    let calls = exec.calls();
    let stop = calls
        .iter()
        .position(|c| c.contains("systemctl disable --now") && c.contains("oldsvc"))
        .expect("stopped and disabled");
    let unit = pos(&calls, "rm -f /etc/systemd/system/oldsvc.service").expect("unit file removed");
    let prog = pos(&calls, "rm -f /usr/local/bin/oldsvc").expect("program removed");
    assert!(stop < unit && stop < prog);
    assert!(calls
        .iter()
        .skip(unit)
        .any(|c| c.contains("systemctl daemon-reload")));
    assert!(
        exec.calls_containing("/var/lib/oldsvc").is_empty(),
        "data kept"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.contains("restic") && c.contains("oldsvc")),
        "its restic repository is not touched"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.contains("systemctl disable") && c.contains(" kyu")),
        "the unit that stays is left alone"
    );
    let after = load_state(&exec).await;
    let rec = after.stacks.get("kyu").unwrap();
    assert_eq!(
        rec.natives
            .iter()
            .map(|n| n.unit.as_str())
            .collect::<Vec<_>>(),
        vec!["kyu"]
    );
    let gone = after
        .retired
        .get("kyu/oldsvc")
        .expect("recorded as retired");
    assert_eq!(gone.kind, RetiredKind::Unit);
    assert_eq!(gone.retired_at, NOW);
    assert_eq!(
        gone.repos,
        vec!["oldsvc-config".to_string()],
        "its backups are kept"
    );
}

/// A unit registered by `homelab adopt` that the stack file never declared
/// is not the deploy's to retire.
/// covers: ask-8
#[tokio::test]
async fn an_adopted_unit_the_stack_never_declared_is_not_retired() {
    let exec = MockExecutor::new();
    script_existing(&exec, 110, "syncthing", "");
    let mut rec = record(&manifest(110, "syncthing"));
    rec.natives = vec![native("syncthing", 110, "kyu")];
    let mut st = HostState::default();
    st.stacks.insert("syncthing".into(), rec);
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec.calls_containing("systemctl disable").is_empty());
    assert_eq!(load_state(&exec).await.stacks["syncthing"].natives.len(), 1);
}

// ── 9 · a mount the stack no longer declares is detached ────────────────────

/// `Loskoppelen`: `pct set --delete mpN` under lifted protection; the host
/// directory stays. A mount this stack never declared is left alone.
/// covers: ask-8
#[tokio::test]
async fn a_mount_the_stack_no_longer_declares_is_detached_and_its_directory_kept() {
    let exec = MockExecutor::new();
    script_existing(
        &exec,
        110,
        "syncthing",
        "mp1: /HDD18TB/old,mp=/mnt/old\nmp2: /hand/made,mp=/hand\n",
    );
    let mut before = manifest(110, "syncthing");
    before.data_mounts = vec![DataMount {
        host_path: "/HDD18TB/old".into(),
        mount_point: "/mnt/old".into(),
        note: None,
        rotate: None,
    }];
    let mut st = HostState::default();
    st.stacks.insert("syncthing".into(), record(&before));
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.lxc.protection = true;
    let report = deploy(&ctx(&exec, &sink, &j), &sp).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let off = pos(&calls, "--protection 0").expect("protection lifted");
    let del = pos(&calls, "pct set 110 --delete mp1").expect("mp1 detached");
    let on = calls
        .iter()
        .rposition(|c| c.contains("--protection 1"))
        .expect("protection restored");
    assert!(off < del && del < on);
    assert!(
        exec.calls_containing("--delete mp2").is_empty(),
        "never declared, never ours"
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.contains("rm") && c.contains("/HDD18TB/old")),
        "the host directory is kept"
    );
}

// ── 10 · a vault copy is kept, and the date its app left is recorded ────────

/// `Samen met de back-up`: the deletion policy is not decided, so the deploy
/// deletes nothing from the vault and records when the app left.
/// covers: ask-8
#[tokio::test]
async fn a_removed_app_keeps_its_vault_copy_and_records_when_it_left() {
    let exec = MockExecutor::new();
    script_existing(&exec, 110, "syncthing", "");
    let mut before = manifest(110, "syncthing");
    before.apps = vec!["syncthing".into(), "oldapp".into()];
    before.storage.push(MountSpec {
        host_path: "/appdata/syncthing/oldapp-config".into(),
        mount_point: "/appdata/syncthing/oldapp-config".into(),
        no_data: false,
        no_backup: None,
        host_owner_uid: Some(101000),
        app: Some("oldapp".into()),
    });
    let mut st = HostState::default();
    st.stacks.insert("syncthing".into(), record(&before));
    // An app that left earlier and is back in the files now.
    st.retired.insert(
        "syncthing/syncthing".into(),
        RetiredRecord {
            kind: RetiredKind::App,
            stack: "syncthing".into(),
            name: "syncthing".into(),
            vmid: 110,
            retired_at: 5,
            repos: vec![],
            appdata: vec![],
            vault: vec![],
        },
    );
    seed_state(&exec, st).await;
    exec.seed_file("/var/lib/homelab/secrets/syncthing/oldapp.env", "TOKEN=x\n");
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec
        .calls_containing("secrets/syncthing/oldapp.env")
        .is_empty());
    assert!(exec
        .file("/var/lib/homelab/secrets/syncthing/oldapp.env")
        .is_some());
    assert!(
        exec.calls_containing("oldapp-config")
            .iter()
            .all(|c| !c.contains("rm")),
        "its /appdata directory is kept"
    );
    let after = load_state(&exec).await;
    let gone = after
        .retired
        .get("syncthing/oldapp")
        .expect("recorded as retired");
    assert_eq!(gone.kind, RetiredKind::App);
    assert_eq!(gone.retired_at, NOW);
    assert_eq!(gone.repos, vec!["oldapp-config".to_string()]);
    assert_eq!(
        gone.appdata,
        vec!["/appdata/syncthing/oldapp-config".to_string()]
    );
    assert_eq!(
        gone.vault,
        vec!["/var/lib/homelab/secrets/syncthing/oldapp.env".to_string()]
    );
    assert!(
        !after.retired.contains_key("syncthing/syncthing"),
        "an app that is back is no longer retired"
    );

    // A second deploy keeps the first date rather than moving it.
    let sink = VecSink::new();
    let mut c = ctx(&exec, &sink, &j);
    c.now_unix = NOW + 100;
    assert!(deploy(&c, &spec(110, "syncthing")).await.ok);
    assert_eq!(
        load_state(&exec).await.retired["syncthing/oldapp"].retired_at,
        NOW
    );
}

// ── 11 · a destroyed or forgotten stack is recorded as retired (ask-9) ──────

/// Kenny, 2026-09-27 (ask-9): backups, /appdata and vault copies of a
/// destroyed stack are kept forever by default — so the destroy records
/// exactly what it left behind, and an app that had left the stack earlier
/// is folded into the stack's record.
/// covers: ask-9
#[tokio::test]
async fn a_destroyed_stack_is_recorded_with_everything_it_left_behind() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 119-app-drill\nprotection: 1\n"),
    );
    let mut st = HostState::default();
    st.stacks
        .insert("drill".into(), record(&manifest(119, "drill")));
    st.retired.insert(
        "drill/oldapp".into(),
        RetiredRecord {
            kind: RetiredKind::App,
            stack: "drill".into(),
            name: "oldapp".into(),
            vmid: 119,
            retired_at: 5,
            repos: vec!["oldapp-config".into()],
            appdata: vec!["/appdata/drill/oldapp-config".into()],
            vault: vec!["/var/lib/homelab/secrets/drill/oldapp.env".into()],
        },
    );
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(
        &ctx(&exec, &sink, &j),
        &manifest(119, "drill"),
        "drill",
        true,
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let after = load_state(&exec).await;
    let rec = after
        .retired
        .get("drill")
        .expect("the stack is recorded as retired");
    assert_eq!(rec.kind, RetiredKind::Stack);
    assert_eq!(rec.vmid, 119);
    assert_eq!(rec.retired_at, NOW);
    assert_eq!(
        rec.repos,
        vec!["drill-config".to_string(), "oldapp-config".to_string()]
    );
    assert_eq!(
        rec.appdata,
        vec![
            "/appdata/drill/drill-config".to_string(),
            "/appdata/drill/oldapp-config".to_string()
        ]
    );
    assert_eq!(
        rec.vault,
        vec!["/var/lib/homelab/secrets/drill".to_string()]
    );
    assert!(
        !after.retired.contains_key("drill/oldapp"),
        "folded into the stack's own record"
    );
}

/// covers: ask-9
#[tokio::test]
async fn a_forgotten_stack_is_recorded_as_retired_too() {
    let exec = MockExecutor::new();
    let mut st = HostState::default();
    st.stacks
        .insert("drill".into(), record(&manifest(119, "drill")));
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    assert!(forget(&ctx(&exec, &sink, &j), "drill").await.ok);
    let after = load_state(&exec).await;
    assert_eq!(
        after.retired["drill"].repos,
        vec!["drill-config".to_string()]
    );
}

/// A stack deployed again is no longer retired.
/// covers: ask-9
#[tokio::test]
async fn a_redeployed_stack_is_no_longer_retired() {
    let exec = MockExecutor::new();
    script_fresh(&exec, "syncthing");
    let mut st = HostState::default();
    st.retired.insert(
        "syncthing".into(),
        RetiredRecord {
            kind: RetiredKind::Stack,
            stack: "syncthing".into(),
            name: "syncthing".into(),
            vmid: 110,
            retired_at: 5,
            repos: vec!["syncthing-config".into()],
            appdata: vec![],
            vault: vec![],
        },
    );
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    assert!(
        deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing"))
            .await
            .ok
    );
    assert!(load_state(&exec).await.retired.is_empty());
}

// ── 12 · every retired entry is named by the fleet check (Noted) ────────────

/// covers: ask-9
#[test]
fn the_fleet_check_names_what_is_kept_for_every_retired_entry() {
    let mut st = HostState::default();
    st.retired.insert(
        "drill".into(),
        RetiredRecord {
            kind: RetiredKind::Stack,
            stack: "drill".into(),
            name: "drill".into(),
            vmid: 119,
            retired_at: NOW,
            repos: vec!["drill-config".into()],
            appdata: vec!["/appdata/drill/drill-config".into()],
            vault: vec!["/var/lib/homelab/secrets/drill".into()],
        },
    );
    let findings = evaluate(
        &st,
        &LiveFacts::default(),
        NOW,
        u64::MAX,
        GrowthLimits::default(),
        None,
        u64::MAX,
        u64::MAX,
    );
    let hit: Vec<_> = findings.iter().filter(|f| f.subject == "drill").collect();
    assert_eq!(hit.len(), 1, "{:?}", findings);
    let f = hit[0];
    assert_eq!(f.severity, Severity::Noted);
    for kept in [
        "drill-config",
        "/appdata/drill/drill-config",
        "/var/lib/homelab/secrets/drill",
        &homelab_core::state::ymd(NOW),
    ] {
        assert!(f.what.contains(kept), "{} names {}", f.what, kept);
    }
    assert!(f.remedy.contains("homelab wipe drill"));
    assert!(homelab_core::ops::fleetcheck::check_passes(
        std::slice::from_ref(f)
    ));
}

// ── 13 · `homelab wipe` deletes exactly what a retired entry kept ────────────

fn retired_drill() -> RetiredRecord {
    RetiredRecord {
        kind: RetiredKind::Stack,
        stack: "drill".into(),
        name: "drill".into(),
        vmid: 119,
        retired_at: 5,
        repos: vec!["drill-config".into(), "shared-config".into()],
        appdata: vec![
            "/appdata/drill/drill-config".into(),
            "/appdata/other/shared-config".into(),
        ],
        vault: vec!["/var/lib/homelab/secrets/drill".into()],
    }
}

fn state_with_retired_drill() -> HostState {
    let mut st = HostState::default();
    st.retired.insert("drill".into(), retired_drill());
    // A live stack that took over one of drill's apps (D25): its repository
    // and its directory are in use and must survive the wipe.
    let mut other = manifest(120, "other");
    other.storage[0].host_path = "/appdata/other/shared-config".into();
    other.storage[0].mount_point = "/appdata/other/shared-config".into();
    other.storage[0].app = Some("shared".into());
    st.stacks.insert("other".into(), record(&other));
    st
}

/// covers: ask-9
#[test]
fn the_wipe_plan_lists_exactly_what_goes_and_what_is_still_in_use() {
    let st = state_with_retired_drill();
    let plan = homelab_core::ops::retired::wipe_plan(&st, "drill", STATE).unwrap();
    assert_eq!(plan.repos, vec!["drill-config".to_string()]);
    assert_eq!(
        plan.appdata,
        vec!["/appdata/drill/drill-config".to_string()]
    );
    assert_eq!(
        plan.vault,
        vec!["/var/lib/homelab/secrets/drill".to_string()]
    );
    assert_eq!(
        plan.in_use,
        vec![
            "shared-config".to_string(),
            "/appdata/other/shared-config".to_string()
        ]
    );
    // Only retired entries can be wiped: a live stack never.
    assert!(homelab_core::ops::retired::wipe_plan(&st, "other", STATE).is_err());
    assert!(homelab_core::ops::retired::wipe_plan(&st, "ghost", STATE).is_err());
}

/// covers: ask-9
#[tokio::test]
async fn wipe_deletes_the_repositories_directories_and_vault_then_the_record() {
    let exec = MockExecutor::new();
    seed_state(&exec, state_with_retired_drill()).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let report = homelab_core::ops::retired::wipe(&ctx(&exec, &sink, &j), "drill", "drill").await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.calls_containing("rclone purge gdrive:homelab-backups/drill-config")
            .len(),
        1
    );
    assert!(
        exec.calls_containing("shared-config").is_empty(),
        "in use: untouched"
    );
    assert_eq!(
        exec.calls_containing("rm -rf -- /appdata/drill/drill-config")
            .len(),
        1
    );
    assert_eq!(
        exec.calls_containing("rm -rf -- /var/lib/homelab/secrets/drill")
            .len(),
        1
    );
    let after = load_state(&exec).await;
    assert!(!after.retired.contains_key("drill"));
    assert!(after.stacks.contains_key("other"));
    let said = lines(&sink);
    assert!(said
        .iter()
        .any(|l| l.contains("drill-config") && l.contains("deleted")));
}

/// covers: ask-9
#[tokio::test]
async fn wipe_refuses_without_the_typed_name_or_for_a_live_stack() {
    let exec = MockExecutor::new();
    seed_state(&exec, state_with_retired_drill()).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let c = ctx(&exec, &sink, &j);
    assert!(
        !homelab_core::ops::retired::wipe(&c, "drill", "dril")
            .await
            .ok
    );
    assert!(
        !homelab_core::ops::retired::wipe(&c, "other", "other")
            .await
            .ok
    );
    let calls = exec.calls();
    assert!(
        calls
            .iter()
            .all(|c| !c.contains("rclone") && !c.contains("rm -rf")),
        "{:?}",
        calls
    );
    assert!(load_state(&exec).await.retired.contains_key("drill"));
}

/// An app that left the stack is the garbage collector's: its compose file
/// must still be there when `docker compose down` runs, or its containers
/// keep running with nothing left to stop them by.
/// covers: ask-8
#[tokio::test]
async fn a_removed_apps_files_are_left_for_the_garbage_collector() {
    let exec = MockExecutor::new();
    script_existing(&exec, 110, "syncthing", "");
    exec.respond_always(
        "cd '/opt/syncthing' 2>/dev/null && find",
        CmdOutput::ok("syncthing/docker-compose.yml\noldapp/docker-compose.yml\n"),
    );
    let mut before = manifest(110, "syncthing");
    before.apps = vec!["syncthing".into(), "oldapp".into()];
    let mut st = HostState::default();
    st.stacks.insert("syncthing".into(), record(&before));
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec
        .calls_containing("rm -f /opt/syncthing/oldapp/docker-compose.yml")
        .is_empty());
    assert_eq!(
        exec.calls_containing("cd '/opt/syncthing/oldapp' && docker compose down")
            .len(),
        1
    );
}

// ── 14 · a stack that drops its last manual question loses it ──────────────

/// Found by the documentation pass the same day: the deploy only
/// re-registered manual questions when the stack still had at least one, so
/// a stack that dropped its LAST `manual:` line kept the old question in
/// `homelab checks` until it was destroyed.
/// covers: step-22
#[tokio::test]
async fn a_stack_that_drops_its_last_manual_question_loses_it() {
    let exec = MockExecutor::new();
    script_fresh(&exec, "syncthing");
    let mut st = HostState::default();
    st.manual_checks.extend([
        check("syncthing", "syncthing", "Kijk of de sync loopt."),
        check("media", "jellyfin", "Speel een film af."),
    ]);
    seed_state(&exec, st).await;
    let sink = VecSink::new();
    let j = NullJournal;
    // spec() declares no checks at all.
    assert!(
        deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing"))
            .await
            .ok
    );
    let after = load_state(&exec).await;
    assert!(
        after.manual_checks.values().all(|r| r.stack != "syncthing"),
        "{:?}",
        after.manual_checks
    );
    assert_eq!(
        after.manual_checks.len(),
        1,
        "another stack's question stays"
    );
}
