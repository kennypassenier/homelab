//! Installing one signed release of a managed tool on the host, keeping the
//! previous program and putting it back when the new one does not run.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor, shq};
use crate::ops::guards::fetch;
use crate::ops::native::{ReleaseRefs, glibc_probe_script, glibc_verdict, listed_sha};
use crate::release_sig::verify_sums;
use crate::tools::spec::ToolSpec;

/// Where a download waits while it is checked (root-only state directory).
const STAGING_DIR: &str = "/var/lib/homelab/staged/tools";

/// The swap, as one script so it is atomic from the outside: keep the old
/// program as `<path>.prev`, put the new one beside the target, prove it
/// runs, move it into place and prove the target runs; if the final proof
/// fails, the old program goes back and the script exits 7.
pub fn swap_script(spec: &ToolSpec, staged: &str) -> String {
    let (path, prev) = (shq(&spec.path), shq(&spec.prev_path()));
    let new = shq(&format!("{}.homelab-new", spec.path));
    format!(
        "set -e; [ -e {path} ] && cp -p {path} {prev} || true; \
         install -m 755 {staged} {new}; {new} --version >/dev/null; \
         mv -f {new} {path}; \
         {path} --version >/dev/null || {{ [ -e {prev} ] && mv -f {prev} {path}; exit 7; }}",
        staged = shq(staged),
    )
}

/// Download the release asset, check it against the signed list and the
/// machine's glibc, then swap it in. Returns the installed tag.
pub async fn install(
    exec: &dyn Executor,
    spec: &ToolSpec,
    refs: &ReleaseRefs,
) -> Result<String, CoreError> {
    let sig_url = refs.sig_url.clone().ok_or_else(|| {
        CoreError::SafetyAbort(format!(
            "{} {} is not signed — not installing it",
            spec.name, refs.tag
        ))
    })?;
    let sig = fetch(exec, &sig_url, "the signature").await?;
    let sums = fetch(exec, &refs.sums_url, "SHA256SUMS").await?;
    verify_sums(&sums, &sig)
        .map_err(|e| CoreError::SafetyAbort(format!("{} {}: {e}", spec.name, refs.tag)))?;
    let wanted = listed_sha(&sums, &spec.asset).ok_or_else(|| {
        CoreError::SafetyAbort(format!(
            "the signed SHA256SUMS of {} {} lists no '{}' — not installing it",
            spec.name, refs.tag, spec.asset
        ))
    })?;

    let staged = format!("{STAGING_DIR}/{}-{}", spec.name, refs.tag);
    let download = format!(
        "mkdir -p {STAGING_DIR} && curl -sSL -m 300 -o {f} {u} && chmod 755 {f} && \
         sha256sum {f} | cut -d' ' -f1",
        f = shq(&staged),
        u = shq(&refs.asset_url)
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &download], 400)).await?;
    let outcome = verify_download(exec, spec, refs, &wanted, &staged, &out).await;
    let _ = exec.run(&Cmd::new("rm", &["-f", &staged], 30)).await;
    outcome?;
    Ok(refs.tag.clone())
}

/// The checks between "downloaded" and "swapped": it arrived, it is the file
/// the signed list names, it can run here, and the swap keeps it running.
async fn verify_download(
    exec: &dyn Executor,
    spec: &ToolSpec,
    refs: &ReleaseRefs,
    wanted: &str,
    staged: &str,
    download: &crate::executor::CmdOutput,
) -> Result<(), CoreError> {
    if !download.success() {
        return Err(CoreError::Other(format!(
            "download of {} {} failed: {}",
            spec.name,
            refs.tag,
            download.stderr.trim()
        )));
    }
    if !download.stdout.trim().eq_ignore_ascii_case(wanted) {
        return Err(CoreError::SafetyAbort(format!(
            "CHECKSUM MISMATCH for {} in {} {}: the signed SHA256SUMS does not list this \
             download — corrupted or tampered; nothing installed",
            spec.asset, spec.name, refs.tag
        )));
    }
    let probe = exec
        .run(&Cmd::new("sh", &["-c", &glibc_probe_script(staged)], 60))
        .await?;
    glibc_verdict(&probe.stdout).map_err(|why| {
        CoreError::SafetyAbort(format!(
            "{} {}: {why} :: nothing installed",
            spec.name, refs.tag
        ))
    })?;
    let swap = exec
        .run(&Cmd::new("sh", &["-c", &swap_script(spec, staged)], 120))
        .await?;
    if swap.success() {
        return Ok(());
    }
    Err(CoreError::Command {
        rendered: format!("swap {}", spec.path),
        detail: format!(
            "{} {} did not run in place (exit {}), the previous program is back: {}",
            spec.name,
            refs.tag,
            swap.code,
            swap.stderr.trim()
        ),
    })
}

/// Put the previous program back (the dashboard's roll-back).
pub async fn rollback(exec: &dyn Executor, spec: &ToolSpec) -> Result<(), CoreError> {
    let script = format!(
        "[ -e {prev} ] && mv -f {prev} {path} && {path} --version >/dev/null",
        prev = shq(&spec.prev_path()),
        path = shq(&spec.path)
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 60)).await?;
    if out.success() {
        Ok(())
    } else {
        Err(CoreError::Other(format!(
            "no previous {} to roll back to (or it does not run)",
            spec.name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::policy::Follow;

    fn latch() -> ToolSpec {
        ToolSpec {
            name: "latch".into(),
            release_repo: "kennypassenier/latch-rs".into(),
            asset: "latch-x86_64-unknown-linux-gnu".into(),
            path: "/usr/local/bin/latch".into(),
            follow: Follow::Latest,
            version: None,
        }
    }

    #[test]
    fn the_swap_keeps_the_previous_program_and_restores_it_on_failure() {
        let s = swap_script(&latch(), "/var/lib/homelab/staged/tools/latch-v2.7.0");
        assert!(s.contains("cp -p '/usr/local/bin/latch' '/usr/local/bin/latch.prev'"));
        assert!(s.contains("mv -f '/usr/local/bin/latch.homelab-new' '/usr/local/bin/latch'"));
        assert!(s.contains("mv -f '/usr/local/bin/latch.prev' '/usr/local/bin/latch'; exit 7"));
    }

    #[test]
    fn the_new_program_must_run_before_it_replaces_anything() {
        let s = swap_script(&latch(), "/staged/x");
        let ran = s
            .find("--version >/dev/null; mv -f")
            .expect("proof before the move");
        let moved = s.find("mv -f '/usr/local/bin/latch.homelab-new'").unwrap();
        assert!(ran < moved);
    }
}
