//! one-process-batch-2 (Kenny, 2026-10-23: "van begin tot einde moet dat 1
//! proces zijn, met een op voorhand bekend aantal stappen"): every operation
//! the dashboard can start has a plan-names function (or constant) in core,
//! next to the op, that the op plans from AND the dashboard counts with. These
//! tests run each op against a scripted executor and hold the plan the op
//! announces to that function, and every mark the op makes to that plan, so
//! the count the dashboard shows up front cannot drift from what runs.
//!
//! The default `MockExecutor` answers every command with success and empty
//! output, so an op may stop early; the plan is announced before its first
//! step either way, and a mark outside the plan panics in a debug build
//! (`Runner::check_mark`), which is what these runs would show.

use homelab_core::executor::MockExecutor;
use homelab_core::manifest::*;
use homelab_core::native::NativeServiceManifest;
use homelab_core::ops::OpCtx;
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
        trusted_sources: Vec::new(),
    }
}

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
        first_secret_cmd: None,
        metrics: None,
        release_repo: Some("kennypassenier/kyu".into()),
        release_asset: None,
    }
}

/// Every `Plan` event, in order (a fan-out such as backup-devices announces
/// one per run).
fn plans_of(events: &[PipelineEvent]) -> Vec<Vec<String>> {
    events
        .iter()
        .filter_map(|e| match e {
            PipelineEvent::Plan { steps, .. } => Some(steps.clone()),
            _ => None,
        })
        .collect()
}

/// Each run's plan, then its marks, in order: the plan must come first and
/// every mark must be a name of the plan announced before it.
fn assert_plan_then_marks(events: &[PipelineEvent], expected: &[Vec<String>]) {
    assert_eq!(plans_of(events), expected, "the announced plan(s)");
    let mut current: Vec<String> = Vec::new();
    let mut seen_plan = false;
    for e in events {
        match e {
            PipelineEvent::Plan { steps, .. } => {
                current = steps.clone();
                seen_plan = true;
            }
            PipelineEvent::StepStarted { step, .. } | PipelineEvent::StepSkipped { step, .. } => {
                assert!(seen_plan, "step {step:?} marked before any plan");
                assert!(
                    current.contains(step),
                    "step {step:?} is not in the announced plan {current:?}"
                );
            }
            _ => {}
        }
    }
}

macro_rules! run {
    ($sink:ident, |$ctx:ident| $body:expr) => {{
        let exec = MockExecutor::new();
        let $sink = VecSink::new();
        let journal = NullJournal;
        {
            let $ctx = ctx(&exec, &$sink, &journal);
            let _ = $body.await;
        }
        $sink.events()
    }};
}

fn one(names: Vec<String>) -> Vec<Vec<String>> {
    vec![names]
}

