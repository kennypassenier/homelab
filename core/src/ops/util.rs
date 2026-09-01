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
    push_content_staged(exec, vmid, dest, content, perms, &staging_path(vmid, dest)).await
}

/// T74: the staging path, unique per target file.
///
/// It used to be one fixed path for the whole daemon. That is harmless while
/// the host runs exactly one mutating operation at a time — which it does —
/// and silent the moment it does not: two pushes would overwrite each other's
/// staging file and land one stack's compose in another stack's container,
/// with BOTH copies reporting success. Nothing in the error output would
/// point at the cause, because there is no error.
///
/// Derived rather than random or time-based: core never reads clocks (that
/// is what makes its operations reproducible in tests), and a random name
/// would leave a different orphan behind on every failed push. Keyed on
/// `(vmid, dest)`, so two pushes to genuinely different files never collide,
/// and two pushes to the SAME file on the same container share a path — which
/// is correct, because that is one file and the race is the caller's.
pub fn staging_path(vmid: u16, dest: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(dest.as_bytes());
    let digest: String = h
        .finalize()
        .iter()
        .take(8)
        .map(|b| format!("{:02x}", b))
        .collect();
    format!("/var/lib/homelab/push-staging-{}-{}", vmid, digest)
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
