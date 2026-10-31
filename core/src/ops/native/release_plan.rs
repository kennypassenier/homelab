//! The step plans of a release install: the fixed list an unpinned update
//! announces, and the full per-unit plan including the nested install and
//! adopt it ends with.

use super::install::install_plan_names;
use crate::native::NativeServiceManifest;

/// fix-171 round 3 ("exactly the kyu install where Kenny saw the counter
/// climb"): `release_update`'s own fixed step plan — unpinned, and the unit
/// file always read from the container (never given). Every one of
/// `release_install`'s early exits (no release_repo, unsigned release, the
/// binary already current) is a legitimate reason to stop, not a defect, so
/// each one skip-marks whatever of this list it does not reach rather than
/// leaving the run's total to climb mid-flight. The one case that marks
/// nothing at all is "no release_repo declared" — nothing is ever attempted,
/// so nothing needs a plan.
pub const RELEASE_UPDATE_STEPS: &[&str] = &[
    "guard target",
    "ask GitHub for the latest release",
    "read the checksum list",
    "compare with the installed binary",
    "download and verify the asset",
    "read the unit file from the container",
];

/// The name of the step that asks GitHub, which says which release.
pub(super) fn asked_label(pinned: Option<&str>) -> String {
    match pinned {
        None => "ask GitHub for the latest release".to_string(),
        Some(t) => format!("ask GitHub for release {t}"),
    }
}

/// fix-step-plan-nested: `release_install`'s own full plan — its own named
/// steps (which vary with `pinned`/`unit_from_repo`, both known up front)
/// followed by the `install_native` (itself followed by `adopt`) it always
/// ends with when it gets that far. Every name here is marked exactly once,
/// run or skip, by `release_install` — the single source that function and
/// `admin`'s batch-total both call, so the two can never drift the way
/// `RELEASE_UPDATE_STEPS.len() * units` once did (18 announced for kyu where
/// 69 marks actually ran).
pub fn release_install_plan_names(
    m: &NativeServiceManifest,
    pinned: Option<&str>,
    unit_from_repo: Option<&str>,
) -> Vec<String> {
    if m.release_repo.is_none() {
        // Nothing is ever attempted: the "zero marks, no plan needed" shape.
        return Vec::new();
    }
    let mut steps: Vec<String> = vec!["guard target".to_string(), asked_label(pinned)];
    steps.push("read the checksum list".to_string());
    steps.push("compare with the installed binary".to_string());
    steps.push("download and verify the asset".to_string());
    if unit_from_repo.is_none() {
        steps.push("read the unit file from the container".to_string());
    }
    steps.extend(install_plan_names(m, Some(&format!("install-{}", m.unit))));
    steps
}

/// fix-171 round 3 (superseded in its per-unit SHAPE by fix-step-plan-nested
/// above; `admin` now calls [`release_install_plan_names`] directly, the
/// same function `release_install` itself plans from): kept as the cheap
/// "does this unit contribute anything at all" check `admin` filters units
/// with before summing their full plans.
pub fn release_update_plan_len(m: &NativeServiceManifest) -> usize {
    if m.release_repo.is_some() {
        RELEASE_UPDATE_STEPS.len()
    } else {
        0
    }
}
