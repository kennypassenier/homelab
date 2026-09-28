//! feat-stacks-4, feat-stacks-5, feat-stacks-6: the action catalog, the
//! checks before anything runs, and the exact commands the CLI would send.

mod act_support;

use act_support::{manifest, native, spec};
use homelab_admin::core::actions::{
    self, catalog, commands, restarts_dashboard, validate, validate_batch, ActionArgs, ActionKind,
    BatchRequest, Material, HOST_TARGET,
};
use homelab_proto::Command;

fn material_for(kind: ActionKind, stack: &str) -> Material {
    use homelab_admin::core::actions::Needs;
    match kind.needs() {
        Needs::Nothing => Material::None,
        Needs::Vmid => Material::Vmid(104),
        Needs::Manifest => {
            let mut m = manifest("home");
            m.stack_name = stack.into();
            Material::Manifest(Box::new(m))
        }
        Needs::Spec => {
            let mut s = spec("home");
            s.manifest.stack_name = stack.into();
            Material::Spec(Box::new(s))
        }
        Needs::NativeManifest => {
            let mut n = native("admin");
            n.stack_name = stack.into();
            Material::Native(Box::new(n))
        }
    }
}

fn args_for(kind: ActionKind, stack: &str) -> ActionArgs {
    let mut a = ActionArgs::default();
    if kind.confirm() {
        a.confirm = Some(stack.into());
    }
    if kind == ActionKind::DeployCommit {
        a.commit = Some("a1b2c3d4e5f6".into());
    }
    a
}

#[test]
fn feat_stacks_4_every_action_builds_a_command_whose_scope_the_catalog_names() {
    for kind in ActionKind::ALL {
        let stack = if kind.host_wide() {
            HOST_TARGET
        } else {
            "home"
        };
        let req = validate(stack, kind.slug(), args_for(*kind, stack))
            .unwrap_or_else(|r| panic!("{:?}: {r}", kind));
        let cmds = commands(&req, material_for(*kind, stack))
            .unwrap_or_else(|r| panic!("{:?}: {r}", kind));
        let main = cmds.last().expect("a command");
        assert_eq!(kind.scope(), main.scope(), "{:?} → {}", kind, main.name());
        assert_eq!(ActionKind::from_slug(kind.slug()), Some(*kind));
    }
    let c = catalog();
    assert_eq!(c.len(), ActionKind::ALL.len());
    let json = serde_json::to_value(&c).unwrap();
    assert_eq!(json[0]["action"], "deploy");
    assert_eq!(json[0]["args"][0], "force");
}

