//! TODO

use super::*;

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "zfs-replicate" => cmd_zfs_replicate(c).await,
        "backup-native" => cmd_backup_native(c).await,
        "restore-native" => cmd_restore_native(c).await,
        "backup-devices" => cmd_backup_devices(c).await,
        "backup-host-meta" => cmd_backup_host_meta(c).await,
        "incidents" => cmd_incidents(c).await,
        "snapshots" => cmd_snapshots(c).await,
        "verify-restore" => cmd_verify_restore(c).await,
        "snapshot-file" => cmd_snapshot_file(c).await,
        "backup" => cmd_backup(c).await,
        "restore" => cmd_restore(c).await,
        _ => return false,
    }
    true
}

/// E8: ZFS snapshots + replication of the declared jobs.
pub(crate) async fn cmd_zfs_replicate(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::ZfsReplicate).await;
}

pub(crate) async fn cmd_backup_native(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::BackupNative { stack } = invocation(&args) else {
        die("internal: backup-native parsed as another verb")
    };
    let stack = homelab_client::repo_config::stack_name(&stack);
    rpc(host, token, Command::BackupNative { stack }).await;
}

/// fix-223: restore one (or every) unit of an adopted native stack
/// from a snapshot — the same typed-name confirmation `restore` uses.
pub(crate) async fn cmd_restore_native(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let source = &c.source;
    let homelab_client::cli_args::Invocation::RestoreNative {
        stack,
        snapshot,
        unit,
        yes,
    } = invocation(&args)
    else {
        die("internal: restore-native parsed as another verb")
    };
    println!(
        "{}▶ restore-native {}{} from '{}'{}",
        C_YELLOW,
        stack,
        unit.as_deref()
            .map(|u| format!(" :: {}", u))
            .unwrap_or_default(),
        snapshot,
        C_RESET
    );
    let confirm = if yes {
        stack.clone()
    } else {
        let typed = read_typed(&format!(
            "This overwrites the live data of '{}'{}. Type the stack name to confirm: ",
            stack,
            unit.as_deref()
                .map(|u| format!(" (unit {})", u))
                .unwrap_or_default()
        ));
        if typed != stack {
            die("name mismatch — nothing was restored");
        }
        typed
    };
    rpc(
        host,
        token,
        Command::RestoreNative {
            stack,
            snapshot,
            confirm: Some(confirm),
            unit,
            source: source.clone(),
        },
    )
    .await;
}

/// Route A: ask every configured device for its own configuration now,
/// instead of waiting for 04:00 to find out whether it works.
pub(crate) async fn cmd_backup_devices(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::BackupDevices).await;
}

/// H10: on-demand snapshot of vault/state/TLS/intent repo.
pub(crate) async fn cmd_backup_host_meta(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::BackupHostMeta).await;
}

/// fix-131: `incidents show <name>` reads one bundle; it took a root
/// shell on pve before.
pub(crate) async fn cmd_incidents(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    match (args.get(2).map(String::as_str), args.get(3)) {
        (None, _) => rpc(host, token, Command::Incidents { json: false }).await,
        (Some("show"), Some(name)) => {
            rpc(host, token, Command::IncidentShow { name: name.clone() }).await
        }
        (Some("show"), None) => {
            die("usage: homelab incidents show <name> (the names `homelab incidents` lists)")
        }
        (Some(other), _) => die(&format!(
            "unknown: homelab incidents {} :: `homelab incidents` lists, `homelab incidents \
             show <name>` reads one",
            other
        )),
    }
}

/// fix-64 (restore-no-confirm-no-safety-snapshot, 2026-09-27):
/// `homelab restore` already takes a snapshot id; this is where to
/// read what ids exist without the dashboard. Reuses `GetBackups`
/// (feat-backup-1/2) unchanged — the same per-repository status,
/// with every snapshot, the Backups page and its restore picker
/// already read.
pub(crate) async fn cmd_snapshots(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let source = &c.source;
    let stack = homelab_client::repo_config::stack_name(
        args.get(2)
            .unwrap_or_else(|| die("usage: homelab snapshots stacks/<name>")),
    );
    let json = args.iter().any(|a| a == "--json");
    let reply = rpc_reply(
        host,
        token,
        Command::GetBackups {
            stack,
            force: false,
            source: source.clone(),
        },
    )
    .await
    .unwrap_or_else(|| die("the host did not answer"));
    if !reply.ok {
        die(&reply.message);
    }
    if json {
        println!("{}", reply.message);
    } else {
        match serde_json::from_str::<homelab_client::snapshots::GetBackupsReply>(&reply.message) {
            Ok(r) => print!("{}", homelab_client::snapshots::render(r.native, &r.repos)),
            Err(_) => println!("{}", reply.message),
        }
    }
}

