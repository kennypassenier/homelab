//! fix-171 round 2: an operation announces its own step plan before its
//! first step mark, so a client's "step n/m" total is known and fixed from
//! the very first mark — never a guess from a past run's count (the first
//! attempt, fix-171, held `m` at the newest successful run's own step
//! count, which is still a guess about THIS run from a DIFFERENT one).
//!
//! Covers, per the Register's ask: a deploy of a stack with several apps, a
//! deploy with a firewall/routes declared, install-native, and a per-app
//! update whose later steps are skipped rather than silently absent.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::native::NativeServiceManifest;
use homelab_core::ops::native::{INSTALL_STEPS, install_native};
use homelab_core::ops::update::update;
use homelab_core::ops::{OpCtx, deploy, deploy::deploy as run_deploy, destroy};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::{PipelineEvent, VecSink};

fn manifest(vmid: u16, stack: &str, apps: &[&str]) -> StackManifest {
    StackManifest {
        home_address_whitelist: None,
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
            host_path: format!("/appdata/{stack}/{}-config", apps[0]),
            mount_point: format!("/appdata/{stack}/{}-config", apps[0]),
            no_data: false,
            no_backup: None,
            host_owner_uid: Some(101000),
            app: apps.first().map(|a| a.to_string()),
            postgres_check_image: None,
        }],
        apps: apps.iter().map(|a| a.to_string()).collect(),
    }
}

fn spec(vmid: u16, stack: &str, apps: &[&str]) -> DeploySpec {
    DeploySpec {
        secret_files: Vec::new(),
        source: None,
        native_binaries: Default::default(),
        native_manifests: Default::default(),
        manifest: manifest(vmid, stack, apps),
        files: vec![FileBlob {
            path: format!("{stack}/docker-compose.yml"),
            content: "services: {}\n".into(),
            mode: None,
        }],
        env: std::collections::BTreeMap::new(),
        extra_routes: Vec::new(),
        gateway_route: Some(GatewayRoute {
            gateway_vmid: 104,
            filename: format!("{vmid}-app-{stack}.yml"),
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
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
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

fn plan_of(events: &[PipelineEvent]) -> Vec<String> {
    events
        .iter()
        .find_map(|e| match e {
            PipelineEvent::Plan { steps, .. } => Some(steps.clone()),
            _ => None,
        })
        .expect("an op must announce its plan before its first step")
}

/// Every run-or-skip mark, deduplicated by name (a step both starts and
/// finishes, so it appears twice in the raw event stream).
fn marked_names(events: &[PipelineEvent]) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    for e in events {
        match e {
            PipelineEvent::StepStarted { step, .. } | PipelineEvent::StepSkipped { step, .. } => {
                names.insert(step.clone());
            }
            _ => {}
        }
    }
    names
}

// ── deploy ───────────────────────────────────────────────────────────────

/// The plan is the very first event deploy emits, before "validate" — true
/// regardless of whether the deploy goes on to succeed.
#[tokio::test]
async fn deploy_announces_its_plan_before_any_step() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let journal = NullJournal;
    let _ = run_deploy(
        &ctx(&exec, &sink, &journal),
        &spec(110, "syncthing", &["syncthing"]),
    )
    .await;
    let events = sink.events();
    let plan_idx = events
        .iter()
        .position(|e| matches!(e, PipelineEvent::Plan { .. }))
        .expect("a plan was announced");
    assert_eq!(
        plan_idx, 0,
        "the plan is the very first event, before any step"
    );
    let first_step = events
        .iter()
        .position(|e| {
            matches!(
                e,
                PipelineEvent::StepStarted { .. } | PipelineEvent::StepSkipped { .. }
            )
        })
        .expect("at least one step mark");
    assert!(plan_idx < first_step);
}

/// fix-171 round 2 (Kenny's "N (total) must be STATIC and CORRECT FROM STEP
/// 1, for every action"): the announced plan length is deploy's own fixed
/// constant, unaffected by how many apps the manifest declares (apps are
/// deployed inside ONE "start apps" step, never one step per app) or
/// whether a firewall/route is declared — those add no new step names,
/// they only decide which of the fixed names run versus get a skip mark.
#[tokio::test]
async fn deploy_plan_length_is_independent_of_app_count() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let journal = NullJournal;
    let one_app = spec(110, "syncthing", &["syncthing"]);
    let _ = run_deploy(&ctx(&exec, &sink, &journal), &one_app).await;
    let plan_one = plan_of(&sink.events());

    let exec2 = MockExecutor::new();
    let sink2 = VecSink::new();
    let many_apps = spec(111, "media", &["jellyfin", "sonarr", "radarr", "bazarr"]);
    let _ = run_deploy(&ctx(&exec2, &sink2, &journal), &many_apps).await;
    let plan_many = plan_of(&sink2.events());

    assert_eq!(plan_one.len(), deploy::STEPS.len());
    assert_eq!(plan_many.len(), deploy::STEPS.len());
    assert_eq!(
        plan_one, plan_many,
        "the plan is the same ordered list regardless of app count"
    );
}

/// A step the plan lists but whose precondition does not hold this run
/// (no firewall declared, no log shipper configured) still gets a mark —
/// `skip`, not silence — so counting run-or-skipped names reaches the
/// announced total exactly.
#[tokio::test]
async fn deploy_a_run_with_no_firewall_or_log_shipper_skips_those_steps_and_still_reaches_m() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let s = spec(110, "syncthing", &["syncthing"]);
    let report = run_deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let events = sink.events();
    let plan = plan_of(&events);
    assert_eq!(plan.len(), deploy::STEPS.len());

    let names = marked_names(&events);
    assert_eq!(
        names.len(),
        deploy::STEPS.len(),
        "n (run + skipped) must reach m exactly"
    );
    let expected: std::collections::BTreeSet<String> =
        deploy::STEPS.iter().map(|s| s.to_string()).collect();
    assert_eq!(names, expected);

    // No firewall was declared and no log shipper is configured in this
    // ctx: those steps must be explicit skips, not silently absent.
    let skipped: std::collections::BTreeSet<String> = events
        .iter()
        .filter_map(|e| match e {
            PipelineEvent::StepSkipped { step, .. } => Some(step.clone()),
            _ => None,
        })
        .collect();
    for name in [
        "firewall",
        "firewall after create",
        "refresh the tile watcher's firewall",
        "retire gateway route",
        "log shipper",
    ] {
        assert!(skipped.contains(name), "{name} should have been skipped");
    }
    // A route IS declared in this spec, so "gateway route" must have run,
    // not been skipped.
    assert!(!skipped.contains("gateway route"));
}

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

