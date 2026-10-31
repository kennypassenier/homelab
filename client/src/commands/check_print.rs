//! Printing of check results: the fleet check by severity, and per-file drift diffs.

use super::*;

/// fix-103 (check-output-buries-problem, 2026-09-27): the fleet check with
/// the summary first and each severity group in its own colour.
pub(crate) fn print_check(msg: &str, ok: bool) {
    use homelab_client::output::{Tone, check_tones};
    for (i, (tone, line)) in check_tones(msg).into_iter().enumerate() {
        let color = match tone {
            Tone::Broken => C_RED,
            Tone::Drift => C_YELLOW,
            Tone::Noted => C_DIM,
            Tone::Plain => C_GREEN,
        };
        let mark = match (i, ok) {
            (0, true) => "✓ ",
            (0, false) => "✗ ",
            _ => "",
        };
        println!("{}{}{}{}", color, mark, line, C_RESET);
    }
}

/// The host's applied files for `stack`; a dim line says why when there are
/// none to compare.
async fn applied_for_diff(
    host: &str,
    token: &str,
    stack: &str,
) -> Option<Vec<homelab_proto::FileBlob>> {
    let command = Command::GetApplied {
        stack: stack.to_string(),
    };
    let Some(reply) = rpc_reply(host, token, command).await else {
        println!(
            "{}  {}: the host did not answer GetApplied for the diff{}",
            C_DIM, stack, C_RESET
        );
        return None;
    };
    if !reply.ok {
        println!("{}  {}: {}{}", C_DIM, stack, reply.message, C_RESET);
        return None;
    }
    match serde_json::from_str(&reply.message) {
        Ok(v) => Some(v),
        Err(_) => {
            println!(
                "{}  {}: the host's applied files did not read — update the host and the \
                 client to the same release{}",
                C_DIM, stack, C_RESET
            );
            None
        }
    }
}

fn print_file_diffs(stack: &str, diffs: &[homelab_client::apply::FileDiffEntry]) {
    println!("{}  {} — what changed:{}", C_YELLOW, stack, C_RESET);
    for d in diffs {
        println!("    {} {}", d.sign, d.path);
        match &d.diff {
            Some(text) => {
                for line in text.lines() {
                    println!("      {}", line);
                }
            }
            None => println!("      (secret — content not shown)"),
        }
    }
}

/// fix-219 (drift-finding-names-only-filenames, 2026-10-02): for every stack
/// a rendered fleet check/`today` text names as drifted against the host
/// (`homelab_core::ops::fleetcheck::repo_drift_stacks_in_rendered_text`),
/// print the actual per-file change: this working copy's own pinned files
/// against the host's intent repository copy (`GetApplied`, secret-free by
/// construction), the same pairing `deploy_diff` already uses for the
/// dashboard's Apply page. Only ever runs for stacks already reported
/// drifted — a clean fleet costs nothing beyond what it already cost.
pub(crate) async fn print_repo_drift_diffs(host: &str, token: &str, base: &str, rendered: &str) {
    let stacks = homelab_core::ops::fleetcheck::repo_drift_stacks_in_rendered_text(rendered);
    for stack in stacks {
        let dir = Path::new(base).join(&stack);
        let local = match homelab_client::spec::stack_files(&dir) {
            Ok(f) => f,
            Err(e) => {
                println!(
                    "{}  {}: could not read its local files for the diff — {}{}",
                    C_DIM, stack, e, C_RESET
                );
                continue;
            }
        };
        let Some(applied) = applied_for_diff(host, token, &stack).await else {
            continue;
        };
        let diffs = homelab_client::apply::file_diffs(&local, &applied);
        if !diffs.is_empty() {
            print_file_diffs(&stack, &diffs);
        }
    }
}