/// fix-237: one snapshot restored into the drill's scratch directory
/// on the host, judged, counted and emptied again — never the live
/// data. The host streams one result line per repository.
pub(crate) async fn cmd_verify_restore(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let source = &c.source;
    let homelab_client::cli_args::Invocation::VerifyRestore(a) = invocation(&args) else {
        die("internal: verify-restore parsed as another verb")
    };
    let stack = homelab_client::repo_config::stack_name(&a.stack);
    println!(
        "{}▶ verify-restore {}{} snapshot '{}' into the restore drill's scratch directory \
         (live data untouched){}",
        C_YELLOW,
        stack,
        a.app
            .as_deref()
            .map(|x| format!(" :: {}", x))
            .unwrap_or_default(),
        a.snapshot,
        C_RESET
    );
    rpc(
        host,
        token,
        Command::VerifyRestore {
            stack,
            app: a.app,
            snapshot: a.snapshot,
            source: source.clone(),
        },
    )
    .await;
}

/// fix-241: one file of one snapshot, read-only (`restic dump` on the
/// host, capped) — the exact old settings without a live restore.
pub(crate) async fn cmd_snapshot_file(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let source = &c.source;
    let a = homelab_client::snapshots::snapshot_file_args(args.get(2..).unwrap_or(&[]))
        .unwrap_or_else(|e| die(&e));
    let stack = homelab_client::repo_config::stack_name(&a.stack);
    let reply = rpc_reply(
        host,
        token,
        Command::ReadSnapshotFile {
            stack,
            owner: a.app,
            snapshot: a.snapshot,
            path: a.path,
            source: source.clone(),
        },
    )
    .await
    .unwrap_or_else(|| die("the host did not answer"));
    if !reply.ok {
        die(&reply.message);
    }
    if a.json {
        println!("{}", reply.message);
    } else {
        match serde_json::from_str::<homelab_core::ops::backup::SnapshotFile>(&reply.message) {
            Ok(f) => {
                let (text, note) = homelab_client::snapshots::render_snapshot_file(&f);
                eprintln!("{}{}{}", C_YELLOW, note, C_RESET);
                print!("{}", text);
            }
            Err(_) => println!("{}", reply.message),
        }
    }
}

pub(crate) async fn cmd_backup(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Backup { stack } = invocation(&args) else {
        die("internal: backup parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    // F291: the manifest alone. A backup runs on the host against
    // /appdata paths and needs no file and no secret from here.
    let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
    rpc(host, token, Command::BackupStack(Box::new(manifest))).await;
}

pub(crate) async fn cmd_restore(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let source = &c.source;
    // fix-64: flags may stand anywhere; the rest is dir then snapshot.
    // fix-112: `--app <name>` restores one app of the stack.
    let homelab_client::cli_args::Invocation::Restore(homelab_client::RestoreArgs {
        dir,
        snapshot,
        app,
        yes,
        skip_safety_copy,
    }) = invocation(&args)
    else {
        die("internal: restore parsed as another verb")
    };
    // fix-101: every spelling of a stack names the same directory.
    let dir = &stack_dir(&dir);
    // F294: the manifest alone, for the same reason as F291 above.
    let manifest = spec::build_manifest(Path::new(&dir)).unwrap_or_else(|e| die(&e));
    let stack = manifest.stack_name.clone();
    println!(
        "{}▶ restore {}{} from '{}'{}",
        C_YELLOW,
        stack,
        app.as_deref()
            .map(|a| format!(" :: {}", a))
            .unwrap_or_default(),
        snapshot,
        C_RESET
    );
    // fix-64: a restore overwrites live data, so the command line asks
    // for the name the way the TUI always did; `--yes` answers it
    // for scripts. The host refuses a request without it.
    let confirm = if yes {
        stack.clone()
    } else {
        let typed = read_typed(&format!(
            "This overwrites the live data of '{}'{}. Type the stack name to confirm: ",
            stack,
            if skip_safety_copy {
                ", WITHOUT a copy of it (--no-safety-copy)"
            } else {
                " (a copy of it is kept on the host first)"
            }
        ));
        if typed != stack {
            die("name mismatch — nothing was restored");
        }
        typed
    };
    rpc(
        host,
        token,
        Command::RestoreStack {
            manifest: Box::new(manifest),
            snapshot,
            confirm: Some(confirm),
            skip_safety_copy,
            app,
            source: source.clone(),
        },
    )
    .await;
}