// ── destroy ──────────────────────────────────────────────────────────────

/// destroy's plan is a fixed constant too; every one of its steps is
/// already unconditional, so none is ever skipped.
#[tokio::test]
async fn destroy_announces_its_fixed_plan_and_every_step_runs() {
    let exec = MockExecutor::new();
    exec.respond_always("pct config", CmdOutput::ok("hostname: 110-app-syncthing"));
    exec.respond_always("pct list", CmdOutput::ok(""));
    let sink = VecSink::new();
    let journal = NullJournal;
    let m = manifest(110, "syncthing", &["syncthing"]);
    let report = destroy::destroy(&ctx(&exec, &sink, &journal), &m, "syncthing", true).await;
    assert!(report.ok, "destroy failed: {:?}", report.error);

    let events = sink.events();
    let plan = plan_of(&events);
    assert_eq!(plan.len(), destroy::STEPS.len());
    assert_eq!(
        plan,
        destroy::STEPS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
    let names = marked_names(&events);
    assert_eq!(names.len(), destroy::STEPS.len());
    // Nothing in destroy is conditional on absent input: no skip marks.
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, PipelineEvent::StepSkipped { .. }))
    );
}

// ── install-native ───────────────────────────────────────────────────────

const UNIT_FILE: &str = "[Unit]\nDescription=kyu\n\n[Service]\n\
                         ExecStart=/usr/local/bin/kyu\n\n[Install]\n\
                         WantedBy=multi-user.target\n";

fn kyu_manifest() -> NativeServiceManifest {
    NativeServiceManifest {
        restore_note: None,
        stack_name: "kyu".into(),
        vmid: 109,
        hostname: "109-app-kyu".into(),
        unit: "kyu".into(),
        binary: "/usr/local/bin/kyu".into(),
        env_file: Some("/etc/kyu/kyu.env".into()),
        data_dirs: vec!["/var/lib/kyu".into()],
        update_cmd: Some("kyu update".into()),
        stateless: false,
        backup_from_newest: None,
        backup_pause: homelab_core::native::BackupPause::Off,
        update_policy: Default::default(),
        after_restore: None,
        metrics: None,
        release_repo: Some("kennypassenier/kyu".into()),
        release_asset: None,
    }
}

