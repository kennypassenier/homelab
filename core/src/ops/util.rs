//! Small shared helpers for operations.

use crate::error::CoreError;
use crate::executor::{pct_sh, run_ok, Cmd, Executor};

/// Push literal content to a path inside an LXC. Returns true when the
/// destination changed (drives conditional restarts — B1).
///
/// The idempotency compare uses sha256, not `cat`: pushed files include
/// `.env` secrets, and the tracing executor echoes command output into the
/// transcript/incident stream — a hash may appear there, plaintext never
/// (standing rule 10; regression-guarded by `secrets_tests.rs`).
pub async fn push_content(
    exec: &dyn Executor,
    vmid: u16,
    dest: &str,
    content: &str,
    perms: &str,
) -> Result<bool, CoreError> {
    push_content_staged(
        exec,
        vmid,
        dest,
        content,
        perms,
        "/var/lib/homelab/push-staging",
    )
    .await
}

/// H21 hardening: the staging file lives under the root-only state dir, not
/// a predictable world-writable /tmp path (symlink-planting classic).
pub async fn push_content_staged(
    exec: &dyn Executor,
    vmid: u16,
    dest: &str,
    content: &str,
    perms: &str,
    staging: &str,
) -> Result<bool, CoreError> {
    let remote = pct_sh(
        exec,
        vmid,
        &format!("sha256sum '{}' 2>/dev/null | cut -d' ' -f1 || true", dest),
        30,
    )
    .await?
    .stdout;
    let local = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(content.as_bytes());
        h.finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    };
    if remote.trim() == local {
        return Ok(false);
    }
    if let Some(parent) = std::path::Path::new(dest).parent() {
        run_ok(
            exec,
            &Cmd::new(
                "pct",
                &[
                    "exec",
                    &vmid.to_string(),
                    "--",
                    "mkdir",
                    "-p",
                    &parent.display().to_string(),
                ],
                30,
            ),
        )
        .await?;
    }
    let tmp = staging;
    exec.write_file(tmp, content, 0o600).await?;
    run_ok(
        exec,
        &Cmd::new(
            "pct",
            &["push", &vmid.to_string(), tmp, dest, "--perms", perms],
            60,
        ),
    )
    .await?;
    Ok(true)
}