fn owned(steps: &[&str]) -> Vec<String> {
    steps.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn self_update_plans_its_constant() {
    use homelab_core::ops::selfupdate::{SELF_UPDATE_STEPS, SelfUpdateCfg, self_update};
    let ev = run!(sink, |c| self_update(&c, &SelfUpdateCfg::default()));
    assert_plan_then_marks(&ev, &one(owned(SELF_UPDATE_STEPS)));
}

#[tokio::test]
async fn seal_env_plans_its_constant() {
    use homelab_core::ops::sealenv::{SEAL_ENV_STEPS, seal_env};
    let ev = run!(sink, |c| seal_env(&c, "kitchen"));
    assert_plan_then_marks(&ev, &one(owned(SEAL_ENV_STEPS)));
}

#[tokio::test]
async fn enable_and_disable_plan_the_same_constant() {
    use homelab_core::ops::enable::{SET_ENABLED_STEPS, set_enabled};
    for on in [true, false] {
        let ev = run!(sink, |c| set_enabled(&c, "kitchen", on));
        assert_plan_then_marks(&ev, &one(owned(SET_ENABLED_STEPS)));
    }
}

#[tokio::test]
async fn resize_plans_its_constant() {
    use homelab_core::ops::resize::{RESIZE_STEPS, hot_apply};
    let m = manifest(120, "kitchen", &["web"]);
    let ev = run!(sink, |c| hot_apply(&c, &m));
    assert_plan_then_marks(&ev, &one(owned(RESIZE_STEPS)));
}

#[tokio::test]
async fn restart_host_plans_its_constant() {
    use homelab_core::ops::restarthost::{RESTART_HOST_STEPS, restart_host};
    let ev = run!(sink, |c| restart_host(&c));
    assert_plan_then_marks(&ev, &one(owned(RESTART_HOST_STEPS)));
}

#[tokio::test]
async fn template_build_plans_its_constant() {
    use homelab_core::ops::template::{TEMPLATE_BUILD_STEPS, TemplateCfg, build_template};
    let ev = run!(sink, |c| build_template(&c, &TemplateCfg::default()));
    assert_plan_then_marks(&ev, &one(owned(TEMPLATE_BUILD_STEPS)));
}

#[tokio::test]
async fn wipe_plans_its_constant() {
    use homelab_core::ops::retired::{WIPE_STEPS, wipe};
    let ev = run!(sink, |c| wipe(&c, "kitchen", "kitchen"));
    assert_plan_then_marks(&ev, &one(owned(WIPE_STEPS)));
}

#[tokio::test]
async fn backup_devices_plans_the_constant_once_per_device() {
    use homelab_core::ops::backup::BackupCfg;
    use homelab_core::ops::devicebackup::{DeviceBackup, backup_device, backup_devices_plan_names};
    let dev = |n: &str| DeviceBackup {
        name: n.into(),
        url: "https://10.10.10.1/api/core/backup/download/this".into(),
        cred_file: "/root/.config/opnsense/key".into(),
        filename: "config.xml".into(),
        pin: Some("sha256//AAAA".into()),
        ca_file: None,
    };
    let devices = [dev("opnsense"), dev("switch")];
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let journal = NullJournal;
    for d in &devices {
        let _ = backup_device(&ctx(&exec, &sink, &journal), d, &BackupCfg::default()).await;
    }
    let ev = sink.events();
    let all: Vec<String> = plans_of(&ev).into_iter().flatten().collect();
    assert_eq!(all, backup_devices_plan_names(devices.len()));
    assert_eq!(plans_of(&ev).len(), devices.len());
}

#[tokio::test]
async fn patch_plans_one_step_per_target_and_none_for_no_targets() {
    use homelab_core::ops::patch::{patch_fleet, patch_plan_names};
    let targets = vec![
        ("kitchen".to_string(), 120u16),
        ("media".to_string(), 121u16),
    ];
    let ev = run!(sink, |c| patch_fleet(&c, &targets));
    assert_plan_then_marks(&ev, &one(patch_plan_names(&["kitchen", "media"])));
    assert_eq!(patch_plan_names::<&str>(&[]), Vec::<String>::new());
    let none: Vec<(String, u16)> = Vec::new();
    let ev = run!(sink, |c| patch_fleet(&c, &none));
    assert!(plans_of(&ev).is_empty(), "no targets, no plan");
}

#[tokio::test]
async fn zfs_replicate_plans_validate_plus_five_per_job() {
    use homelab_core::ops::backup::BackupTrigger;
    use homelab_core::ops::zfs::{ZfsJob, replicate, replicate_plan_names};
    let jobs = vec![
        ZfsJob {
            source: "HDD2TB".into(),
            target: "HDD18TB/replica/HDD2TB".into(),
        },
        ZfsJob {
            source: "HDD4TB".into(),
            target: "HDD18TB/replica/HDD4TB".into(),
        },
    ];
    let ev = run!(sink, |c| replicate(&c, &jobs, &[], BackupTrigger::Manual));
    assert_plan_then_marks(&ev, &one(replicate_plan_names(&jobs)));
    assert_eq!(replicate_plan_names(&jobs).len(), 1 + 5 * jobs.len());
}

/// app-settings-enforced-1: the nightly settings pass announces its gate and
/// one step per app, and every mark it makes (a skip for an app with no
/// registered setting included) is inside that plan.
#[tokio::test]
async fn settings_enforce_plans_safety_gates_plus_one_step_per_app() {
    use homelab_core::ops::settings::{enforce, enforce_plan_names};
    let m = manifest(120, "kitchen", &["web", "db"]);
    let ev = run!(sink, |c| enforce(&c, &m));
    assert_plan_then_marks(&ev, &one(enforce_plan_names(&m)));
    assert_eq!(enforce_plan_names(&m).len(), 1 + 2);
}

#[tokio::test]
async fn update_plans_safety_gates_plus_each_selected_apps_steps() {
    use homelab_core::ops::update::{update, update_plan_names};
    let m = manifest(120, "kitchen", &["web", "db"]);
    for only in [None, Some("db")] {
        let ev = run!(sink, |c| update(&c, &m, only, false));
        assert_plan_then_marks(&ev, &one(update_plan_names(&m, only, false)));
    }
    assert_eq!(update_plan_names(&m, None, false).len(), 1 + 2 * 9);
    assert_eq!(update_plan_names(&m, Some("db"), false).len(), 1 + 9);
    assert!(update_plan_names(&m, Some("nope"), false).is_empty());
    // A scheduled run adds the pre-update copy to every app.
    assert_eq!(update_plan_names(&m, None, true).len(), 1 + 2 * 10);
}

#[tokio::test]
async fn restore_plans_the_safety_copy_steps_only_when_taken() {
    use homelab_core::ops::backup::{BackupCfg, restore_app, restore_plan_names};
    let m = manifest(120, "kitchen", &["web"]);
    for safety in [true, false] {
        let ev = run!(sink, |c| restore_app(
            &c,
            &m,
            &BackupCfg::default(),
            "latest",
            safety,
            None
        ));
        assert_plan_then_marks(&ev, &one(restore_plan_names(safety)));
    }
    assert_eq!(restore_plan_names(true).len(), 9);
    assert_eq!(restore_plan_names(false).len(), 7);
}

#[tokio::test]
async fn host_meta_backup_plans_its_constant() {
    use homelab_core::ops::backup::{BackupCfg, HOST_META_BACKUP_STEPS, backup_host_meta};
    let ev = run!(sink, |c| backup_host_meta(&c, &BackupCfg::default()));
    assert_plan_then_marks(&ev, &one(owned(HOST_META_BACKUP_STEPS)));
}

#[tokio::test]
async fn backup_plans_the_constant() {
    use homelab_core::ops::backup::{BACKUP_STEPS, BackupCfg, backup};
    let m = manifest(120, "kitchen", &["web"]);
    let ev = run!(sink, |c| backup(&c, &m, &BackupCfg::default()));
    assert_plan_then_marks(&ev, &one(owned(BACKUP_STEPS)));
}

#[tokio::test]
async fn forget_plans_its_fixed_list() {
    use homelab_core::ops::destroy::{forget, forget_plan_names};
    let ev = run!(sink, |c| forget(&c, "kitchen"));
    assert_plan_then_marks(&ev, &one(forget_plan_names()));
}

#[tokio::test]
async fn verify_restore_plans_one_step_per_repository() {
    use homelab_core::ops::backup::BackupCfg;
    use homelab_core::ops::restoredrill::{verify_restore, verify_restore_plan_names};
    let owners = vec!["web".to_string(), "db".to_string()];
    let ev = run!(sink, |c| verify_restore(
        &c,
        &BackupCfg::default(),
        "/var/tmp/drill",
        &owners,
        "latest"
    ));
    assert_plan_then_marks(&ev, &one(verify_restore_plan_names(&owners)));
}

#[tokio::test]
async fn adopt_plans_its_constant() {
    use homelab_core::ops::native::{ADOPT_STEPS, adopt};
    let ev = run!(sink, |c| adopt(&c, &kyu_manifest()));
    assert_plan_then_marks(&ev, &one(owned(ADOPT_STEPS)));
}

#[tokio::test]
async fn backup_native_plans_follow_the_pause_mode_and_statelessness() {
    use homelab_core::native::BackupPause;
    use homelab_core::ops::backup::BackupCfg;
    use homelab_core::ops::native::{backup_native, backup_native_plan_names};
    for pause in [BackupPause::Off, BackupPause::Unit, BackupPause::Chassis] {
        let m = NativeServiceManifest {
            backup_pause: pause,
            ..kyu_manifest()
        };
        let ev = run!(sink, |c| backup_native(&c, &m, &BackupCfg::default()));
        assert_plan_then_marks(&ev, &one(backup_native_plan_names(&m)));
    }
    let off = backup_native_plan_names(&kyu_manifest());
    let chassis = backup_native_plan_names(&NativeServiceManifest {
        backup_pause: BackupPause::Chassis,
        ..kyu_manifest()
    });
    assert_eq!(
        chassis.len(),
        off.len() + 2,
        "a pause step and a resume step"
    );
    // A stateless unit with nothing to archive plans nothing, and the op
    // announces nothing.
    let stateless = NativeServiceManifest {
        stateless: true,
        data_dirs: Vec::new(),
        ..kyu_manifest()
    };
    assert!(backup_native_plan_names(&stateless).is_empty());
    let ev = run!(sink, |c| backup_native(
        &c,
        &stateless,
        &BackupCfg::default()
    ));
    assert!(plans_of(&ev).is_empty());
}

#[tokio::test]
async fn update_native_plans_its_constant_only_with_an_update_cmd() {
    use homelab_core::ops::native::{UPDATE_NATIVE_STEPS, update_native, update_native_plan_names};
    let m = kyu_manifest();
    let ev = run!(sink, |c| update_native(&c, &m, None));
    assert_plan_then_marks(&ev, &one(owned(UPDATE_NATIVE_STEPS)));
    assert_eq!(update_native_plan_names(&m), owned(UPDATE_NATIVE_STEPS));
    let without = NativeServiceManifest {
        update_cmd: None,
        ..kyu_manifest()
    };
    assert!(update_native_plan_names(&without).is_empty());
    let ev = run!(sink, |c| update_native(&c, &without, None));
    assert!(plans_of(&ev).is_empty());
}

#[tokio::test]
async fn rollback_native_plans_its_constant() {
    use homelab_core::ops::native::{ROLLBACK_NATIVE_STEPS, rollback_native};
    let ev = run!(sink, |c| rollback_native(&c, &kyu_manifest()));
    assert_plan_then_marks(&ev, &one(owned(ROLLBACK_NATIVE_STEPS)));
}

#[tokio::test]
async fn restore_native_plans_follow_the_manifest() {
    use homelab_core::ops::backup::BackupCfg;
    use homelab_core::ops::native::{restore_native, restore_native_plan_names};
    let with_reseed = NativeServiceManifest {
        after_restore: Some("kyu reindex".into()),
        ..kyu_manifest()
    };
    let stateless = NativeServiceManifest {
        stateless: true,
        ..kyu_manifest()
    };
    for m in [kyu_manifest(), with_reseed.clone(), stateless.clone()] {
        let ev = run!(sink, |c| restore_native(
            &c,
            &m,
            &BackupCfg::default(),
            "latest",
            Some("kyu")
        ));
        assert_plan_then_marks(&ev, &one(restore_native_plan_names(&m)));
    }
    assert_eq!(restore_native_plan_names(&with_reseed).len(), 6);
    assert_eq!(restore_native_plan_names(&kyu_manifest()).len(), 5);
    assert_eq!(restore_native_plan_names(&stateless).len(), 1);
}

#[tokio::test]
async fn release_update_and_install_release_plan_the_composed_names() {
    use homelab_core::ops::native::{install_release, release_install_plan_names, release_update};
    let m = kyu_manifest();
    let ev = run!(sink, |c| release_update(&c, &m));
    assert_plan_then_marks(&ev, &one(release_install_plan_names(&m, None, None)));
    let ev = run!(sink, |c| install_release(&c, &m, "v1.2.3", "[Unit]\n"));
    assert_plan_then_marks(
        &ev,
        &one(release_install_plan_names(
            &m,
            Some("v1.2.3"),
            Some("[Unit]\n"),
        )),
    );
    // No release_repo: nothing is attempted, nothing is planned.
    let none = NativeServiceManifest {
        release_repo: None,
        ..kyu_manifest()
    };
    assert!(release_install_plan_names(&none, None, None).is_empty());
}

#[test]
fn apply_guards_is_one_fixed_step() {
    assert_eq!(homelab_core::ops::guards::APPLY_GUARDS_STEPS, &["guards"]);
}
