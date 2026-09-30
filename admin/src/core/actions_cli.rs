//! feat-stacks-7: "copy as CLI command". The exact `homelab …` line that
//! sends the same command from a workstation, built from the `Command`
//! itself so the two cannot drift; tested against the CLI's own parser
//! (`homelab_client::cli_args::parse`).
//!
//! cli-yes (Kenny, 2026-09-28, "With --yes once the name was typed"): a
//! restore, destroy, wipe or prune-orphans line carries `--yes` once the
//! form's typed name matches ([`cli_line_typed`]); before that, the line
//! leaves the confirmation to the CLI, which asks for the name when it is
//! run.

use homelab_proto::Command;

/// A word as the shell must see it: bare when it is plain, else in single
/// quotes (no word the dashboard sends contains a quote; one that does is
/// refused rather than escaped).
fn word(w: &str) -> Option<String> {
    if w.contains('\'') || w.is_empty() {
        return None;
    }
    let plain = w
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/' | b':' | b'@'));
    Some(if plain {
        w.to_string()
    } else {
        format!("'{w}'")
    })
}

/// The line, or None for a command no CLI verb sends (a binary upload, a
/// read the CLI never offers, an answer the TUI gives interactively).
/// `force` is the deploy guard's override, which is a flag of the verb and
/// not part of the command.
pub fn cli_line(command: &Command, force: bool) -> Option<String> {
    cli_line_typed(command, force, false)
}

/// Whether the command carries the typed confirmation its verb would ask
/// for again (restore, destroy, a real wipe, prune-orphans).
fn carries_confirmation(command: &Command) -> bool {
    use Command::*;
    match command {
        RestoreStack {
            manifest, confirm, ..
        } => confirm.as_deref() == Some(manifest.stack_name.as_str()),
        DestroyStack {
            manifest, confirm, ..
        } => confirm == &manifest.stack_name,
        DestroyRecorded { stack, confirm, .. } => confirm == stack,
        WipeRetired { name, confirm } => confirm.as_deref() == Some(name.as_str()),
        PruneOrphans {
            manifest, confirm, ..
        } => confirm == &manifest.stack_name,
        _ => false,
    }
}

/// `cli_line`, with `--yes` added when `typed` (the form's typed name
/// matched) and the command carries that confirmation (cli-yes).
pub fn cli_line_typed(command: &Command, force: bool, typed: bool) -> Option<String> {
    let line = cli_line_bare(command, force)?;
    Some(if typed && carries_confirmation(command) {
        format!("{line} --yes")
    } else {
        line
    })
}

