//! The first half of a release install: ask GitHub, verify the signed
//! checksum list and compare with the installed binary, stopping early (and
//! skip-marking the rest of the plan) when there is nothing to install.

use super::install::install_plan_names;
use super::release::ReleaseRefs;
use super::release_plan::asked_label;
use super::release_steps::{Wanted, ask_github, compare_installed, read_checksums};
use crate::error::CoreError;
use crate::executor::Executor;
use crate::native::NativeServiceManifest;
use crate::runner::{Scope, StepFailure};
use crate::sink::Level;

/// Mark every planned step of this run that an early exit does not reach —
/// `names`, the unit-file read when it was planned, and the nested
/// `install_native` + `adopt` names (fix-step-plan-nested). Every name in
/// the announced plan gets exactly one mark.
fn skip_rest(
    scope: &mut Scope<'_, '_>,
    m: &NativeServiceManifest,
    unit_from_repo: Option<&str>,
    names: &[&str],
) {
    for name in names {
        scope.skip(name);
    }
    if unit_from_repo.is_none() {
        scope.skip("read the unit file from the container");
    }
    for name in install_plan_names(m, Some(&format!("install-{}", m.unit))) {
        scope.skip(&name);
    }
}

/// fix-29: an unsigned release is not installed. Not an error when nobody
/// asked for this release: the author signs after uploading, and the next
/// night tries again. fix-171 round 3: an unsigned release is a legitimate
/// reason to stop, in BOTH branches, so the rest of the announced plan is
/// marked skipped rather than left for n to fall permanently short of m.
fn stop_unsigned(
    scope: &mut Scope<'_, '_>,
    m: &NativeServiceManifest,
    w: &Wanted<'_>,
    (refs, unit_from_repo): (&ReleaseRefs, Option<&str>),
) -> Result<(), StepFailure> {
    let rest = [
        "read the checksum list",
        "compare with the installed binary",
        "download and verify the asset",
    ];
    skip_rest(scope, m, unit_from_repo, &rest);
    if w.pinned.is_some() {
        return Err(StepFailure {
            step: "read the checksum list".to_string(),
            err: CoreError::SafetyAbort(format!(
                "{} {} of {} is not signed (no SHA256SUMS.minisig) — not installing it; \
                 sign it first",
                m.unit, refs.tag, w.repo
            )),
        });
    }
    scope.log(
        Level::Info,
        format!(
            "[release] {} {} of {} is not signed yet — skipped, tried again next night",
            m.unit, refs.tag, w.repo
        ),
    );
    Ok(())
}

/// Ask GitHub, verify the signed checksum list and compare with the
/// installed binary. `None` when the run ends here: the release is unsigned
/// (not an error unless pinned) or the binary already is the wanted one.
pub(super) async fn verify_phase<'a>(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    (w, unit_from_repo): (&Wanted<'_>, Option<&str>),
    scope: &mut Scope<'_, 'a>,
) -> Result<Option<(ReleaseRefs, String)>, StepFailure> {
    let mut slot: Option<ReleaseRefs> = None;
    let asked = asked_label(w.pinned);
    scoped_step!(scope, asked.as_str(), ask_github(exec, w, &mut slot).await);
    let refs = slot.expect("set by the step above");
    let Some(sig_url) = refs.sig_url.clone() else {
        stop_unsigned(scope, m, w, (&refs, unit_from_repo))?;
        return Ok(None);
    };
    let mut wanted = String::new();
    scoped_step!(
        scope,
        "read the checksum list",
        read_checksums(exec, (m, w), (&refs, &sig_url), &mut wanted).await
    );
    let mut current = false;
    scoped_step!(
        scope,
        "compare with the installed binary",
        compare_installed(exec, m, &wanted, &mut current).await
    );
    if current {
        // fix-171 round 3: already on the wanted binary is success, not
        // failure — the remaining planned steps are marked skipped.
        skip_rest(scope, m, unit_from_repo, &["download and verify the asset"]);
        let (tag, repo) = (&refs.tag, w.repo);
        let note = format!(
            "[release] {} already runs {tag} of {repo} — nothing to install",
            m.unit
        );
        scope.log(Level::Info, note);
        return Ok(None);
    }
    Ok(Some((refs, wanted)))
}