fn adopt_mocks(exec: &MockExecutor) {
    exec.respond_always("pct config 109", CmdOutput::ok("hostname: 109-app-kyu\n"));
    exec.respond_always("systemctl is-active", CmdOutput::ok("active\n"));
    exec.respond_always(
        "systemctl show",
        CmdOutput::ok(
            "ExecStart={ path=/usr/local/bin/kyu ; argv[]=/usr/local/bin/kyu }\n\
             EnvironmentFiles=/etc/kyu/kyu.env (ignore_errors=no)\n",
        ),
    );
    exec.respond_always("test -x", CmdOutput::ok(""));
}

fn glibc_ok(exec: &MockExecutor) {
    exec.respond_always("GNU_LIBC_VERSION", CmdOutput::ok("need=none have=2.41\n"));
}

/// install-native's plan is a fixed constant, announced before its first
/// step, matching the number of marks a successful install actually
/// produces.
#[tokio::test]
async fn install_native_announces_its_fixed_plan_before_the_first_step() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    glibc_ok(&exec);
    exec.respond_always("test -f", CmdOutput::ok("no\n"));
    exec.respond_always("base64 -d", CmdOutput::ok(""));
    exec.respond_always("systemctl daemon-reload", CmdOutput::ok(""));
    exec.respond_always("systemctl stop", CmdOutput::ok(""));
    exec.respond_always("cat /var/lib/homelab/state.json", CmdOutput::failed(1, ""));
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = install_native(
        &ctx(&exec, &sink, &journal),
        &kyu_manifest(),
        "YmluYXJ5",
        UNIT_FILE,
    )
    .await;
    assert!(report.ok, "{:?}", report.error);

    let events = sink.events();
    let plan_idx = events
        .iter()
        .position(|e| matches!(e, PipelineEvent::Plan { .. }))
        .expect("a plan was announced");
    assert_eq!(plan_idx, 0);
    let plan = plan_of(&events);
    assert_eq!(plan.len(), INSTALL_STEPS.len());
    assert_eq!(
        plan,
        INSTALL_STEPS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
}

// ── update: a per-app loop whose later steps are skipped, not absent ─────

/// fix-171 round 2: `update`'s per-app steps are a genuinely branching case
/// (a `continue` after a policy/busy/pull failure drops the REST of that
/// app's steps). The plan still lists every name any app could produce,
/// known from the app list and `auto` alone, and a `continue` fills every
/// step it will not reach with an explicit skip — so `n` still reaches the
/// announced `m` exactly even though this app never got past "policy".
#[tokio::test]
async fn update_skips_the_rest_of_an_apps_steps_when_its_policy_gate_says_skip() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config 112",
        CmdOutput::ok("hostname: 112-app-twoapps\n"),
    );
    // Every container of both apps carries a policy OTHER than "auto", so
    // the scheduled run skips each of them outright.
    exec.respond_always(
        "docker inspect",
        CmdOutput::ok("manual|svc\n"), // policy|service, not "auto"
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut m = manifest(112, "twoapps", &["app-one", "app-two"]);
    m.apps = vec!["app-one".into(), "app-two".into()];
    let report = update(&ctx(&exec, &sink, &journal), &m, None, true).await;

    let events = sink.events();
    let plan = plan_of(&events);
    // "safety gates" + 5 names per app (no "pre-update copy" skip-branch
    // reached because the policy gate stops first) ... the PLAN still lists
    // every name `auto=true` could produce for each app (9 names each).
    assert_eq!(plan.len(), 1 + 2 * 9);

    let names = marked_names(&events);
    assert_eq!(
        names.len(),
        plan.len(),
        "n (run + skipped) must reach m exactly, even though every app's \
         policy gate skipped the rest of its own steps: {:?}",
        report.error
    );
    for app in ["app-one", "app-two"] {
        for suffix in [
            "capture",
            "running before",
            "busy check",
            "pull",
            "pre-update copy",
            "stop-first",
            "up",
            "verify",
        ] {
            let name = format!("{app} :: {suffix}");
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    PipelineEvent::StepSkipped { step, .. } if step == &name
                )),
                "{name} should have been skipped, not silently absent"
            );
        }
    }
}

// ── fix-171 round 3: the remaining single-purpose ops, each with a fixed,
// unconditional plan. ───────────────────────────────────────────────────────

#[tokio::test]
async fn restart_host_announces_its_one_step_plan() {
    use homelab_core::ops::restarthost::restart_host;
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = restart_host(&ctx(&exec, &sink, &journal)).await;
    assert!(report.ok, "{:?}", report.error);
    let events = sink.events();
    let m = events
        .iter()
        .find_map(|e| match e {
            PipelineEvent::Plan { steps, .. } => Some(steps.len()),
            _ => None,
        })
        .expect("a plan was announced");
    assert_eq!(m, 1);
}
