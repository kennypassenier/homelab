//! feat-secrets-1/2: the host-side I/O for the dashboard's Secrets page —
//! reveal a sealed secret from the vault, and write one back through latch.
//! Lives on the host, not in `homelab-core` (AR1: core does no I/O of its
//! own); path computation (`vault_rel`/`latch_rel_path`) and the
//! `SecretRef` type stay in `homelab_core::ops::secrets`.
//!
//! Neither function goes through `Executor`/`Cmd`/`TracingExecutor`: a
//! reveal is a plain file read, and a write is a plain
//! `std::process::Command` whose stdin (the new value) and stdout (latch's
//! own content on read, not used here) are never captured into a line the
//! executor, a transcript or `audit.log` could echo (fix-39, fix-32's
//! rule, applied to a path those fixes did not yet cover). The caller is
//! still responsible for one audit line that names WHAT was read or
//! written, never the value — see `main.rs`'s `Rpc::RevealSecret` /
//! `Rpc::SetSecret` handlers.

use homelab_core::ops::secrets::{vault_rel, SecretRef};

/// feat-secrets-1: the sealed value, straight from disk. `Err` when nothing
/// has been sealed yet (a stack declared but never deployed) or the file
/// cannot be read — never panics on a missing vault.
pub async fn reveal(state_dir: &str, stack: &str, secret: &SecretRef) -> Result<String, String> {
    let path = format!("{}/secrets/{}/{}", state_dir, stack, vault_rel(secret));
    tokio::fs::read_to_string(&path).await.map_err(|e| {
        format!(
            "no sealed copy of this secret on the host yet ({path}): {e} :: deploy the stack \
             once so the host can seal a copy, or check the secret is really declared"
        )
    })
}

/// feat-secrets-2: `latch put <rel> --env <env>`, the new content on stdin.
/// A plain `std::process::Command` (not `Executor`): the value must never
/// become a line anything traces. Only the command's own exit and stderr
/// (latch's informational notes, never content) are reported.
///
/// `project_root` is the host's own intent-repo checkout — the same
/// directory `latch cat` already runs from at deploy time
/// (`{state_dir}/repo`, containing `stacks/`) — so a change here is read
/// back by the very next deploy of this stack, and nothing else is
/// touched: `latch put` names one relative path, the way the CLI's own
/// `latch cat` does for a read.
pub fn latch_put(project_root: &str, env: &str, rel: &str, content: &str) -> Result<(), String> {
    use std::io::Write as _;
    use std::process::Stdio;
    if env.trim().is_empty() {
        return Err(
            "HOMELAB_LATCH_ENV is not set on the host :: set it to the latch environment to \
             write (e.g. HOMELAB_LATCH_ENV=prod) and restart the host"
                .to_string(),
        );
    }
    let mut child = std::process::Command::new("latch")
        .args(["put", rel, "--env", env])
        .current_dir(project_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run latch: {e} :: is it installed on the host?"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "latch put: could not open stdin".to_string())?
        .write_all(content.as_bytes())
        .map_err(|e| format!("latch put {rel}: writing the new value failed: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("latch put {rel}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "latch put {} --env {} failed: {}",
            rel,
            env,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latch_put_refuses_without_an_env() {
        let e = latch_put("/tmp", "", "stack/app/.env", "x=1").unwrap_err();
        assert!(e.contains("HOMELAB_LATCH_ENV"));
    }
}
