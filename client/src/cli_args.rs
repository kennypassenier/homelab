//! feat-stacks-7 (homelab-admin, milestone act): the command line of the
//! verbs that act on one stack, or on the host as a whole, parsed in one
//! place. `main` takes its arguments from here, and the dashboard's "copy as
//! CLI command" is tested against the same function, so a line the
//! dashboard offers is a line this CLI reads the same way.
//!
//! The rules are the ones the verbs always had: the stack is the word after
//! the verb (a name or a path, `stack_dir`/`stack_name` resolve both); flags
//! may stand anywhere; `restore` is `restore_args` (fix-64, fix-112) and
//! `rollback-native` is `stack_and_unit` (fix-114).

use crate::RestoreArgs;

/// One parsed command line. A stack is kept as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    Deploy {
        stack: String,
        force: bool,
    },
    Backup {
        stack: String,
    },
    Restore(RestoreArgs),
    Update {
        stack: String,
        app: Option<String>,
    },
    Resize {
        stack: String,
    },
    Enable {
        stack: String,
        enabled: bool,
    },
    Adopt {
        stack: String,
    },
    BackupNative {
        stack: String,
    },
    UpdateNative {
        stack: String,
    },
    ReleaseUpdateNative {
        stack: String,
    },
    RollbackNative {
        stack: String,
        unit: Option<String>,
    },
    /// fix-223: restore a native (adopted) service from a snapshot; `unit`
    /// restricts it to one service of a multi-unit stack, same
    /// `<stack>/<unit>` shape `rollback-native` already uses.
    RestoreNative {
        stack: String,
        snapshot: String,
        unit: Option<String>,
        yes: bool,
    },
    Guards {
        vmid: u16,
    },
    Forget {
        stack: String,
    },
    /// `yes`: the name was typed where the line came from (the dashboard's
    /// form, decision cli-yes); the CLI then does not ask for it again.
    Destroy {
        stack: String,
        skip_backup: bool,
        yes: bool,
    },
    Wipe {
        name: String,
        yes: bool,
    },
    PruneOrphans {
        stack: String,
        yes: bool,
    },
    Patch,
    ZfsReplicate,
    BackupHostMeta,
    BackupDevices,
}

fn usage(verb: &str) -> String {
    match verb {
        "deploy" => "usage: homelab deploy stacks/<name>".into(),
        "backup" => "usage: homelab backup stacks/<name>".into(),
        "update" => "usage: homelab update stacks/<name> [app]".into(),
        "resize" => "usage: homelab resize stacks/<name>".into(),
        "enable" | "disable" => "usage: homelab enable|disable <stack-name>".into(),
        "adopt" => "usage: homelab adopt stacks/<name>".into(),
        "backup-native" => "usage: homelab backup-native <stack>".into(),
        "update-native" => "usage: homelab update-native <stack>".into(),
        "release-update-native" => "usage: homelab release-update-native <stack>".into(),
        "rollback-native" => "usage: homelab rollback-native <stack>[/<unit>]".into(),
        "restore-native" => {
            "usage: homelab restore-native <stack>[/<unit>] [snapshot] [--yes]".into()
        }
        "guards" => "usage: homelab guards <vmid>".into(),
        "forget" => "usage: homelab forget <stack>".into(),
        "destroy" => "usage: homelab destroy stacks/<name>".into(),
        "wipe" => "usage: homelab wipe <stack> | <stack>/<app>".into(),
        "prune-orphans" => "usage: homelab prune-orphans stacks/<name>".into(),
        other => format!("usage: homelab {}", other),
    }
}

/// `args` is the command line after the program name: the verb first.
/// `Ok(None)` for a verb this module does not model (`tui`, `check`, ...).
pub fn parse(args: &[String]) -> Result<Option<Invocation>, String> {
    let Some(verb) = args.first() else {
        return Ok(None);
    };
    let verb = verb.as_str();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    // The words after the verb that are not flags: flags may stand anywhere,
    // so `deploy --force stacks/x` names the stack `stacks/x`, not
    // `--force` (it did, until the TUI parity round).
    let words: Vec<&String> = args[1..].iter().filter(|a| !a.starts_with("--")).collect();
    let first = || {
        words
            .first()
            .map(|w| w.to_string())
            .ok_or_else(|| usage(verb))
    };
    let first_word = first;
    Ok(Some(match verb {
        "deploy" => Invocation::Deploy {
            stack: first()?,
            force: has("--force"),
        },
        "backup" => Invocation::Backup { stack: first()? },
        "restore" => Invocation::Restore(crate::restore_args(args.get(1..).unwrap_or(&[]))?),
        "update" => Invocation::Update {
            stack: first()?,
            app: words.get(1).map(|w| w.to_string()),
        },
        "resize" => Invocation::Resize { stack: first()? },
        "enable" | "disable" => Invocation::Enable {
            stack: first()?,
            enabled: verb == "enable",
        },
        "adopt" => Invocation::Adopt { stack: first()? },
        "backup-native" => Invocation::BackupNative { stack: first()? },
        "update-native" => Invocation::UpdateNative { stack: first()? },
        "release-update-native" => Invocation::ReleaseUpdateNative { stack: first()? },
        "rollback-native" => {
            let (stack, unit) = crate::stack_and_unit(&first()?);
            Invocation::RollbackNative { stack, unit }
        }
        "restore-native" => {
            let (stack, unit) = crate::stack_and_unit(&first()?);
            Invocation::RestoreNative {
                stack,
                snapshot: words
                    .get(1)
                    .map(|w| w.to_string())
                    .unwrap_or_else(|| "latest".into()),
                unit,
                yes: has("--yes"),
            }
        }
        "guards" => Invocation::Guards {
            vmid: words
                .first()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| usage(verb))?,
        },
        "forget" => Invocation::Forget { stack: first()? },
        "destroy" => Invocation::Destroy {
            stack: first()?,
            skip_backup: has("--no-backup"),
            yes: has("--yes"),
        },
        "wipe" => Invocation::Wipe {
            name: first_word()?,
            yes: has("--yes"),
        },
        "prune-orphans" => Invocation::PruneOrphans {
            stack: first()?,
            yes: has("--yes"),
        },
        "patch" => Invocation::Patch,
        "zfs-replicate" => Invocation::ZfsReplicate,
        "backup-host-meta" => Invocation::BackupHostMeta,
        "backup-devices" => Invocation::BackupDevices,
        _ => return Ok(None),
    }))
}

/// A command line as a shell would split it: words separated by spaces,
/// single quotes keep a word whole (the only quoting `cli_line` writes).
pub fn split(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                in_word = true;
            }
            ' ' if !quoted => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod fix_223_tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    /// fix-223: `restore-native kyu/kyu-runner` restores that one unit;
    /// a bare stack keeps restoring every unit, as before.
    #[test]
    fn fix_223_restore_native_takes_an_optional_unit() {
        assert_eq!(
            parse(&args("restore-native kyu/kyu-runner 5ebbc732 --yes")).unwrap(),
            Some(Invocation::RestoreNative {
                stack: "kyu".into(),
                snapshot: "5ebbc732".into(),
                unit: Some("kyu-runner".into()),
                yes: true,
            })
        );
        assert_eq!(
            parse(&args("restore-native kyu")).unwrap(),
            Some(Invocation::RestoreNative {
                stack: "kyu".into(),
                snapshot: "latest".into(),
                unit: None,
                yes: false,
            })
        );
    }
}
