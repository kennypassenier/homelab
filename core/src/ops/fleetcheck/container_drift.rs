//! The files a deploy pushed against what sits in the container now.

use super::repo_drift::join_names;
use super::{Finding, LiveFacts, Severity};
use crate::state::HostState;

/// fix-142 (nightly hash comparison, Kenny's go 2026-10-01): the files a
/// deploy pushed under `/opt/<stack>/` against what sits in the container
/// right now, hashed inside it.
///
/// Unlike `evaluate_repo_drift`, which compares the repository with the
/// host's intent-history copy, this compares the host's own record of what
/// it pushed — post any registry-cache compose rewrite,
/// `StackState::pushed_file_hashes` — with the live container. That was the
/// missing half the original fleetcheck-blind-to-repo-drift finding could
/// not build: a deploy rewrites every cached app's compose file, so hashing
/// the repository's own copy and comparing it straight against the
/// container would call every cached app drifted. Now the host hashes what
/// it actually wrote, once, at deploy time, and only ever compares against
/// that.
///
/// Nightly only (`live.container_file_hashes` is gathered host-side, like
/// `digests` is client-side for `evaluate_repo_drift` — the two run in
/// different places and never both at once). A stack with no recorded
/// pushed hashes (never deployed since this was built, or adopted rather
/// than deployed) is skipped, and a container that could not be asked is
/// left out of the comparison rather than reported as entirely gone — an
/// unasked question is not a finding. A file the container has that was
/// never pushed is not reported either: `/opt/<stack>/` legitimately holds
/// files no deploy wrote (logs, caches, `generated_dirs`), and listing those
/// every night would bury the files that actually matter.
pub fn evaluate_container_drift(state: &HostState, live: &LiveFacts) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        if st.pushed_file_hashes.is_empty() {
            continue;
        }
        let Some(live_files) = live.container_file_hashes.get(name) else {
            continue;
        };
        let changed: Vec<&String> = st
            .pushed_file_hashes
            .iter()
            .filter(|(p, h)| live_files.get(*p).is_some_and(|c| c != *h))
            .map(|(p, _)| p)
            .collect();
        let gone: Vec<&String> = st
            .pushed_file_hashes
            .keys()
            .filter(|p| !live_files.contains_key(*p))
            .collect();
        if !(changed.is_empty() && gone.is_empty()) {
            out.push(drift_finding(name, &changed, &gone));
        }
    }
    out
}

fn drift_finding(name: &str, changed: &[&String], gone: &[&String]) -> Finding {
    let mut parts: Vec<String> = Vec::new();
    if !changed.is_empty() {
        parts.push(format!(
            "changed inside the container: {}",
            join_names(changed.to_vec())
        ));
    }
    if !gone.is_empty() {
        parts.push(format!(
            "gone from the container: {}",
            join_names(gone.to_vec())
        ));
    }
    Finding {
        severity: Severity::Drift,
        subject: name.to_string(),
        what: format!(
            "/opt/{} no longer matches what the last deploy pushed — {}",
            name,
            parts.join("; ")
        ),
        remedy: format!(
            "redeploy {} to put back what was pushed, or adopt the change if it was \
             intentional — nothing but a deploy is meant to write these files",
            name
        ),
    }
}
