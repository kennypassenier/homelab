//! TUI parity ([CHANGED] per stack, fix-107): what is known about whether a
//! stack's files differ from what the host applied. Only what was compared
//! is green: a stack nobody compared is not "in sync". Pure.

use serde::Serialize;

/// The TUI's five answers (`DriftState`), with the same words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftState {
    /// The files differ from what the host applied: [CHANGED].
    Changed,
    /// The files match what the host applied.
    Same,
    /// The working copy has no deployable directory for the stack.
    NoLocalFiles,
    /// The host has recorded no applied deploy to compare with.
    NeverApplied,
    /// The local hash could not be computed (yet).
    NotCompared,
    /// redesign-stacks-7: in the working copy, not on the host yet: the
    /// next Deploy all changes creates its container. Dashboard only (the
    /// TUI lists the host's stacks).
    New,
}

impl DriftState {
    pub fn label(self) -> &'static str {
        match self {
            DriftState::Changed => "[CHANGED] the files differ from what the host applied",
            DriftState::Same => "none: the files match what the host applied",
            DriftState::NoLocalFiles => "unknown (no local files)",
            DriftState::NeverApplied => "unknown (nothing applied recorded)",
            DriftState::NotCompared => "not compared yet",
            DriftState::New => "new: in the files, not on the host yet",
        }
    }
}

/// One stack's answer. `local` is the working copy's intent hash for the
/// stack (None: no deployable directory; Err: it did not build).
pub fn drift_state(applied_hash: &str, local: Option<&Result<String, String>>) -> DriftState {
    match local {
        None => DriftState::NoLocalFiles,
        Some(_) if applied_hash.is_empty() => DriftState::NeverApplied,
        Some(Err(_)) => DriftState::NotCompared,
        Some(Ok(h)) if h == applied_hash => DriftState::Same,
        Some(Ok(_)) => DriftState::Changed,
    }
}

/// redesign-stacks-7 (3.71.0 review, "the differs-from-files count is
/// wrong"): every stack the next Deploy all changes touches, in the same
/// words as its plan (`applyview::plan`), so the Stacks page's count, its
/// flags and the plan can never disagree. `host` is every stack the host
/// records with the hash it applied; the others are `LocalStacks`' fields.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DriftAnswer {
    /// Per stack: its state and, for one that does not build, why.
    pub stacks: std::collections::BTreeMap<String, (DriftState, Option<String>)>,
    /// What Deploy all changes would do: deploy (new ones included),
    /// create, destroy, and how many stacks block it by not building.
    pub deploy: usize,
    pub new: usize,
    pub destroy: usize,
    pub broken: usize,
}

pub fn drift_answer(
    host: &[(String, String)],
    hashes: &[(String, Result<String, String>)],
    dirs: &[String],
    ephemeral: &[String],
) -> DriftAnswer {
    let view = super::applyview::plan(hashes, dirs, host, ephemeral);
    let mut stacks = std::collections::BTreeMap::new();
    for (name, applied) in host {
        let local = hashes.iter().find(|(n, _)| n == name).map(|(_, h)| h);
        let mut state = drift_state(applied, local);
        // "No local files" means gone from the files only when the plan
        // destroys it; a directory that declares nothing (an ephemeral
        // stack, a half-written one) is not compared, never "gone".
        if state == DriftState::NoLocalFiles && !view.destroy.contains(name) {
            state = DriftState::NotCompared;
        }
        stacks.insert(name.clone(), (state, None));
    }
    for n in &view.deploy {
        let state = if view.new.contains(n) {
            DriftState::New
        } else {
            DriftState::Changed
        };
        stacks.insert(n.clone(), (state, None));
    }
    for n in &view.destroy {
        stacks.insert(n.clone(), (DriftState::NoLocalFiles, None));
    }
    for (n, why) in &view.broken {
        stacks.insert(n.clone(), (DriftState::NotCompared, Some(why.clone())));
    }
    for n in &view.unchanged {
        stacks.insert(n.clone(), (DriftState::Same, None));
    }
    DriftAnswer {
        stacks,
        deploy: view.deploy.len(),
        new: view.new.len(),
        destroy: view.destroy.len(),
        broken: view.broken.len(),
    }
}
