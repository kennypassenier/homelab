//! The repository against what the host applied, stack by stack.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Finding, LiveFacts, REPO_DRIFT_WHAT_PREFIX, Severity};
use crate::state::{HostState, StackState};

/// fix-142: one stack directory as the client reads it, for comparison with
/// what the host last applied.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StackDigest {
    /// The stack's name (its directory name).
    pub stack: String,
    /// The parsed `lxc-compose.yml`; None for a stack with only a
    /// `service.yml`. Sent whole rather than hashed so the host compares it
    /// through its own type, and a client one release older does not make
    /// every stack look changed. Used only when `component_digests` is empty
    /// on either side (fix-201).
    #[serde(default)]
    pub manifest: Option<crate::manifest::StackManifest>,
    /// Every file a deploy would send into the container → sha256 hex. Used
    /// only when `component_digests` is empty on either side (fix-201).
    #[serde(default)]
    pub files: std::collections::BTreeMap<String, String>,
    /// fix-201 (today-reports-just-deployed-stacks-as-drifted): this stack
    /// directory's manifest and files digested the same way
    /// `StackState::component_digests` is — the same inputs `intent_hash`
    /// uses, from the fully derived spec (tile `probe` fields included, no
    /// latch: F291). Preferred over `manifest`/`files` above whenever both
    /// sides have it, because it compares the raw spec the host actually
    /// applied against the raw spec this directory would build, rather than
    /// a parsed-manifest snapshot built a different way on one side and a
    /// separate host-side git mirror of the repository on the other — a
    /// mirror that can simply not have caught up yet, and that was never
    /// asked for `lxc-compose.yml` in the first place. Empty (`has_digests()`
    /// false) for an older client.
    #[serde(default)]
    pub component_digests: crate::manifest::ComponentDigests,
}

/// fix-142: the repository against what the host applied. Story:
/// `docs/deployment/REGISTER.md`.
///
/// Each difference is a Drift finding naming what differs: the manifest
/// (compared through the host's own type), and every container-bound file
/// changed, new or gone against the host's intent copy.
///
/// Only what was asked: no digests (an older client, or the nightly round,
/// which has no repository) means no comparison; a stack whose intent copy
/// could not be read is compared on its manifest alone.
///
/// fix-201: when both sides carry `component_digests` (recorded at the last
/// deploy from the exact spec it applied — see `manifest::component_digests`
/// and `StackState::component_digests`), the manifest and files comparison
/// below uses those instead of `manifest`/`intent_files` — raw spec vs raw
/// spec, the same inputs `intent_hash` used to decide `apply` already saw
/// this stack as unchanged. That is what fixed the finding: `apply --plan`
/// and `today` used to read "what changed" from two different places and
/// could disagree. The old comparison remains the fallback for a host or
/// client that recorded no component digests yet.
pub fn evaluate_repo_drift(state: &HostState, live: &LiveFacts) -> Vec<Finding> {
    let mut out = Vec::new();
    for d in &live.digests {
        let Some(st) = state.stacks.get(&d.stack) else {
            out.extend(never_deployed(d, live));
            continue;
        };
        let parts = if d.component_digests.has_digests() && st.component_digests.has_digests() {
            component_parts(d, st)
        } else {
            fallback_parts(d, st, live)
        };
        if !parts.is_empty() {
            out.push(drift_finding(d, st, &parts));
        }
    }
    out
}

/// A stack directory the host has no record of. A vmid that is a live
/// container nobody manages is already reported by the stack-file check.
fn never_deployed(d: &StackDigest, live: &LiveFacts) -> Option<Finding> {
    let vmid = d.manifest.as_ref().map(|m| m.vmid);
    let taken = vmid.is_some_and(|v| live.containers.iter().any(|(c, _)| *c == v));
    let on_demand = d.manifest.as_ref().is_some_and(|m| m.on_demand);
    if d.manifest.is_none() || taken || on_demand {
        return None;
    }
    Some(Finding {
        severity: Severity::Drift,
        subject: d.stack.clone(),
        what: format!(
            "is declared in stacks/{} (vmid {}) but was never deployed",
            d.stack,
            vmid.unwrap_or_default()
        ),
        remedy: format!(
            "`homelab apply` would create it; deploy it with `homelab deploy \
             stacks/{0}`, or remove stacks/{0}/ if it is not meant to exist",
            d.stack
        ),
    })
}

/// fix-201: raw spec vs raw spec — the same inputs `intent_hash` compared
/// when this deploy was judged unchanged, never the separate host-side
/// intent-history mirror (`live.intent_files`), which is not asked for in
/// this branch at all.
fn component_parts(d: &StackDigest, st: &StackState) -> Vec<String> {
    let mut parts = Vec::new();
    if d.component_digests.manifest != st.component_digests.manifest {
        parts.push("lxc-compose.yml differs".into());
    }
    parts.extend(file_diff_parts(
        &d.component_digests.files,
        &st.component_digests.files,
    ));
    parts
}

/// Fallback: no component digests on one side (an older client or host, or
/// a stack never (re)applied since fix-192) — compare the parsed manifest
/// and the files against the host's git-mirror intent copy, same as before
/// fix-201.
fn fallback_parts(d: &StackDigest, st: &StackState, live: &LiveFacts) -> Vec<String> {
    let mut parts = Vec::new();
    if let (Some(local), Some(applied)) = (d.manifest.as_ref(), st.manifest.as_ref())
        && serde_json::to_value(local).ok() != serde_json::to_value(applied).ok()
    {
        parts.push("lxc-compose.yml differs".into());
    }
    if let Some(copy) = live.intent_files.get(&d.stack) {
        parts.extend(file_diff_parts(&d.files, copy));
    }
    parts
}

/// Changed, new and gone files between a local and an applied file map.
fn file_diff_parts(
    local: &BTreeMap<String, String>,
    applied: &BTreeMap<String, String>,
) -> Vec<String> {
    let changed: Vec<&String> = local
        .iter()
        .filter(|(p, h)| applied.get(*p).is_some_and(|c| c != *h))
        .map(|(p, _)| p)
        .collect();
    let added: Vec<&String> = local.keys().filter(|p| !applied.contains_key(*p)).collect();
    let gone: Vec<&String> = applied.keys().filter(|p| !local.contains_key(*p)).collect();
    let mut parts = Vec::new();
    if !changed.is_empty() {
        parts.push(format!("changed: {}", join_names(changed)));
    }
    if !added.is_empty() {
        parts.push(format!("new: {}", join_names(added)));
    }
    if !gone.is_empty() {
        parts.push(format!("gone from the files: {}", join_names(gone)));
    }
    parts
}

/// File names as one comma-separated list.
pub(super) fn join_names(names: Vec<&String>) -> String {
    names
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn drift_finding(d: &StackDigest, st: &StackState, parts: &[String]) -> Finding {
    Finding {
        severity: Severity::Drift,
        subject: d.stack.clone(),
        what: format!(
            "{} {} — {}",
            REPO_DRIFT_WHAT_PREFIX,
            crate::state::ymd(st.applied_at),
            parts.join("; ")
        ),
        remedy: format!(
            "`homelab deploy stacks/{}` (or `homelab apply`) to apply them, or put the \
             files back as they were",
            d.stack
        ),
    }
}
