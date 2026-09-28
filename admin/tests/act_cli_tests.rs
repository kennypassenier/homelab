//! feat-stacks-7: "copy as CLI command" gives a line the CLI's own parser
//! (`homelab_client::cli_args::parse`, which `main` uses) reads back as the
//! same command.

mod act_support;

use act_support::{manifest, native, spec};
use homelab_admin::core::actions_cli::cli_line;
use homelab_client::cli_args::{parse, split, Invocation};
use homelab_client::repo_config::stack_name;
use homelab_proto::Command;

fn back(line: &str) -> Invocation {
    let words = split(line);
    assert_eq!(words[0], "homelab", "{line}");
    assert!(
        homelab_client::cli_help::is_verb(&words[1]),
        "{} is a verb the CLI knows",
        words[1]
    );
    parse(&words[1..])
        .unwrap_or_else(|e| panic!("{line}: {e}"))
        .unwrap_or_else(|| panic!("{line}: not a parsed verb"))
}

fn named(stack: &str) -> homelab_proto::StackManifest {
    let mut m = manifest("home");
    m.stack_name = stack.into();
    m
}

#[test]
fn feat_stacks_7_every_stack_action_round_trips_through_the_cli_parser() {
    let mut s = spec("home");
    s.manifest.stack_name = "media".into();
    let cases: Vec<(Command, bool, Invocation)> = vec![
        (
            Command::DeployStack(Box::new(s.clone())),
            false,
            Invocation::Deploy {
                stack: "media".into(),
                force: false,
            },
        ),
        (
            Command::DeployStack(Box::new(s.clone())),
            true,
            Invocation::Deploy {
                stack: "media".into(),
                force: true,
            },
        ),
        (
            Command::BackupStack(Box::new(named("media"))),
            false,
            Invocation::Backup {
                stack: "media".into(),
            },
        ),
        (
            Command::UpdateStack {
                manifest: Box::new(named("media")),
                app: Some("jellyfin".into()),
            },
            false,
            Invocation::Update {
                stack: "media".into(),
                app: Some("jellyfin".into()),
            },
        ),
        (
            Command::UpdateStack {
                manifest: Box::new(named("media")),
                app: None,
            },
            false,
            Invocation::Update {
                stack: "media".into(),
                app: None,
            },
        ),
        (
            Command::ApplyResources(Box::new(named("media"))),
            false,
            Invocation::Resize {
                stack: "media".into(),
            },
        ),
        (
            Command::SetStackEnabled {
                stack: "media".into(),
                enabled: false,
            },
            false,
            Invocation::Enable {
                stack: "media".into(),
                enabled: false,
            },
        ),
        (
            Command::SetStackEnabled {
                stack: "media".into(),
                enabled: true,
            },
            false,
            Invocation::Enable {
                stack: "media".into(),
                enabled: true,
            },
        ),
        (
            Command::AdoptService(Box::new(native("admin"))),
            false,
            Invocation::Adopt {
                stack: "admin".into(),
            },
        ),
        (
            Command::BackupNative {
                stack: "kyu".into(),
            },
            false,
            Invocation::BackupNative {
                stack: "kyu".into(),
            },
        ),
        (
            Command::UpdateNative {
                stack: "kyu".into(),
            },
            false,
            Invocation::UpdateNative {
                stack: "kyu".into(),
            },
        ),
        (
            Command::ReleaseUpdateNative {
                stack: "kyu".into(),
            },
            false,
            Invocation::ReleaseUpdateNative {
                stack: "kyu".into(),
            },
        ),
        (
            Command::RollbackNative {
                stack: "kyu".into(),
                unit: Some("kyu-runner".into()),
            },
            false,
            Invocation::RollbackNative {
                stack: "kyu".into(),
                unit: Some("kyu-runner".into()),
            },
        ),
        (
            Command::RollbackNative {
                stack: "almanac".into(),
                unit: None,
            },
            false,
            Invocation::RollbackNative {
                stack: "almanac".into(),
                unit: None,
            },
        ),
        (
            Command::ApplyGuards { vmid: 104 },
            false,
            Invocation::Guards { vmid: 104 },
        ),
        (
            Command::ForgetStack {
                stack: "drill".into(),
            },
            false,
            Invocation::Forget {
                stack: "drill".into(),
            },
        ),
        (
            Command::DestroyStack {
                manifest: Box::new(named("drill")),
                confirm: "drill".into(),
                skip_backup: true,
            },
            false,
            Invocation::Destroy {
                stack: "drill".into(),
                skip_backup: true,
                yes: false,
            },
        ),
        (
            Command::DestroyRecorded {
                stack: "drill".into(),
                confirm: "drill".into(),
                skip_backup: false,
            },
            false,
            Invocation::Destroy {
                stack: "drill".into(),
                skip_backup: false,
                yes: false,
            },
        ),
        (
            Command::WipeRetired {
                name: "drill".into(),
                confirm: Some("drill".into()),
            },
            false,
            Invocation::Wipe {
                name: "drill".into(),
                yes: false,
            },
        ),
        (
            Command::PruneOrphans {
                manifest: Box::new(named("media")),
                spec: Box::new(s.clone()),
                confirm: "media".into(),
            },
            false,
            Invocation::PruneOrphans {
                stack: "media".into(),
                yes: false,
            },
        ),
        (Command::PatchFleet, false, Invocation::Patch),
        (Command::ZfsReplicate, false, Invocation::ZfsReplicate),
        (Command::BackupHostMeta, false, Invocation::BackupHostMeta),
        (Command::BackupDevices, false, Invocation::BackupDevices),
    ];
    for (command, force, want) in cases {
        let line = cli_line(&command, force).unwrap_or_else(|| panic!("{}", command.name()));
        let got = back(&line);
        assert_eq!(got, want, "{line}");
        // The stack as the CLI resolves it names the command's stack.
        if let Invocation::Backup { stack } | Invocation::Deploy { stack, .. } = &got {
            assert_eq!(stack_name(stack), "media");
        }
    }
}

