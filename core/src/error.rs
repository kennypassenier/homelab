//! Error model (AR7): typed errors inside the crate, and at the boundary an
//! [`OperatorError`] that always tells the operator what failed, why, and
//! what they can do about it.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    /// A safety gate refused the operation before anything ran (A1/A2/A3).
    #[error("SAFETY ABORT: {0}")]
    SafetyAbort(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("command failed: {rendered}: {detail}")]
    Command { rendered: String, detail: String },

    #[error("timeout after {seconds}s: {rendered}")]
    Timeout { rendered: String, seconds: u64 },

    #[error("state error: {0}")]
    State(String),

    #[error("{0}")]
    Other(String),
}

/// Operator-facing error: what / why / what you can do. Rendered by the TUI
/// and included in incident bundles (AR14).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OperatorError {
    pub what: String,
    pub why: String,
    pub remedy: String,
}

impl OperatorError {
    pub fn from_core(step: &str, err: &CoreError) -> Self {
        let (why, remedy) = match err {
            CoreError::SafetyAbort(msg) => (
                msg.clone(),
                "This gate exists to protect unmanaged guests. Check the stack \
                 manifest (vmid, hostname) — if the refusal is wrong, the \
                 manifest is wrong."
                    .to_string(),
            ),
            CoreError::Validation(msg) => (
                msg.clone(),
                "Fix the manifest or compose file and re-run; the wizard shows \
                 the same validation inline."
                    .to_string(),
            ),
            CoreError::Timeout { seconds, .. } => (
                format!("no result within {}s", seconds),
                "Check host load and network, then re-run the operation — \
                 re-running is always safe (idempotent)."
                    .to_string(),
            ),
            CoreError::Command { detail, .. } => (
                detail.clone(),
                "See the transcript in the incident bundle for the exact \
                 command and output; re-run after fixing the cause."
                    .to_string(),
            ),
            CoreError::State(msg) => (
                msg.clone(),
                "Run doctor (F6) to compare recorded state with reality.".to_string(),
            ),
            CoreError::Other(msg) => (msg.clone(), "See transcript for context.".to_string()),
        };
        Self {
            what: format!("step '{}' failed", step),
            why,
            remedy,
        }
    }
}
