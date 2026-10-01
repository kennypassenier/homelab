//! Small shared helpers for operations.

use crate::compose::ComposePs;
use crate::error::CoreError;
use crate::executor::{Cmd, CmdOutput, Executor, Script, pct_sh, run_ok};

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
        &format!(
            "sha256sum {} 2>/dev/null | cut -d' ' -f1 || true",
            shq(dest)
        ),
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

/// The one quoting helper lives with the executor; re-exported here because
/// the operations already import it from `util`.
pub(crate) use crate::executor::shq;

/// rule-20 (disk-audit, 2026-10-01): how old an orphaned `push-staging-*`
/// file must be before the cleanup removes it. A legitimate one lives for
/// the few seconds between `write_file` and the `pct push` + `rm -f` that
/// follow it in `push_content_staged`; that `rm` only runs on the success
/// path, so a push the daemon never finished (died mid-step, the container
/// was unreachable) leaves its staging file behind for good — `push-staging-118-*`
/// files from 2026-09-02 were still there a month later. An hour is far
/// longer than any push takes, so nothing a push still owns is ever swept.
pub const STALE_PUSH_STAGING_MAX_AGE_S: u64 = 3600;

/// The names, from a `(name, mtime_unix)` listing of the state dir, that
/// match `push-staging-<vmid>-<digest>` (`staging_path`) and are at least
/// `max_age_s` old. Pure: the daemon supplies the directory listing and the
/// clock, so this is tested without touching a filesystem.
pub fn stale_push_staging(entries: &[(String, u64)], now: u64, max_age_s: u64) -> Vec<String> {
    entries
        .iter()
        .filter(|(name, mtime)| {
            name.starts_with("push-staging-") && now.saturating_sub(*mtime) >= max_age_s
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// `cd /opt/<stack>/<app>`, built through the quote-safe [`Script`] (shell-
/// strings-quoting, expert panel 2026-09-27) instead of a `format!` that
/// interpolates `stack`/`app` between bare `'{}'` quotes — the shape
/// fix-132/fix-133 found still hand-rolled at several `docker compose` call
/// sites.
pub fn app_dir_script(stack: &str, app: &str) -> Script {
    Script::new().cd(&format!("/opt/{}/{}", stack, app))
}

/// Run `docker compose <verb>` in `/opt/<stack>/<app>`, quoted through
/// [`app_dir_script`]. `verb` is a fixed, trusted fragment (never a value
/// built from stack/app data) — the same trust `Script::raw` documents.
pub async fn compose_in_app(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
    verb: &str,
    timeout_s: u64,
) -> Result<CmdOutput, CoreError> {
    let script = app_dir_script(stack, app).raw(verb).build();
    pct_sh(exec, vmid, &script, timeout_s).await
}

/// Compose's own idea of which services in `/opt/<stack>/<app>` are running,
/// read through `docker compose ps --format json` (fix-132: the text table
/// changes shape across compose versions) instead of
/// `--status running --services`.
///
/// `Ok(None)` is fix-132/fix-133's `Unknown`: the probe failed, or its
/// output held no line the parser could read as a compose-ps entry. That is
/// not the same claim as "no service is running" (an app that is actually up
/// would be reported dead), so every caller handles it as its own case
/// rather than folding it into an empty list.
///
/// `services_filter` is appended after `--format json` verbatim (a leading-
/// space-separated list of service names, or empty for the whole app) — the
/// same positional-argument filter `docker compose ps` already accepts.
pub async fn compose_running_services(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
    services_filter: &str,
) -> Result<Option<Vec<String>>, CoreError> {
    let verb = format!("docker compose ps --format json{}", services_filter);
    let out = compose_in_app(exec, vmid, stack, app, &verb, 60).await?;
    if !out.success() {
        return Ok(None);
    }
    match crate::compose::parse_compose_ps(&out.stdout) {
        ComposePs::Answered(entries) => Ok(Some(
            entries
                .into_iter()
                .filter(|e| e.running())
                .map(|e| e.service)
                .collect(),
        )),
        ComposePs::Unknown(_) => Ok(None),
    }
}

#[cfg(test)]
mod stale_push_staging_tests {
    //! rule-20: orphaned `push-staging-*` files, left behind since
    //! 2026-09-02 by pushes the daemon never finished, are swept once they
    //! are old enough that no push still owns them.
    use super::*;

    #[test]
    fn a_file_older_than_the_threshold_is_stale() {
        let entries = vec![("push-staging-118-abcd1234".to_string(), 1_000u64)];
        assert_eq!(
            stale_push_staging(&entries, 1_000 + 3600, 3600),
            vec!["push-staging-118-abcd1234".to_string()]
        );
    }

    #[test]
    fn a_fresh_file_from_a_push_in_progress_is_kept() {
        let entries = vec![("push-staging-118-abcd1234".to_string(), 1_000u64)];
        assert!(stale_push_staging(&entries, 1_060, 3600).is_empty());
    }

    #[test]
    fn a_file_that_is_not_push_staging_is_never_touched() {
        let entries = vec![("staged-host".to_string(), 0u64)];
        assert!(stale_push_staging(&entries, 10_000_000, 3600).is_empty());
    }
}
