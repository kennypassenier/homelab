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
        no_apps_yet: false,
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
        backup_first: false,
        client_schema: homelab_core::manifest::CURRENT_CLIENT_SCHEMA,
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

/// destroy's plan is a fixed constant too — fix-step-plan-nested: that
/// constant now COMPOSES the `backup` op it always marks one way or the
/// other ("backup before destroy" used to hide a whole second standalone op
/// announcing its own separate plan, the same shape that let the kyu
/// counter climb past its announced total LIVE on 2026-10-02). With
/// `skip_backup: true` here, every one of `backup`'s own names is
/// skip-marked rather than run — still one mark each, still part of the ONE
/// plan destroy itself announced.
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
    // fix-step-plan-nested: exactly one Plan event for the whole job — a
    // nested op (backup) must never announce a second one.
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, PipelineEvent::Plan { .. }))
            .count(),
        1,
        "a composed op announces ONE plan, not one per nested call"
    );
    let plan = plan_of(&events);
    let expected = destroy::destroy_plan_names(&m.stack_name);
    assert_eq!(plan.len(), expected.len());
    assert_eq!(plan, expected);
    let names = marked_names(&events);
    assert_eq!(names.len(), expected.len());
    // n never exceeds m: every mark lands on a name from the plan above.
    assert!(names.iter().all(|n| expected.contains(n)));
    // `--no-backup` (skip_backup) skip-marks every one of the nested
    // `backup` names plus the restore-check of that backup, and only those —
    // destroy's own steps still all ran.
    let skipped: std::collections::BTreeSet<String> = events
        .iter()
        .filter_map(|e| match e {
            PipelineEvent::StepSkipped { step, .. } => Some(step.clone()),
            _ => None,
        })
        .collect();
    let mut backup_names: std::collections::BTreeSet<String> =
        homelab_core::ops::backup::backup_plan_names(&m.stack_name)
            .into_iter()
            .collect();
    backup_names.insert(destroy::VERIFY_RESTORE_STEP.to_string());
    assert_eq!(skipped, backup_names);
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
/// produces. fix-step-plan-nested: `install_native` always ends by calling
/// `adopt` — that was ALREADY true before this fix, just invisibly: `adopt`
/// ran for real but announced its OWN separate plan under its own op id, so
/// this test's old assertion (`plan.len() == INSTALL_STEPS.len()`, 9) only
/// checked install's own slice and never noticed adopt's further 8 marks
/// arriving on a second, unchecked `Plan` event — exactly the blind spot
/// that let the kyu counter climb LIVE on 2026-10-02. Now there is only one
/// `Plan` event, and it already lists adopt's 8 names too.
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
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, PipelineEvent::Plan { .. }))
            .count(),
        1,
        "install_native + its nested adopt must announce ONE plan, not two"
    );
    let plan_idx = events
        .iter()
        .position(|e| matches!(e, PipelineEvent::Plan { .. }))
        .expect("a plan was announced");
    assert_eq!(plan_idx, 0);
    let plan = plan_of(&events);
    let expected = homelab_core::ops::native::install_plan_names(&kyu_manifest(), None);
    assert_eq!(
        plan.len(),
        INSTALL_STEPS.len() + 8,
        "9 own + 8 nested adopt"
    );
    assert_eq!(plan, expected);

    // Every name marked is one the plan actually listed, and every listed
    // name got exactly one mark (run or skip) — n never exceeds m.
    let names = marked_names(&events);
    assert_eq!(names.len(), expected.len());
    assert!(names.iter().all(|n| expected.contains(n)));
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

// ── redesign-stacks-6: a backup before the deploy changes anything ─────────

