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
    /// The self-update verb, e.g. `kyu update`. None = the homelab
    /// never updates this service (by decision, recorded here).
    #[serde(default)]
    pub update_cmd: Option<String>,
    /// T40: this service keeps no state at all, deliberately. Without it an
    /// empty `data_dirs` is refused, which is right for a service that simply
    /// forgot to declare its data — and wrong for kyu-runner, whose own unit
    /// file says "no state directory, no disk to protect" and runs under
    /// DynamicUser. The flag makes the difference visible instead of forcing
    /// a fabricated directory that would then be backed up for nothing.
    #[serde(default)]
    pub stateless: bool,
    /// T11: where the binary comes from when the orchestrator installs it —
    /// `owner/repo` of the GitHub release. None = this service is adopted
    /// only, and its binary arrived by a hand nobody wrote down. That was
    /// true of all four native services until this field existed: the
    /// container manifest said "the binaries are installed the way C7
    /// installs them" and C7 had no such verb.
    #[serde(default)]
    pub release_repo: Option<String>,
    /// The asset name inside that release. Defaults to the unit name when
    /// absent, which is what all four services happen to use.
    #[serde(default)]
    pub release_asset: Option<String>,
}

impl NativeServiceManifest {
    /// The release asset to fetch, falling back to the unit name.
    pub fn asset_name(&self) -> &str {
        self.release_asset.as_deref().unwrap_or(&self.unit)
    }
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
    if m.data_dirs.is_empty() && !m.stateless {
        problems.push(
            "data_dirs is empty — a service with no declared state cannot be backed up; \
             declare at least one directory, or set `stateless: true` if it genuinely \
             keeps none (kyu-runner is the real case: its unit says so and it runs \
             under DynamicUser)"
                .into(),
        );
    }
    if m.stateless && !m.data_dirs.is_empty() {
        problems.push(format!(
            "stateless: true but {} data_dirs are declared — one of the two is wrong, \
             and guessing which would decide silently whether this service is backed up",
            m.data_dirs.len()
        ));
    }
    // A repository is `owner/name` and nothing else. The check is here
    // rather than at the download because a typo would otherwise surface as
    // `gh` saying "release not found", which reads as "the release is
    // missing" — a completely different problem from "the stack file is
    // wrong".
    if let Some(repo) = &m.release_repo {
        let parts: Vec<&str> = repo.split('/').collect();
        if parts.len() != 2 || parts.iter().any(|p| p.is_empty()) {
            problems.push(format!("release_repo '{}' must be 'owner/name'", repo));
        }
    }
    if m.release_asset.is_some() && m.release_repo.is_none() {
        problems.push(
            "release_asset is set without a release_repo — there is nowhere to fetch it from"
                .into(),
        );
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// A5 · what a unit file needs before it can start.
///
/// The G13 drill measured what happens without this: the deploy ran
/// `systemctl enable --now kyu` on a container where `/usr/local/bin` was
/// empty, the user `kyu` did not exist and the env file was not there. Three
/// things nothing created, started in an order that gave them no chance —
/// and thirteen restarts before systemd gave up. It worked everywhere else
/// only because every native container had been built by hand and adopted
/// afterwards, so a lost container could not be rebuilt at all.
///
/// Read off the unit file rather than the manifest on purpose: the unit is
/// what systemd obeys, and a manifest that disagrees with it would be a
/// second source of truth to keep in step.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnitPrereqs {
    /// `User=` — the account systemd runs it as. `DynamicUser=yes` means
    /// systemd makes one per start, so there is nothing to create.
    pub user: Option<String>,
    /// `EnvironmentFile=` paths. A leading `-` makes the file optional to
    /// systemd, and that is kept: an optional file missing is not a fault.
    pub env_files: Vec<String>,
    /// `LoadCredential=id:path` — systemd copies these into a private
    /// directory before the service starts, so a missing one fails the start
    /// exactly like a missing env file.
    pub credentials: Vec<String>,
    /// The program `ExecStart=` runs, with its arguments stripped.
    pub binary: Option<String>,
}

/// Parse the prerequisites out of a unit file.
///
/// Only `[Service]` keys are read. A key in the wrong section is invisible to
/// systemd — which this project learned the expensive way on 2026-09-02, when
/// `StartLimitIntervalSec` sat in `[Service]` and was silently ignored while
/// the comment above it promised the opposite (F227).
pub fn unit_prereqs(text: &str) -> UnitPrereqs {
    let mut out = UnitPrereqs::default();
    let mut section = String::new();
    let mut dynamic_user = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.to_string();
            continue;
        }
        if section != "[Service]" || t.starts_with('#') {
            continue;
        }
        let Some((k, v)) = t.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "User" => out.user = Some(v.to_string()),
            "DynamicUser" => dynamic_user = matches!(v, "yes" | "true" | "1"),
            "EnvironmentFile" => {
                // A leading '-' is systemd's own "may be absent".
                if let Some(p) = v.strip_prefix('-') {
                    let _ = p;
                } else {
                    out.env_files.push(v.to_string());
                }
            }
            "LoadCredential" => {
                if let Some((_id, path)) = v.split_once(':') {
                    out.credentials.push(path.to_string());
                }
            }
            "ExecStart" => {
                let cmd = v.trim_start_matches(['-', '+', '!', '@']);
                if let Some(first) = cmd.split_whitespace().next() {
                    out.binary = Some(first.to_string());
                }
            }
            _ => {}
        }
    }
    if dynamic_user {
        // systemd invents the account per start; creating one would be wrong.
        out.user = None;
    }
    out
}
