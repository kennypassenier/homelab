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
}

impl DriftState {
    pub fn label(self) -> &'static str {
        match self {
            DriftState::Changed => "[CHANGED] the files differ from what the host applied",
            DriftState::Same => "none: the files match what the host applied",
            DriftState::NoLocalFiles => "unknown (no local files)",
            DriftState::NeverApplied => "unknown (nothing applied recorded)",
            DriftState::NotCompared => "not compared yet",
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