/// An existing container with its data on the host: what a backup reads.
fn script_existing(exec: &MockExecutor) {
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct config", CmdOutput::ok("hostname: 110-app-syncthing\n"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("test -d", CmdOutput::ok("yes\n"));
    exec.respond_always("du -sbc", CmdOutput::ok("1000\n50000000000\n"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
}

/// The step marks in the order they happened.
fn marks_in_order(events: &[PipelineEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            PipelineEvent::StepStarted { step, .. } | PipelineEvent::StepSkipped { step, .. } => {
                Some(step.clone())
            }
            _ => None,
        })
        .collect()
}

/// The Stacks page says "each stack is backed up first" for Deploy all
/// changes and the batch Deploy: the backup is part of the deploy's one
/// plan, right after the read-only gates, it runs (tagged as taken before a
/// deploy) and it comes before the first step that changes anything.
#[tokio::test]
async fn redesign_stacks_6_a_deploy_asked_to_back_up_first_backs_up_before_it_changes_anything() {
    let exec = MockExecutor::new();
    script_existing(&exec);
    exec.respond_always("restic backup", CmdOutput::ok(""));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing", &["syncthing"]);
    s.backup_first = true;
    let _ = run_deploy(&ctx(&exec, &sink, &journal), &s).await;
    let events = sink.events();

    let plan = plan_of(&events);
    let backup_names = homelab_core::ops::backup::backup_plan_names("syncthing");
    let gates = plan.iter().position(|n| n == "safety gates").unwrap();
    assert_eq!(
        plan[gates + 1..gates + 1 + backup_names.len()].to_vec(),
        backup_names,
        "the backup's steps follow the safety gates in the announced plan"
    );
    assert_eq!(plan, deploy::plan_names(&s));
    assert_eq!(plan.len(), deploy::STEPS.len() + backup_names.len());

    let snaps = exec.calls_containing("restic backup");
    assert!(
        !snaps.is_empty() && snaps.iter().all(|c| c.contains("--tag trigger:pre-deploy")),
        "the backup ran, tagged as taken before a deploy: {snaps:?}"
    );
    let marks = marks_in_order(&events);
    let snapshot = marks
        .iter()
        .position(|n| n == "backup-syncthing :: snapshot")
        .expect("the snapshot step was marked");
    let first_change = marks
        .iter()
        .position(|n| n == "registry cache")
        .expect("the deploy went on after the backup");
    assert!(
        snapshot < first_change,
        "the backup comes before the first changing step: {marks:?}"
    );
}

/// A backup that fails changes nothing: the deploy stops right there with
/// a refusal that says why, and no step after the backup runs.
#[tokio::test]
async fn redesign_stacks_6_a_failed_backup_stops_the_deploy_before_anything_changes() {
    let exec = MockExecutor::new();
    script_existing(&exec);
    exec.respond_always(
        "restic backup",
        CmdOutput::failed(1, "repository is locked"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing", &["syncthing"]);
    s.backup_first = true;
    let report = run_deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(!report.ok, "a failed backup must stop the deploy");
    let why = report.error.unwrap().why;
    assert!(
        why.contains("refusing to deploy 'syncthing': the backup taken first did not succeed"),
        "{why}"
    );
    let marks = marks_in_order(&sink.events());
    for after in [
        "registry cache",
        "host storage",
        "provision container",
        "push files",
    ] {
        assert!(
            !marks.iter().any(|n| n == after),
            "{after} ran after a failed backup: {marks:?}"
        );
    }
    for changing in ["pct create", "pct set", "pct push", "pct start"] {
        assert!(
            exec.calls_containing(changing).is_empty(),
            "{changing} ran after a failed backup"
        );
    }
}

/// A stack that does not exist yet has nothing to back up: the backup's
/// names are skip-marked (still one mark each), and a deploy not asked to
/// back up plans exactly as before.
#[tokio::test]
async fn redesign_stacks_6_a_new_stack_skips_the_backup_and_a_plain_deploy_plans_as_before() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing", &["syncthing"]);
    s.backup_first = true;
    let _ = run_deploy(&ctx(&exec, &sink, &journal), &s).await;
    let events = sink.events();
    let skipped: std::collections::BTreeSet<String> = events
        .iter()
        .filter_map(|e| match e {
            PipelineEvent::StepSkipped { step, .. } => Some(step.clone()),
            _ => None,
        })
        .collect();
    for n in homelab_core::ops::backup::backup_plan_names("syncthing") {
        assert!(skipped.contains(&n), "{n} was not skip-marked");
    }
    assert!(exec.calls_containing("restic backup").is_empty());

    let plain = spec(110, "syncthing", &["syncthing"]);
    assert_eq!(
        deploy::plan_names(&plain),
        deploy::STEPS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
}

/// redesign-stacks-8: an empty stack says its apps come later
/// (`no_apps_yet`), the way `native_only` says it runs no docker; the
/// marker with apps declared, or beside `native_only`, is refused.
#[test]
fn redesign_stacks_8_an_empty_stack_is_valid_only_when_it_says_its_apps_come_later() {
    use homelab_core::manifest::validate_manifest;
    let mut m = manifest(150, "blank", &["x"]);
    m.apps.clear();
    m.storage.clear();
    assert!(validate_manifest(&m).is_err(), "an unexplained empty list");
    m.no_apps_yet = true;
    validate_manifest(&m).expect("an empty stack that says so");
    m.apps = vec!["web".into()];
    let e = validate_manifest(&m).unwrap_err().to_string();
    assert!(
        e.contains("no_apps_yet is set but the stack declares apps"),
        "{e}"
    );
}
