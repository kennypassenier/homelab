//! one-process-batch-2 (Kenny, 2026-10-23: "van begin tot einde moet dat 1
//! proces zijn, met een op voorhand bekend aantal stappen"): every command
//! that reaches the host has its step names in core, next to the op that runs
//! them, and the dashboard names a job's steps from those same functions
//! before anything is sent. These tests hold `commands_plan_with` to the core
//! functions command by command, and say exactly which commands stay
//! estimates without the facts they need.

mod act_support;

use act_support::{manifest, native, spec};
use homelab_admin::core::actions_progress::{
    PlanFacts, Tracker, command_stack, commands_plan_with, facts_needed, member_plan_with,
};
use homelab_core::ops;
use homelab_proto::Command;

fn names(commands: &[Command], facts: &PlanFacts) -> Vec<String> {
    let plan = commands_plan_with(commands, facts);
    assert!(plan.exact(), "{:?}", plan.estimated);
    plan.names()
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn commands_that_carry_their_inputs_are_named_from_the_core_plan() {
    let f = PlanFacts::default();
    let m = manifest("media");
    let apps = m.apps.clone();
    let stack = m.stack_name.clone();

    assert_eq!(
        names(
            &[Command::UpdateStack {
                manifest: Box::new(m.clone()),
                app: None
            }],
            &f
        ),
        ops::update::update_plan_names(&m, None, false)
    );
    let one = apps[0].clone();
    assert_eq!(
        names(
            &[Command::UpdateStack {
                manifest: Box::new(m.clone()),
                app: Some(one.clone())
            }],
            &f
        ),
        ops::update::update_plan_names(&m, Some(&one), false)
    );
    assert_eq!(
        names(&[Command::BackupStack(Box::new(m.clone()))], &f),
        strs(ops::backup::BACKUP_STEPS)
    );
    for skip in [false, true] {
        assert_eq!(
            names(
                &[Command::RestoreStack {
                    manifest: Box::new(m.clone()),
                    snapshot: "latest".into(),
                    confirm: Some(stack.clone()),
                    skip_safety_copy: skip,
                    app: None,
                    source: None,
                }],
                &f
            ),
            ops::backup::restore_plan_names(!skip)
        );
    }
    assert_eq!(
        names(&[Command::ApplyResources(Box::new(m.clone()))], &f),
        strs(ops::resize::RESIZE_STEPS)
    );
    assert_eq!(
        names(
            &[Command::SetStackEnabled {
                stack: stack.clone(),
                enabled: true
            }],
            &f
        ),
        strs(ops::enable::SET_ENABLED_STEPS)
    );
    assert_eq!(
        names(
            &[Command::SealEnv {
                stack: stack.clone()
            }],
            &f
        ),
        strs(ops::sealenv::SEAL_ENV_STEPS)
    );
    assert_eq!(
        names(
            &[Command::ForgetStack {
                stack: stack.clone()
            }],
            &f
        ),
        ops::destroy::forget_plan_names()
    );
    assert_eq!(
        names(
            &[Command::DestroyStack {
                manifest: Box::new(m.clone()),
                confirm: stack.clone(),
                skip_backup: false
            }],
            &f
        ),
        ops::destroy::destroy_plan_names(&stack)
    );
    assert_eq!(
        names(&[Command::BackupHostMeta], &f),
        strs(ops::backup::HOST_META_BACKUP_STEPS)
    );
    assert_eq!(
        names(&[Command::ApplyGuards { vmid: 120 }], &f),
        strs(ops::guards::APPLY_GUARDS_STEPS)
    );
    assert_eq!(
        names(&[Command::RestartHost], &f),
        strs(ops::restarthost::RESTART_HOST_STEPS)
    );
    assert_eq!(
        names(
            &[Command::BuildTemplate {
                temp_vmid: 9000,
                version: 4,
                unprivileged: true,
                base_template: None
            }],
            &f
        ),
        strs(ops::template::TEMPLATE_BUILD_STEPS)
    );
    assert_eq!(
        names(
            &[Command::WipeRetired {
                name: "old".into(),
                confirm: Some("old".into())
            }],
            &f
        ),
        strs(ops::retired::WIPE_STEPS)
    );
    // The preview of a wipe (no typed name) runs no steps.
    assert!(
        names(
            &[Command::WipeRetired {
                name: "old".into(),
                confirm: None
            }],
            &f
        )
        .is_empty()
    );
    let n = native("admin");
    assert_eq!(
        names(&[Command::AdoptService(Box::new(n.clone()))], &f),
        strs(ops::native::ADOPT_STEPS)
    );
    assert_eq!(
        names(
            &[Command::RollbackNative {
                stack: "admin".into(),
                unit: None
            }],
            &f
        ),
        strs(ops::native::ROLLBACK_NATIVE_STEPS)
    );
    assert_eq!(
        names(
            &[Command::InstallNativeRelease {
                manifest: Box::new(n.clone()),
                unit_file: "[Unit]\n".into(),
                tag: "v1.0.0".into()
            }],
            &f
        ),
        ops::native::release_install_plan_names(&n, Some("v1.0.0"), Some("[Unit]\n"))
    );
}

#[test]
fn a_command_that_runs_no_steps_is_exact_with_none() {
    let f = PlanFacts::default();
    for c in [
        Command::Ping,
        Command::Status,
        Command::GetState,
        Command::ExecIn {
            vmid: 120,
            command: "uptime".into(),
        },
        Command::StageNativeBinary {
            stack: "admin".into(),
            unit: "admin".into(),
            binary_b64: String::new(),
        },
    ] {
        let plan = commands_plan_with(&[c], &f);
        assert!(plan.exact());
        assert!(plan.names().is_empty());
    }
}

#[test]
fn native_commands_are_estimated_until_the_stacks_services_are_read() {
    let n = native("admin");
    let cmds = [
        Command::BackupNative {
            stack: "admin".into(),
        },
        Command::UpdateNative {
            stack: "admin".into(),
        },
        Command::ReleaseUpdateNative {
            stack: "admin".into(),
        },
        Command::RestoreNative {
            stack: "admin".into(),
            snapshot: "latest".into(),
            confirm: None,
            unit: None,
            source: None,
        },
    ];
    for c in &cmds {
        let plan = commands_plan_with(std::slice::from_ref(c), &PlanFacts::default());
        assert!(!plan.exact(), "{c:?}");
        assert!(plan.estimated[0].contains("native services were not read"));
    }
    assert_eq!(facts_needed(&cmds).natives, vec!["admin".to_string()]);

    let mut facts = PlanFacts::default();
    facts.natives.insert("admin".into(), vec![n.clone()]);
    assert_eq!(
        names(&cmds[..1], &facts),
        ops::native::backup_native_plan_names(&n)
    );
    assert_eq!(
        names(&cmds[1..2], &facts),
        ops::native::update_native_plan_names(&n)
    );
    assert_eq!(
        names(&cmds[2..3], &facts),
        ops::native::release_install_plan_names(&n, None, None)
    );
    assert_eq!(
        names(&cmds[3..4], &facts),
        ops::native::restore_native_plan_names(&n)
    );
}

#[test]
fn a_stack_of_several_native_units_is_one_part_per_unit_and_a_fixed_sum() {
    let a = native("admin");
    let mut b = a.clone();
    b.unit = "admin-two".into();
    let mut facts = PlanFacts::default();
    facts
        .natives
        .insert("admin".into(), vec![a.clone(), b.clone()]);
    let c = [Command::ReleaseUpdateNative {
        stack: "admin".into(),
    }];
    let plan = commands_plan_with(&c, &facts);
    assert!(plan.exact());
    assert!(plan.multi(), "the host announces one plan per unit");
    assert_eq!(
        plan.names().len(),
        2 * ops::native::release_install_plan_names(&a, None, None).len()
    );
    // Each unit's steps carry the unit when a stack runs several.
    let labeled = plan.labeled();
    assert!(labeled[0].starts_with("admin · "), "{}", labeled[0]);
    assert!(labeled.last().unwrap().starts_with("admin-two · "));
}

#[test]
fn restore_native_of_one_unit_names_only_that_unit() {
    let a = native("admin");
    let mut b = a.clone();
    b.unit = "admin-two".into();
    let mut facts = PlanFacts::default();
    facts
        .natives
        .insert("admin".into(), vec![a.clone(), b.clone()]);
    let c = [Command::RestoreNative {
        stack: "admin".into(),
        snapshot: "latest".into(),
        confirm: None,
        unit: Some("admin-two".into()),
        source: None,
    }];
    let plan = commands_plan_with(&c, &facts);
    assert_eq!(plan.names(), ops::native::restore_native_plan_names(&b));
    assert!(!plan.multi());
}

#[test]
fn patch_zfs_devices_and_verify_restore_need_their_facts() {
    let c = [Command::PatchFleet];
    assert!(!commands_plan_with(&c, &PlanFacts::default()).exact());
    let mut f = PlanFacts {
        fleet_stacks: Some(vec!["kitchen".into(), "media".into()]),
        ..Default::default()
    };
    assert_eq!(
        names(&c, &f),
        ops::patch::patch_plan_names(&["kitchen", "media"])
    );
    f.fleet_stacks = Some(Vec::new());
    assert!(
        names(&c, &f).is_empty(),
        "no managed stacks, nothing planned"
    );

    let z = [Command::ZfsReplicate];
    assert!(!commands_plan_with(&z, &PlanFacts::default()).exact());
    let jobs = vec![ops::zfs::ZfsJob {
        source: "HDD2TB".into(),
        target: "HDD18TB/replica/HDD2TB".into(),
    }];
    let f = PlanFacts {
        zfs_jobs: Some(jobs.clone()),
        ..Default::default()
    };
    assert_eq!(names(&z, &f), ops::zfs::replicate_plan_names(&jobs));

    let d = [Command::BackupDevices];
    assert!(!commands_plan_with(&d, &PlanFacts::default()).exact());
    let f = PlanFacts {
        device_backups: Some(vec!["opnsense".into(), "switch".into()]),
        ..Default::default()
    };
    let plan = commands_plan_with(&d, &f);
    assert!(plan.exact());
    assert!(plan.multi(), "one device-backup plan per device");
    assert_eq!(
        plan.names(),
        ops::devicebackup::backup_devices_plan_names(2)
    );

    let v = |app: Option<&str>, snapshot: &str| {
        [Command::VerifyRestore {
            stack: "media".into(),
            app: app.map(str::to_string),
            snapshot: snapshot.into(),
            source: None,
        }]
    };
    assert!(!commands_plan_with(&v(None, "latest"), &PlanFacts::default()).exact());
    let mut f = PlanFacts::default();
    f.repositories
        .insert("media".into(), vec!["sonarr".into(), "radarr".into()]);
    assert_eq!(
        names(&v(None, "latest"), &f),
        ops::restoredrill::verify_restore_plan_names(&["sonarr", "radarr"])
    );
    assert_eq!(
        names(&v(Some("radarr"), "latest"), &f),
        ops::restoredrill::verify_restore_plan_names(&["radarr"])
    );
    // A request the host refuses reports no steps.
    assert!(names(&v(Some("nope"), "latest"), &f).is_empty());
    assert!(names(&v(None, "abc123"), &f).is_empty());
}

#[test]
fn a_job_across_stacks_labels_each_step_with_its_stack() {
    let a = spec("media");
    let b = spec("kitchen");
    let cmds = [
        Command::DeployStack(Box::new(a.clone())),
        Command::DeployStack(Box::new(b.clone())),
        Command::DestroyRecorded {
            stack: "old".into(),
            confirm: "old".into(),
            skip_backup: false,
        },
    ];
    let plan = commands_plan_with(&cmds, &PlanFacts::default());
    assert!(plan.exact());
    let labeled = plan.labeled();
    assert_eq!(labeled.len(), plan.names().len());
    assert_eq!(
        labeled[0],
        format!("media · {}", ops::deploy::plan_names(&a)[0])
    );
    let first_b = ops::deploy::plan_names(&a).len();
    assert_eq!(
        labeled[first_b],
        format!("kitchen · {}", ops::deploy::plan_names(&b)[0])
    );
    assert!(labeled.last().unwrap().starts_with("old · "));
    // One stack, one command: the names stay bare.
    let one = commands_plan_with(&cmds[..1], &PlanFacts::default());
    assert_eq!(one.labeled(), one.names());
    assert_eq!(command_stack(&cmds[2]).as_deref(), Some("old"));
}

#[test]
fn a_member_is_exact_when_its_commands_are_named_and_says_why_when_not() {
    let m = member_plan_with(
        1,
        "media",
        Some(&[Command::SealEnv {
            stack: "media".into(),
        }]),
        &PlanFacts::default(),
        &[],
        "seal-env-media",
    );
    assert!(m.exact);
    assert_eq!(m.steps, strs(ops::sealenv::SEAL_ENV_STEPS));
    assert!(m.estimated_because.is_empty());

    let e = member_plan_with(
        2,
        "media",
        Some(&[Command::BackupNative {
            stack: "media".into(),
        }]),
        &PlanFacts::default(),
        &[],
        "backup-native-media",
    );
    assert!(!e.exact);
    assert_eq!(e.estimated_because.len(), 1);

    let unread = member_plan_with(3, "media", None, &PlanFacts::default(), &[], "x");
    assert!(!unread.exact);
    assert!(!unread.estimated_because.is_empty());
}

#[test]
fn an_expected_total_shows_at_once_and_the_hosts_announcement_replaces_it() {
    let mut t = Tracker::new(Vec::new());
    t.expect_total(5);
    let p = t.planned("update-media").expect("a total is known");
    assert_eq!((p.n, p.m), (0, Some(5)));
    // The host's own plan of the same length changes nothing...
    let steps: Vec<String> = (0..5).map(|i| format!("s{i}")).collect();
    t.on_plan("update-media", &steps);
    assert_eq!(t.planned("update-media").unwrap().m, Some(5));
    // ...and a plan of another length replaces it, never adds to it.
    let mut t = Tracker::new(Vec::new());
    t.expect_total(5);
    t.on_plan("update-media", &steps[..3]);
    assert_eq!(t.planned("update-media").unwrap().m, Some(3));
    // A fixed total (several plans to come) is only confirmed.
    let mut t = Tracker::new(Vec::new());
    t.set_total(8);
    t.on_plan("a", &steps[..3]);
    t.on_plan("b", &steps[..5]);
    assert_eq!(t.planned("a").unwrap().m, Some(8));
}
