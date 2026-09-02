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
    // F206: the staging copy is where a secret is briefly plaintext on the
    // HOST, and it used to stay there. Nine of them were lying under
    // /var/lib/homelab when this was found; none held a secret, but only
    // because a staging file is written solely when the content CHANGED and
    // that day's re-commit had produced identical values. The next changed
    // `.env` would have stayed readable until somebody noticed.
    //
    // After the push, not before: `pct push` reads this file, and removing
    // it earlier would break the thing it exists for. Best-effort on the
    // removal itself — a push that succeeded must not be reported as failed
    // because the cleanup could not run, and the file is 0600 under a
    // root-only directory in the meantime.
    let _ = exec.run(&Cmd::new("rm", &["-f", tmp], 30)).await;
    Ok(true)
}

/// Write a file the orchestrator generates into a directory that belongs to
/// a CONTAINER, and leave it owned by whoever owns that directory.
///
/// F190: `write_file` runs as host root, and an unprivileged container's
/// files are owned by a mapped uid — 100000, not 0. A root-owned file inside
/// a bind-mounted config directory is not merely untidy: Uptime Kuma chowns
/// everything under `/app/data` at startup, cannot own a host-root file,
/// exits non-zero and crash-loops. That is how a database backup took the
/// monitoring down on 2026-09-02, and measuring afterwards showed the
/// orchestrator had the same habit — `host-monitors.json` and
/// `services.yaml` were both host-root, and both written by this code.
///
/// The owner is taken from the parent directory rather than computed,
/// because the directory was already given the right owner when the stack
/// that owns it was deployed (`host_owner_uid`, O5). Copying it needs no
/// knowledge of which container this is and cannot drift away from it.
pub async fn write_file_owned_like_dir(
    exec: &dyn Executor,
    dest: &str,
    body: &str,
    mode: u32,
) -> Result<(), CoreError> {
    exec.write_file(dest, body, mode).await?;
    let dir = std::path::Path::new(dest)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".into());
    // Best-effort on purpose: on a privileged container the reference chown
    // is a no-op, and a failure here must not fail a deploy over a
    // convenience file. It is logged by the tracing executor either way.
    let _ = exec
        .run(&Cmd::new("chown", &["--reference", &dir, dest], 30))
        .await;
    Ok(())
}

/// Single-quote a string for `sh -c`, escaping any quote it contains.
///
/// One definition on purpose: `native.rs` grew its own copy, and two
/// quoting helpers are two places to get quoting subtly different.
pub(crate) fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