#[test]
fn feat_stacks_7_a_restore_round_trips_with_its_snapshot_app_and_flags() {
    let command = Command::RestoreStack {
        manifest: Box::new(named("paperwork")),
        snapshot: "4f2a9c1e".into(),
        confirm: Some("paperwork".into()),
        skip_safety_copy: true,
        app: Some("paperless".into()),
    };
    let line = cli_line(&command, false).unwrap();
    assert_eq!(
        line,
        "homelab restore paperwork 4f2a9c1e --app paperless --no-safety-copy"
    );
    match back(&line) {
        Invocation::Restore(r) => {
            assert_eq!(
                (r.dir.as_str(), r.snapshot.as_str(), r.app.as_deref()),
                ("paperwork", "4f2a9c1e", Some("paperless"))
            );
            assert!(r.skip_safety_copy);
            assert!(!r.yes, "the CLI still asks for the typed name");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn feat_stacks_7_a_word_with_spaces_is_quoted_and_comes_back_whole() {
    let line = cli_line(
        &Command::AnswerManualCheck {
            check_id: "3f2a9c1e".into(),
            ok: false,
            note: "posters missing again".into(),
            accept_days: None,
        },
        false,
    )
    .unwrap();
    assert_eq!(
        line,
        "homelab checks answer 3f2a9c1e nok 'posters missing again'"
    );
    assert_eq!(
        split(&line),
        vec![
            "homelab",
            "checks",
            "answer",
            "3f2a9c1e",
            "nok",
            "posters missing again"
        ]
    );
    // A word with a quote in it is not offered as a line at all.
    assert!(cli_line(
        &Command::ForgetStack {
            stack: "it's".into()
        },
        false
    )
    .is_none());
}

#[test]
fn feat_stacks_7_commands_no_verb_sends_have_no_line() {
    for c in [
        Command::GetState,
        Command::SelfUpdateHost {
            binary_b64: String::new(),
        },
        Command::StageNativeBinary {
            stack: "kyu".into(),
            unit: "kyu".into(),
            binary_b64: String::new(),
        },
        Command::CurrentOp,
    ] {
        assert!(cli_line(&c, false).is_none(), "{}", c.name());
    }
}

#[test]
fn feat_stacks_7_the_cli_parser_keeps_the_verbs_old_rules() {
    let p = |s: &str| parse(&split(s)).unwrap().unwrap();
    // Flags anywhere, a path or a name.
    // TUI parity round: a flag before the stack no longer becomes the
    // stack (`deploy --force stacks/x` read --force as the stack's name).
    assert_eq!(
        p("deploy --force stacks/media"),
        Invocation::Deploy {
            stack: "stacks/media".into(),
            force: true
        },
    );
    assert_eq!(
        p("deploy stacks/media --force"),
        Invocation::Deploy {
            stack: "stacks/media".into(),
            force: true
        },
    );
    assert_eq!(
        p("destroy stacks/drill --no-backup"),
        Invocation::Destroy {
            stack: "stacks/drill".into(),
            skip_backup: true,
            yes: false,
        }
    );
    assert_eq!(
        p("destroy --yes --no-backup drill"),
        Invocation::Destroy {
            stack: "drill".into(),
            skip_backup: true,
            yes: true,
        }
    );
    assert_eq!(
        p("update --force stacks/media sonarr"),
        Invocation::Update {
            stack: "stacks/media".into(),
            app: Some("sonarr".into()),
        }
    );
    assert!(parse(&split("deploy --force"))
        .unwrap_err()
        .contains("usage"));
    assert!(parse(&split("guards abc")).is_err());
    assert!(parse(&split("wipe --yes")).is_err());
    assert!(parse(&split("backup")).unwrap_err().contains("usage"));
    assert_eq!(parse(&split("tui")).unwrap(), None);
}
