//! fix-116: before a service's own update runs, ask GitHub whether the
//! installed binary already is the latest release. When it is, nothing is
//! copied and no update runs.

use crate::executor::{Cmd, Executor};
use crate::native::NativeServiceManifest;
use crate::runner::Scope;
use crate::sink::Level;

use super::release::{listed_sha, parse_latest_release};

/// fix-116: the tag of `repo`'s latest release and the checksum its
/// SHA256SUMS lists for `asset`. Used only to decide that nothing needs to be
/// done; installing still goes through the signed path (`release_update`) or
/// the service's own verb.
async fn latest_listed_sha(
    exec: &dyn Executor,
    repo: &str,
    asset: &str,
) -> Result<(String, String), String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let out = exec
        .run(&Cmd::new(
            "curl",
            &[
                "-sSL",
                "-m",
                "30",
                "-H",
                "Accept: application/vnd.github+json",
                &url,
            ],
            60,
        ))
        .await
        .map_err(|e| e.to_string())?;
    if !out.success() {
        return Err(out.stderr.trim().to_string());
    }
    let refs = parse_latest_release(&out.stdout, asset)?;
    let sums = exec
        .run(&Cmd::new("curl", &["-sSL", "-m", "60", &refs.sums_url], 90))
        .await
        .map_err(|e| e.to_string())?;
    if !sums.success() {
        return Err(sums.stderr.trim().to_string());
    }
    let sha = listed_sha(&sums.stdout, asset)
        .ok_or_else(|| format!("SHA256SUMS of {} lists no '{}'", refs.tag, asset))?;
    Ok((refs.tag, sha))
}

/// The steps an already-current release leaves unrun.
fn skip_rest(scope: &mut Scope<'_, '_>) {
    for step in [
        "preserve binary",
        "run self-update",
        "restart if changed",
        "keep one previous binary",
        "drop the kit's own rollback copy",
    ] {
        scope.skip(step);
    }
}

/// The release is asked first, as `release_update` does, and a service
/// already on it is left alone — no copy, no update run. When the release
/// cannot be read the service's own update decides, as before. Story:
/// docs/deployment/REGISTER.md.
///
/// Returns true when the update is done. fix-171 round 3: already current is
/// success, not a crash — the remaining planned steps are marked skipped.
pub(super) async fn already_current(
    scope: &mut Scope<'_, '_>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    before: &str,
) -> bool {
    let Some(repo) = &m.release_repo else {
        return false;
    };
    match latest_listed_sha(exec, repo, m.asset_name()).await {
        Ok((tag, listed)) if !before.is_empty() && listed.eq_ignore_ascii_case(before) => {
            skip_rest(scope);
            scope.log(
                Level::Info,
                format!(
                    "[update] {} already runs {} of {} — nothing copied, no update run",
                    m.unit, tag, repo
                ),
            );
            true
        }
        Ok(_) => false,
        Err(why) => {
            scope.log(
                Level::Info,
                format!(
                    "[update] could not read the latest release of {} ({}) — the service's own \
                     update decides",
                    repo, why
                ),
            );
            false
        }
    }
}