#[test]
fn feat_stacks_4_the_commands_are_the_ones_the_cli_sends() {
    let req = validate(
        "home",
        "restore",
        ActionArgs {
            confirm: Some("home".into()),
            app: Some("homepage".into()),
            ..Default::default()
        },
    )
    .unwrap();
    match commands(&req, material_for(ActionKind::Restore, "home"))
        .unwrap()
        .remove(0)
    {
        Command::RestoreStack {
            snapshot,
            confirm,
            app,
            skip_safety_copy,
            manifest,
        } => {
            assert_eq!(
                snapshot, "latest",
                "no snapshot named is latest, as in the CLI"
            );
            assert_eq!(confirm.as_deref(), Some("home"));
            assert_eq!(app.as_deref(), Some("homepage"));
            assert!(!skip_safety_copy);
            assert_eq!(manifest.stack_name, "home");
        }
        other => panic!("{:?}", other.name()),
    }
    let req = validate(
        "kyu",
        "rollback-native",
        ActionArgs {
            unit: Some("kyu-runner".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        commands(&req, Material::None).unwrap().as_slice(),
        [Command::RollbackNative { stack, unit: Some(u) }] if stack == "kyu" && u == "kyu-runner"
    ));
    let req = validate("home", "disable", ActionArgs::default()).unwrap();
    assert!(matches!(
        commands(&req, Material::None).unwrap().as_slice(),
        [Command::SetStackEnabled { enabled: false, .. }]
    ));
}

#[test]
fn feat_stacks_4_a_deploy_stages_each_binary_first_then_sends_the_spec_without_them() {
    let mut s = spec("admin");
    s.native_binaries.insert("admin".into(), "QUJD".into());
    s.native_binaries.insert("other".into(), String::new());
    let cmds = actions::deploy_commands(s);
    assert_eq!(cmds.len(), 2);
    assert!(
        matches!(&cmds[0], Command::StageNativeBinary { unit, binary_b64, stack } if unit == "admin" && binary_b64 == "QUJD" && stack == "admin")
    );
    match &cmds[1] {
        Command::DeployStack(spec) => {
            assert!(spec.native_binaries.values().all(|b| b.is_empty()));
            assert_eq!(spec.native_binaries.len(), 2);
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn feat_stacks_4_a_destroy_without_a_directory_goes_by_the_hosts_record() {
    let req = validate(
        "drill",
        "destroy",
        ActionArgs {
            confirm: Some("drill".into()),
            skip_backup: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        commands(&req, Material::None).unwrap().as_slice(),
        [Command::DestroyRecorded { stack, confirm, skip_backup: true }] if stack == "drill" && confirm == "drill"
    ));
}

#[test]
fn feat_stacks_4_stack_files_that_name_another_stack_are_refused() {
    let req = validate("home", "backup", ActionArgs::default()).unwrap();
    let r = commands(&req, material_for(ActionKind::Backup, "media")).unwrap_err();
    assert!(r.why.contains("media"), "{r}");
}

#[test]
fn feat_stacks_4_every_refusal_says_what_why_and_fix() {
    let cases: Vec<(&str, &str, ActionArgs, &str)> = vec![
        ("home", "reboot", ActionArgs::default(), "no such action"),
        ("home", "patch", ActionArgs::default(), "whole host"),
        ("Home!", "backup", ActionArgs::default(), "lowercase"),
        (
            "home",
            "backup",
            ActionArgs {
                force: true,
                ..Default::default()
            },
            "does not take force",
        ),
        ("home", "restore", ActionArgs::default(), "typed name"),
        (
            "home",
            "destroy",
            ActionArgs {
                confirm: Some("hom".into()),
                ..Default::default()
            },
            "typed name",
        ),
        (
            "home",
            "deploy-commit",
            ActionArgs::default(),
            "needs the commit",
        ),
        (
            "home",
            "restore",
            ActionArgs {
                confirm: Some("home".into()),
                snapshot: Some("x; rm -rf /".into()),
                ..Default::default()
            },
            "not a snapshot",
        ),
    ];
    for (stack, action, args, why) in cases {
        let r = validate(stack, action, args).unwrap_err();
        assert!(r.why.contains(why), "{stack} {action}: {r}");
        assert!(!r.what.is_empty() && !r.fix.is_empty());
        let json = serde_json::to_value(&r).unwrap();
        for k in ["what", "why", "fix"] {
            assert!(json[k].is_string(), "arch-errors: {k} in {json}");
        }
    }
}

#[test]
fn arch_self_the_dashboard_never_destroys_forgets_or_wipes_itself() {
    for action in ["destroy", "forget", "wipe"] {
        let r = validate(
            "admin",
            action,
            ActionArgs {
                confirm: Some("admin".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(r.why.contains("arch-self"), "{action}: {r}");
    }
    // A deploy of itself is allowed and announced as a restart.
    assert!(validate("admin", "deploy", ActionArgs::default()).is_ok());
    assert!(restarts_dashboard("admin", ActionKind::Deploy));
    assert!(!restarts_dashboard("admin", ActionKind::Backup));
    assert!(!restarts_dashboard("home", ActionKind::Deploy));
}

#[test]
fn feat_stacks_4_a_wipe_without_a_name_only_lists() {
    let req = validate("drill", "wipe", ActionArgs::default()).unwrap();
    assert!(matches!(
        commands(&req, Material::None).unwrap().as_slice(),
        [Command::WipeRetired { confirm: None, .. }]
    ));
}

#[test]
fn feat_stacks_5_a_batch_is_checked_whole_before_anything_runs() {
    let ok = validate_batch(BatchRequest {
        action: "backup".into(),
        stacks: vec!["home".into(), "media".into()],
        args: ActionArgs::default(),
        confirms: Default::default(),
    })
    .unwrap();
    assert_eq!(
        ok.iter().map(|r| r.stack.as_str()).collect::<Vec<_>>(),
        vec!["home", "media"],
        "the order given is the order run"
    );
    let twice = validate_batch(BatchRequest {
        action: "backup".into(),
        stacks: vec!["home".into(), "home".into()],
        args: ActionArgs::default(),
        confirms: Default::default(),
    })
    .unwrap_err();
    assert!(twice.why.contains("twice"));
    let one_bad = validate_batch(BatchRequest {
        action: "restore".into(),
        stacks: vec!["home".into(), "media".into()],
        args: ActionArgs::default(),
        confirms: [("home".to_string(), "home".to_string())].into(),
    })
    .unwrap_err();
    assert!(one_bad.what.contains("media"), "{one_bad}");
    assert!(validate_batch(BatchRequest {
        action: "patch".into(),
        stacks: vec![HOST_TARGET.into()],
        args: ActionArgs::default(),
        confirms: Default::default(),
    })
    .is_err());
    assert!(validate_batch(BatchRequest {
        action: "backup".into(),
        stacks: vec![],
        args: ActionArgs::default(),
        confirms: Default::default(),
    })
    .is_err());
}

#[test]
fn feat_stacks_4_arguments_travel_as_documented_json() {
    let a: ActionArgs =
        serde_json::from_str(r#"{"confirm":"home","snapshot":"abc123","skip_safety_copy":true}"#)
            .unwrap();
    assert_eq!(a.snapshot.as_deref(), Some("abc123"));
    assert!(serde_json::from_str::<ActionArgs>(r#"{"forse":true}"#).is_err());
}
