//! C7: native Rust services — bare binaries under systemd in their own LXC,
//! no docker layer. The homelab is the safety net their self-update cannot
//! be (keep the previous binary, arm a rollback from OUTSIDE the app), and
//! adoption lets it take over a container that was built by hand first —
//! Kenny's stated workflow: try a service outside the homelab, then have it
//! inlined without a restart.

use serde::{Deserialize, Serialize};

/// Everything the homelab needs to know about one native service. One
/// service per stack/container — the shapes that need more run compose.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeServiceManifest {
    pub stack_name: String,
    pub vmid: u16,
    pub hostname: String,
    /// systemd unit name, without the `.service` suffix.
    pub unit: String,
    /// Absolute path of the installed binary inside the container.
    pub binary: String,
    /// Absolute path of the EnvironmentFile, if the unit uses one.
    #[serde(default)]
    pub env_file: Option<String>,
    /// Absolute in-container paths whose contents are the service's state.
    /// Nightly backup reaches them with `pct exec tar | restic --stdin`
    /// (adoption never restarts a service, so a bind-mount to /appdata is
    /// not an option — the data stays where it is).
    #[serde(default)]
    pub data_dirs: Vec<String>,
    /// The self-update verb, e.g. `mailbox update`. None = the homelab
    /// never updates this service (by decision, recorded here).
    #[serde(default)]
    pub update_cmd: Option<String>,
}

fn lower_dashed(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Same fail-closed philosophy as `manifest::validate`: every problem in
/// one pass, so the operator fixes the file once, not five times.
pub fn validate_native(m: &NativeServiceManifest) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    if !lower_dashed(&m.stack_name) {
        problems.push(format!(
            "stack_name '{}' must be non-empty lowercase [a-z0-9-]",
            m.stack_name
        ));
    }
    if !lower_dashed(&m.unit) {
        problems.push(format!(
            "unit '{}' must be non-empty lowercase [a-z0-9-] (no .service suffix)",
            m.unit
        ));
    }
    let canonical = format!("{}-app-{}", m.vmid, m.stack_name);
    if m.hostname != canonical {
        problems.push(format!(
            "hostname '{}' must be '{}' (A2 guard depends on it)",
            m.hostname, canonical
        ));
    }
    for p in std::iter::once(&m.binary)
        .chain(m.env_file.iter())
        .chain(m.data_dirs.iter())
    {
        if !p.starts_with('/') || p.contains("..") {
            problems.push(format!("path '{}' must be absolute and free of '..'", p));
        }
    }
    if m.data_dirs.is_empty() {
        problems.push(
            "data_dirs is empty — a service with no declared state cannot be backed up; \
             declare at least one directory (or the decision to have none belongs in the \
             stack file as a comment AND an explicit empty override once that exists)"
                .into(),
        );
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}
