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
        Needs::HostRelease => Material::HostRelease {
            tag: "v3.63.0".into(),
            binary_b64: "QUJD".into(),
        },
        Needs::NativeRelease => {
            let mut n = native("admin");
            n.stack_name = stack.into();
            Material::NativeRelease {
                manifest: Box::new(n),
                unit_file: "[Service]\n".into(),
                tag: "v1.2.3".into(),
                dir: format!("stacks/{stack}"),
            }
        }
        Needs::Apply => Material::Apply {
            deploy: vec![spec("home")],
            destroy: vec!["gone".into()],
        },
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
    if kind.args().contains(&actions::Arg::Vmid) {
        a.vmid = Some("994".into());
    }
    match kind {
        ActionKind::Exec => a.command = Some("df -h".into()),
        ActionKind::TemplateBuild => a.version = Some("5".into()),
        ActionKind::AnswerCheck => {
            a.check = Some("c4bca102".into());
            a.verdict = Some("ok".into());
        }
        ActionKind::Apply => a.destroy = Some("gone".into()),
        _ => {}
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

/// TUI parity round: the new actions send exactly the commands the CLI
/// verbs send (exec, guards on any vmid, template-build, checks answer,
/// release-update, install-native, apply).
#[test]
fn parity_the_new_actions_send_the_cli_commands() {
    let host = |action: &str, args: ActionArgs| {
        let req = validate(HOST_TARGET, action, args).unwrap_or_else(|r| panic!("{action}: {r}"));
        commands(&req, Material::None).unwrap_or_else(|r| panic!("{action}: {r}"))
    };
    assert!(matches!(
        host("exec", ActionArgs { vmid: Some("105".into()), command: Some(" df -h ".into()), ..Default::default() }).as_slice(),
        [Command::ExecIn { vmid: 105, command }] if command == "df -h"
    ));
    assert!(matches!(
        host(
            "guards-ct",
            ActionArgs {
                vmid: Some("111".into()),
                ..Default::default()
            }
        )
        .as_slice(),
        [Command::ApplyGuards { vmid: 111 }]
    ));
    assert!(matches!(
        host("template-build", ActionArgs {
            vmid: Some("994".into()), version: Some("5".into()), privileged: true,
            base: Some("local:vztmpl/debian-13-standard_13.1-2_amd64.tar.zst".into()),
            ..Default::default()
        }).as_slice(),
        [Command::BuildTemplate { temp_vmid: 994, version: 5, unprivileged: false, base_template: Some(b) }]
            if b.starts_with("local:vztmpl/debian-13")
    ));
    assert!(matches!(
        host("answer-check", ActionArgs {
            check: Some("c4bca102".into()), verdict: Some("accept".into()),
            days: Some("30".into()), note: Some("known, fixed next month".into()),
            ..Default::default()
        }).as_slice(),
        [Command::AnswerManualCheck { ok: false, accept_days: Some(30), note, .. }] if note.starts_with("known")
    ));
    assert!(matches!(
        host(
            "answer-check",
            ActionArgs {
                check: Some("c4bca102".into()),
                verdict: Some("ok".into()),
                ..Default::default()
            }
        )
        .as_slice(),
        [Command::AnswerManualCheck {
            ok: true,
            accept_days: None,
            ..
        }]
    ));
    // The host update ships the verified binary; the CLI line names the tag.
    let req = validate(HOST_TARGET, "update-host", ActionArgs::default()).unwrap();
    let m = material_for(ActionKind::UpdateHost, HOST_TARGET);
    assert_eq!(
        actions::cli_override(&req, &m).as_deref(),
        Some("homelab release-update v3.63.0")
    );
    assert!(matches!(
        commands(&req, m).unwrap().as_slice(),
        [Command::SelfUpdateHost { binary_b64 }] if binary_b64 == "QUJD"
    ));
    // install-native: the host downloads; the CLI line is the verb's own.
    let req = validate(
        "admin",
        "install-native",
        ActionArgs {
            tag: Some("v1.2.3".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let m = material_for(ActionKind::InstallNative, "admin");
    assert_eq!(
        actions::cli_override(&req, &m).as_deref(),
        Some("homelab install-native stacks/admin v1.2.3")
    );
    assert!(matches!(
        commands(&req, m).unwrap().as_slice(),
        [Command::InstallNativeRelease { tag, .. }] if tag == "v1.2.3"
    ));
    assert!(restarts_dashboard("admin", ActionKind::InstallNative));
}

/// Security (dash-exec): exec needs the all scope and no typed name; the
/// host still refuses unless exec_enabled. A command is one line.
#[test]
fn parity_exec_is_all_scope_one_line_and_no_confirmation() {
    let e = ActionKind::from_slug("exec").unwrap();
    assert_eq!(e.scope(), homelab_proto::Scope::All);
    assert!(!e.confirm(), "Kenny: no confirmation");
    assert!(e.host_wide());
    for (vmid, cmd, why) in [
        (None, Some("ls"), "container's number"),
        (Some("abc"), Some("ls"), "not a container number"),
        (Some("99"), Some("ls"), "not a container number"),
        (Some("105"), None, "needs a command"),
        (Some("105"), Some("   "), "needs a command"),
        (Some("105"), Some("ls\nrm -rf /"), "one line"),
    ] {
        let r = validate(
            HOST_TARGET,
            "exec",
            ActionArgs {
                vmid: vmid.map(str::to_string),
                command: cmd.map(str::to_string),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(r.why.contains(why), "{vmid:?} {cmd:?}: {r}");
    }
    // Only as a host-wide action, never on a stack.
    assert!(validate("home", "exec", ActionArgs::default()).is_err());
}

/// Security (dash-apply): destroying a gone stack needs its typed name,
/// never admin's, never twice, never a name that is not gone.
#[test]
fn parity_apply_destroys_only_typed_gone_stacks() {
    use homelab_admin::core::applyview::{chosen_destroys, plan};
    let ok = |h: &str| Ok::<String, String>(h.into());
    let view = plan(
        &[
            ("home".into(), ok("h1")),
            ("media".into(), ok("m2")),
            ("broken".into(), Err("latch failed".into())),
        ],
        &["home".into(), "media".into(), "broken".into()],
        &[
            ("home".into(), "h1".into()),
            ("media".into(), "m1".into()),
            ("drill".into(), "d1".into()),
            ("old".into(), "o1".into()),
        ],
        &[],
    );
    assert_eq!(view.deploy, vec!["media"]);
    assert_eq!(view.unchanged, vec!["home"]);
    assert_eq!(view.destroy, vec!["drill", "old"]);
    assert_eq!(view.broken.len(), 1);
    // Nothing typed: nothing destroyed.
    assert!(chosen_destroys(&view, &[]).unwrap().is_empty());
    // One typed: that one only.
    assert_eq!(
        chosen_destroys(&view, &["old".into()]).unwrap(),
        vec!["old"]
    );
    // A stack the files still declare is never destroyed by apply.
    assert!(chosen_destroys(&view, &["media".into()]).is_err());
    // The form refuses admin and a name typed twice before anything runs.
    for (typed, why) in [
        ("admin", "arch-self"),
        ("drill, drill", "twice"),
        ("Drill", "not a stack name"),
    ] {
        let r = validate(
            HOST_TARGET,
            "apply",
            ActionArgs {
                destroy: Some(typed.into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(r.why.contains(why), "{typed}: {r}");
    }
    let req = validate(
        HOST_TARGET,
        "apply",
        ActionArgs {
            destroy: Some("drill".into()),
            skip_backup: true,
            ..Default::default()
        },
    )
    .unwrap();
    let cmds = commands(
        &req,
        Material::Apply {
            deploy: vec![spec("home")],
            destroy: vec!["drill".into()],
        },
    )
    .unwrap();
    assert!(matches!(cmds.first(), Some(Command::DeployStack(_))));
    assert!(matches!(
        cmds.last(),
        Some(Command::DestroyRecorded { stack, confirm, skip_backup: true }) if stack == "drill" && confirm == "drill"
    ));
    assert_eq!(
        actions::cli_override(&req, &Material::None).as_deref(),
        Some("homelab apply --yes --no-backup")
    );
}

/// The answer's rules are the CLI's: accept needs days and a reason.
#[test]
fn parity_a_check_answer_follows_the_cli_rules() {
    let a = |verdict: &str, days: Option<&str>, note: Option<&str>| ActionArgs {
        check: Some("c4bca102".into()),
        verdict: Some(verdict.into()),
        days: days.map(str::to_string),
        note: note.map(str::to_string),
        ..Default::default()
    };
    assert!(validate(HOST_TARGET, "answer-check", a("ok", None, None)).is_ok());
    assert!(validate(
        HOST_TARGET,
        "answer-check",
        a("nok", None, Some("broken again"))
    )
    .is_ok());
    for (args, why) in [
        (a("maybe", None, None), "ok, nok or accept"),
        (a("accept", None, Some("reason")), "number of days"),
        (a("accept", Some("0"), Some("reason")), "number of days"),
        (a("accept", Some("30"), None), "reason"),
        (a("ok", Some("3"), None), "days go with accept"),
    ] {
        let r = validate(HOST_TARGET, "answer-check", args).unwrap_err();
        assert!(r.why.contains(why), "{r}");
    }
}