fn cli_line_bare(command: &Command, force: bool) -> Option<String> {
    use Command::*;
    let mut parts: Vec<String> = vec!["homelab".into()];
    let mut push = |w: &str| -> Option<()> {
        parts.push(word(w)?);
        Some(())
    };
    match command {
        DeployStack(spec) => {
            push("deploy")?;
            push(&spec.manifest.stack_name)?;
            if force {
                push("--force")?;
            }
        }
        BackupStack(m) => {
            push("backup")?;
            push(&m.stack_name)?;
        }
        RestoreStack {
            manifest,
            snapshot,
            skip_safety_copy,
            app,
            ..
        } => {
            push("restore")?;
            push(&manifest.stack_name)?;
            push(snapshot)?;
            if let Some(app) = app {
                push("--app")?;
                push(app)?;
            }
            if *skip_safety_copy {
                push("--no-safety-copy")?;
            }
        }
        UpdateStack { manifest, app } => {
            push("update")?;
            push(&manifest.stack_name)?;
            if let Some(app) = app {
                push(app)?;
            }
        }
        ApplyResources(m) => {
            push("resize")?;
            push(&m.stack_name)?;
        }
        SetStackEnabled { stack, enabled } => {
            push(if *enabled { "enable" } else { "disable" })?;
            push(stack)?;
        }
        AdoptService(m) => {
            push("adopt")?;
            push(&m.stack_name)?;
        }
        BackupNative { stack } => {
            push("backup-native")?;
            push(stack)?;
        }
        UpdateNative { stack } => {
            push("update-native")?;
            push(stack)?;
        }
        ReleaseUpdateNative { stack } => {
            push("release-update-native")?;
            push(stack)?;
        }
        RollbackNative { stack, unit } => {
            push("rollback-native")?;
            match unit {
                Some(u) => push(&format!("{stack}/{u}"))?,
                None => push(stack)?,
            }
        }
        ApplyGuards { vmid } => {
            push("guards")?;
            push(&vmid.to_string())?;
        }
        ForgetStack { stack } => {
            push("forget")?;
            push(stack)?;
        }
        DestroyStack {
            manifest,
            skip_backup,
            ..
        } => {
            push("destroy")?;
            push(&manifest.stack_name)?;
            if *skip_backup {
                push("--no-backup")?;
            }
        }
        DestroyRecorded {
            stack, skip_backup, ..
        } => {
            push("destroy")?;
            push(stack)?;
            if *skip_backup {
                push("--no-backup")?;
            }
        }
        WipeRetired { name, .. } => {
            push("wipe")?;
            push(name)?;
        }
        PruneOrphans { manifest, .. } => {
            push("prune-orphans")?;
            push(&manifest.stack_name)?;
        }
        PatchFleet => push("patch")?,
        ZfsReplicate => push("zfs-replicate")?,
        BackupHostMeta => push("backup-host-meta")?,
        BackupDevices => push("backup-devices")?,
        Ping => push("ping")?,
        Status => push("status")?,
        Doctor { .. } => push("doctor")?,
        Incidents { .. } => push("incidents")?,
        IncidentShow { name } => {
            push("incidents")?;
            push("show")?;
            push(name)?;
        }
        GetConfig => push("config")?,
        ListTemplates => push("templates")?,
        ListManualChecks { .. } => push("checks")?,
        FleetCheck { .. } => push("check")?,
        Today { .. } => push("today")?,
        AnswerManualCheck {
            check_id,
            ok,
            note,
            accept_days,
        } => {
            push("checks")?;
            push("answer")?;
            push(check_id)?;
            match accept_days {
                Some(days) => {
                    push("accept")?;
                    push(&days.to_string())?;
                }
                None => push(if *ok { "ok" } else { "nok" })?,
            }
            if !note.trim().is_empty() {
                push(note)?;
            }
        }
        BuildTemplate {
            temp_vmid,
            version,
            unprivileged,
            base_template,
        } => {
            push("template-build")?;
            push(&temp_vmid.to_string())?;
            push(&version.to_string())?;
            if !unprivileged {
                push("--privileged")?;
            }
            if let Some(b) = base_template {
                push("--base")?;
                push(b)?;
            }
        }
        ExecIn { vmid, command } => {
            push("exec")?;
            push(&vmid.to_string())?;
            push(command)?;
        }
        // No verb sends these: binaries go with deploy/install-native/
        // release-update, which fetch them themselves; the rest are reads
        // and replies only the TUI or the dashboard use.
        StageNativeBinary { .. }
        | InstallNative { .. }
        // The dashboard builds this line itself: the CLI's argument is the
        // directory of the unit's service.yml, which the command does not
        // carry (`actions::cli_override`).
        | InstallNativeRelease { .. }
        | SelfUpdateHost { .. }
        | GetState
        | GetApplied { .. }
        | SetConfig(_)
        | Answer { .. }
        | SessionOptions { .. }
        | CurrentOp
        | History { .. }
        | Notices { .. }
        | Tiles { .. }
        | GetHostConfig
        | SetHostConfig { .. }
        | Ui { .. }
        | UiAttach
        | UiReply { .. }
        | UiHold { .. } => return None,
    }
    Some(parts.join(" "))
}
