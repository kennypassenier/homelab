//! Restoring a stack or a native service from a snapshot.

use super::*;

pub(crate) fn confirm_restore_native(stack: &str, unit: Option<&str>, yes: bool) -> String {
    if yes {
        return stack.to_string();
    }
    let typed = read_typed(&format!(
        "This overwrites the live data of '{}'{}. Type the stack name to confirm: ",
        stack,
        unit.map(|u| format!(" (unit {})", u)).unwrap_or_default()
    ));
    if typed != stack {
        die("name mismatch — nothing was restored");
    }
    typed
}

pub(crate) async fn cmd_restore_native(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    let homelab_client::cli_args::Invocation::RestoreNative {
        stack,
        snapshot,
        unit,
        yes,
    } = invocation(&c.args)
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
    let confirm = confirm_restore_native(&stack, unit.as_deref(), yes);
    let restore = Command::RestoreNative {
        stack,
        snapshot,
        confirm: Some(confirm),
        unit,
        source: c.source.clone(),
    };
    rpc(host, token, restore).await;
}

/// fix-64: a restore overwrites live data, so the command line asks
/// for the name the way the TUI always did; `--yes` answers it
/// for scripts. The host refuses a request without it.
pub(crate) fn confirm_restore(stack: &str, yes: bool, skip_safety_copy: bool) -> String {
    if yes {
        return stack.to_string();
    }
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
}

pub(crate) async fn cmd_restore(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    // fix-64: flags may stand anywhere; the rest is dir then snapshot.
    // fix-112: `--app <name>` restores one app of the stack.
    let homelab_client::cli_args::Invocation::Restore(homelab_client::RestoreArgs {
        dir,
        snapshot,
        app,
        yes,
        skip_safety_copy,
    }) = invocation(&c.args)
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
    let confirm = confirm_restore(&stack, yes, skip_safety_copy);
    let restore = Command::RestoreStack {
        manifest: Box::new(manifest),
        snapshot,
        confirm: Some(confirm),
        skip_safety_copy,
        app,
        source: c.source.clone(),
    };
    rpc(host, token, restore).await;
}
