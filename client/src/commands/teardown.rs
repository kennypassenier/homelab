//! Teardown verbs: destroy, wipe, prune-orphans.

use super::*;

/// ask-8: a stack whose directory is gone is destroyed from the
/// manifest the host recorded when it last applied it.
async fn destroy_recorded(host: &str, token: &str, dir: &Path, skip_backup: bool, yes: bool) {
    let stack = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    eprintln!(
        "{}! no {}/lxc-compose.yml — destroying '{}' from the manifest the host \
         recorded{}",
        C_YELLOW,
        dir.display(),
        stack,
        C_RESET
    );
    let confirm = if yes {
        stack.clone()
    } else {
        read_typed(&format!(
            "Type the stack name '{}' to confirm destroy: ",
            stack
        ))
    };
    if confirm != stack {
        die("name mismatch — aborted");
    }
    let destroy = Command::DestroyRecorded {
        stack,
        confirm,
        skip_backup,
    };
    rpc(host, token, destroy).await;
}

/// C2: typed-name confirmation, exactly like the TUI; cli-yes:
/// a line copied from the dashboard's form, where the name was
/// typed already, carries --yes.
fn confirm_destroy(stack: &str, yes: bool) -> String {
    if yes {
        return stack.to_string();
    }
    prompt_stderr(&format!(
        "{}Type the stack name '{}' to confirm destroy: {}",
        C_RED, stack, C_RESET
    ))
}

/// The prompt on stderr and the trimmed line typed in answer.
fn prompt_stderr(prompt: &str) -> String {
    eprint!("{}", prompt);
    use std::io::Write as _;
    std::io::stderr().flush().ok();
    let mut typed = String::new();
    std::io::stdin().read_line(&mut typed).ok();
    typed.trim().to_string()
}

pub(crate) async fn cmd_destroy(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    let homelab_client::cli_args::Invocation::Destroy {
        stack,
        skip_backup,
        yes,
    } = invocation(&c.args)
    else {
        die("internal: destroy parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    if !Path::new(dir).join("lxc-compose.yml").exists() {
        destroy_recorded(host, token, dir, skip_backup, yes).await;
    }
    // gap-34: `DestroyStack` sends only the manifest — building
    // the full spec ran latch for any stack with
    // `latch_secrets`/`latch_files` and then threw every secret
    // away unread (F291 made the same point for `backup`).
    let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
    let stack = &manifest.stack_name;
    // Kenny's B2: the destroy backs up first and refuses if that
    // fails. Skipping is deliberate and says so out loud.
    if skip_backup {
        eprintln!(
            "{}! --no-backup: destroying without the backup that would otherwise be \
             taken first{}",
            C_YELLOW, C_RESET
        );
    }
    let confirm = confirm_destroy(stack, yes);
    if &confirm != stack {
        die("name mismatch — aborted");
    }
    let destroy = Command::DestroyStack {
        manifest: Box::new(manifest),
        confirm,
        skip_backup,
    };
    rpc(host, token, destroy).await;
}

pub(crate) async fn cmd_wipe(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    let homelab_client::cli_args::Invocation::Wipe { name, yes } = invocation(&c.args) else {
        die("internal: wipe parsed as another verb")
    };
    let name = homelab_client::repo_config::stack_name(&name);
    let list = Command::WipeRetired {
        name: name.clone(),
        confirm: None,
    };
    if !rpc_with(host, token, list).await {
        std::process::exit(1);
    }
    // cli-yes: a line copied from the dashboard's form, where the
    // name was already typed, carries --yes.
    let typed = if yes {
        name.clone()
    } else {
        read_typed(&format!(
            "Type '{}' to delete all of the above, permanently: ",
            name
        ))
    };
    if typed != name {
        die("name mismatch — nothing deleted");
    }
    let wipe = Command::WipeRetired {
        name,
        confirm: Some(typed),
    };
    rpc(host, token, wipe).await;
}

/// Kenny's H2b made this the only remover of files the repository
/// no longer has; since ask-8 (2026-09-27) the deploy removes them
/// itself, so this is mostly a no-op kept for a container that has
/// not been deployed since. Same typed confirmation as before.
pub(crate) async fn cmd_prune_orphans(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    let homelab_client::cli_args::Invocation::PruneOrphans { stack, yes } = invocation(&c.args)
    else {
        die("internal: prune-orphans parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    // gap-34: `orphan_files_keeping` (core/src/ops/deploy/orphans.rs) reads
    // only `spec.files` — never `.env` or `.secret_files` — so
    // latch never needed to run here. `build_spec` ran it anyway
    // for any stack with `latch_secrets`/`latch_files`.
    let spec = spec::build_spec_files_only(Path::new(dir)).unwrap_or_else(|e| die(&e));
    let stack = spec.manifest.stack_name.clone();
    let confirm = if yes {
        stack.clone()
    } else {
        prompt_stderr(&format!(
            "{}Type the stack name '{}' to remove the files the repository no longer \
             has (the deploy log lists them): {}",
            C_RED, stack, C_RESET
        ))
    };
    if confirm != stack {
        die("name mismatch — nothing removed");
    }
    let prune = Command::PruneOrphans {
        manifest: Box::new(spec.manifest.clone()),
        spec: Box::new(spec),
        confirm,
    };
    rpc(host, token, prune).await;
}
