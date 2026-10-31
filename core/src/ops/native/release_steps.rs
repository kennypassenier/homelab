//! The steps of a release install that talk to GitHub or weigh what came
//! back: ask for the release, verify its checksum list, compare with the
//! installed binary, download, read the unit file.

use super::release::{ReleaseRefs, listed_sha, parse_latest_release};
use crate::error::CoreError;
use crate::executor::{Cmd, Executor};
use crate::native::NativeServiceManifest;
use crate::ops::util::shq;
use crate::ops::util_pct_sh;
use crate::runner::StepOutcome;

/// Which release of which repository an install is after.
pub(super) struct Wanted<'x> {
    pub repo: &'x str,
    pub asset: &'x str,
    /// A release tag the person chose; `None` means the latest.
    pub pinned: Option<&'x str>,
}

/// "ask GitHub for the latest release" / "... for release <tag>".
pub(super) async fn ask_github(
    exec: &dyn Executor,
    w: &Wanted<'_>,
    slot: &mut Option<ReleaseRefs>,
) -> Result<StepOutcome, CoreError> {
    let url = match w.pinned {
        None => format!("https://api.github.com/repos/{}/releases/latest", w.repo),
        Some(t) => format!(
            "https://api.github.com/repos/{}/releases/tags/{}",
            w.repo, t
        ),
    };
    let head = "Accept: application/vnd.github+json";
    let cmd = Cmd::new("curl", &["-sSL", "-m", "30", "-H", head, &url], 60);
    let out = exec.run(&cmd).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "could not reach GitHub for {}: {}",
            w.repo,
            out.stderr.trim()
        )));
    }
    *slot = Some(parse_latest_release(&out.stdout, w.asset).map_err(CoreError::Other)?);
    Ok(StepOutcome::Unchanged)
}

/// Fetch `url` with curl, or say what could not be fetched.
async fn fetch_text(exec: &dyn Executor, url: &str, what: &str) -> Result<String, CoreError> {
    let out = exec
        .run(&Cmd::new("curl", &["-sSL", "-m", "60", url], 90))
        .await?;
    if !out.success() {
        return Err(CoreError::Other(format!("{}: {}", what, out.stderr.trim())));
    }
    Ok(out.stdout)
}

/// "read the checksum list": SHA256SUMS, verified against its signature,
/// must list the asset. Sets `wanted` to the listed checksum.
pub(super) async fn read_checksums(
    exec: &dyn Executor,
    (m, w): (&NativeServiceManifest, &Wanted<'_>),
    (refs, sig_url): (&ReleaseRefs, &str),
    wanted: &mut String,
) -> Result<StepOutcome, CoreError> {
    let tag = &refs.tag;
    let what = format!("could not fetch SHA256SUMS of {} {}", w.repo, tag);
    let sums = fetch_text(exec, &refs.sums_url, &what).await?;
    let what = format!("could not fetch the signature of {} {}", w.repo, tag);
    let sig = fetch_text(exec, sig_url, &what).await?;
    crate::release_sig::verify_sums(&sums, &sig)
        .map_err(|e| CoreError::Other(format!("{} {} of {}: {}", m.unit, tag, w.repo, e)))?;
    *wanted = listed_sha(&sums, w.asset).ok_or_else(|| {
        CoreError::Other(format!(
            "SHA256SUMS of {} {} lists no '{}' — refusing an unverifiable binary",
            w.repo, tag, w.asset
        ))
    })?;
    Ok(StepOutcome::Unchanged)
}

/// "compare with the installed binary": sets `current` when it already is
/// the wanted one.
pub(super) async fn compare_installed(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    wanted: &str,
    current: &mut bool,
) -> Result<StepOutcome, CoreError> {
    let out = util_pct_sh(
        exec,
        m.vmid,
        &format!("sha256sum {} 2>/dev/null | cut -d' ' -f1", shq(&m.binary)),
        60,
    )
    .await?;
    *current = out.stdout.trim().eq_ignore_ascii_case(wanted);
    Ok(StepOutcome::Unchanged)
}

/// "download and verify the asset": downloaded to `staged` on this host,
/// checked against the signed list, base64-encoded for the transfer into the
/// container (sets `b64`), and removed again.
pub(super) async fn download_asset(
    exec: &dyn Executor,
    w: &Wanted<'_>,
    (refs, wanted, staged): (&ReleaseRefs, &str, &str),
    b64: &mut String,
) -> Result<StepOutcome, CoreError> {
    let dir = staged.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
    let script = format!(
        "mkdir -p {d} && curl -sSL -m 600 -o {f} {u} && sha256sum {f} | cut -d' ' -f1",
        d = shq(dir),
        f = shq(staged),
        u = shq(&refs.asset_url)
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 700)).await?;
    let rm = Cmd::new("rm", &["-f", staged], 30);
    if !out.success() {
        let _ = exec.run(&rm).await;
        return Err(CoreError::Other(format!(
            "download of {} {} failed: {}",
            w.asset,
            refs.tag,
            out.stderr.trim()
        )));
    }
    if !out.stdout.trim().eq_ignore_ascii_case(wanted) {
        let _ = exec.run(&rm).await;
        return Err(CoreError::SafetyAbort(format!(
            "CHECKSUM MISMATCH for {} in {} {} — download corrupted or tampered; nothing \
             installed",
            w.asset, w.repo, refs.tag
        )));
    }
    *b64 = encode_and_remove(exec, staged).await?;
    Ok(StepOutcome::Changed)
}

/// The downloaded file as base64 for the transfer into the container; the
/// file is removed whether or not the encoding worked.
async fn encode_and_remove(exec: &dyn Executor, staged: &str) -> Result<String, CoreError> {
    let enc = exec.run(&Cmd::new("base64", &["-w0", staged], 300)).await?;
    let _ = exec.run(&Cmd::new("rm", &["-f", staged], 30)).await;
    if !enc.success() || enc.stdout.trim().is_empty() {
        return Err(CoreError::Other(format!(
            "could not encode {} for the transfer into the container",
            staged
        )));
    }
    Ok(enc.stdout.trim().to_string())
}

/// "read the unit file from the container": the one systemd is running; a
/// rebuild put it there from the repository. Sets `unit_file`.
pub(super) async fn read_unit_file(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    unit_file: &mut String,
) -> Result<StepOutcome, CoreError> {
    let out = util_pct_sh(
        exec,
        m.vmid,
        &format!("cat /etc/systemd/system/{}.service", m.unit),
        30,
    )
    .await?;
    if !out.success() || out.stdout.trim().is_empty() {
        return Err(CoreError::Other(format!(
            "{}.service is not on {} — a service without its unit is not one this \
         orchestrator installs; adopt or deploy it first",
            m.unit, m.hostname
        )));
    }
    *unit_file = out.stdout;
    Ok(StepOutcome::Unchanged)
}
