//! C7: native Rust services — bare binaries under systemd in their own LXC,
//! no docker layer. The homelab is the safety net their self-update cannot
//! be (keep the previous binary, arm a rollback from OUTSIDE the app), and
//! adoption lets it take over a container that was built by hand first —
//! Kenny's stated workflow: try a service outside the homelab, then have it
//! inlined without a restart.

use serde::{Deserialize, Serialize};

/// B1: who owns this service's recurring update. `manual` (the default) means
/// the nightly round never installs a release for it; `auto` means the
/// orchestrator fetches the latest release nightly, verifies its checksum
/// against the installed binary and installs it under the armed rollback
/// when it differs. Decided per service in UPDATE_POLICY.md: kyu and
/// kyu-runner are the orchestrator's; http-switchboard stays manual while it
/// sits on the alert path; almanac updates itself.
///
/// fix-58: `manual` means nothing runs, and a service that updates itself
/// through its own verb says so with `self`. Story:
/// `docs/deployment/REGISTER.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdatePolicy {
    #[default]
    Manual,
    Auto,
    /// The nightly round runs the service's own `update_cmd` under the armed
    /// rollback and never installs a release over it (almanac).
    #[serde(rename = "self")]
    OwnVerb,
}

/// fix-113 (owner decision, 2026-10-01): how a native service's store is
/// quiesced for the length of the nightly tar. Written in `service.yml` as
/// `false` (the default), `true` or `chassis` — not as a Rust-style tag —
/// so the (de)serialization is hand-rolled rather than `rename_all`.
///
/// `Off` and `Unit` are what `bool` used to mean before this field grew a
/// third state. `Chassis` asks the chassis-rs kit's own binary to pause
/// (`pct exec <ct> -- <binary> backup-pause --for <secs>`), which can hold
/// writes without stopping the process at all; `backup_native` falls back to
/// `Unit`'s stop/start when the binary cannot (see
/// `crate::ops::native::decide_chassis_pause`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackupPause {
    #[default]
    Off,
    /// The homelab stops the unit itself before the tar and starts it again
    /// after, whatever the snapshot did (the original fix-113 mechanism).
    Unit,
    /// The binary's own `backup-pause`/`backup-resume` subcommand.
    Chassis,
}

impl BackupPause {
    /// `skip_serializing_if`: the default (`false` in the file) need not be
    /// written at all, same as the plain `bool` this field used to be.
    pub fn is_off(&self) -> bool {
        matches!(self, BackupPause::Off)
    }
}

impl<'de> serde::Deserialize<'de> for BackupPause {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Bool(bool),
            Str(String),
        }
        // Accepted both as a real bool (the `service.yml` shape) and as the
        // string "false"/"true" (the dashboard's edit JSON, which sends
        // every field of a select as text — `admin/web/js/editpanels.js`'s
        // `nativeChoice`, the same way `update_policy` and `metrics` do).
        match Raw::deserialize(d)? {
            Raw::Bool(false) => Ok(BackupPause::Off),
            Raw::Bool(true) => Ok(BackupPause::Unit),
            Raw::Str(s) if s == "false" => Ok(BackupPause::Off),
            Raw::Str(s) if s == "true" => Ok(BackupPause::Unit),
            Raw::Str(s) if s == "chassis" => Ok(BackupPause::Chassis),
            Raw::Str(other) => Err(serde::de::Error::custom(format!(
                "backup_pause: '{}' is not one of false, true, chassis",
                other
            ))),
        }
    }
}

impl serde::Serialize for BackupPause {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            BackupPause::Off => s.serialize_bool(false),
            BackupPause::Unit => s.serialize_bool(true),
            BackupPause::Chassis => s.serialize_str("chassis"),
        }
    }
}

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
    /// app-knowledge (2026-09-30): what a person restoring this service must
    /// know that no generic step covers, printed in the DR runbook under the
    /// unit. almanac's note about retired profiles used to be written into
    /// the runbook generator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restore_note: Option<String>,
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
    /// T77 (D94): archive the newest file matching this glob instead of
    /// `data_dirs`. kyu writes an integrity-checked copy of its store every
    /// night (`VACUUM INTO`, re-opened and verified before it reports
    /// success); that copy is a better source than the live database, which
    /// is being written to while tar reads it (F172). The newest match must
    /// be fresh — older than 26 hours means the service's own copy did not
    /// run, and archiving a stale file would look exactly like success
    /// (M-D94). Restore note: such a copy is a COMPLETE database; put it
    /// back as the live file and delete any `-wal`/`-shm` beside it.
    #[serde(default)]
    pub backup_from_newest: Option<String>,
    /// fix-113 (native-tar-no-quiesce, 2026-09-27; extended 2026-10-01 for
    /// the chassis-rs kit's `backup-pause`/`backup-resume` subcommand, owner
    /// decision on fix-113): how the service is quiesced for the length of
    /// the tar. `false` (the default): no quiescing. `true`: the homelab
    /// stops the unit itself before the tar and starts it again afterwards,
    /// whatever the snapshot did — the original fix-113 mechanism. `chassis`:
    /// the binary's own `backup-pause`/`backup-resume` verbs do it, which can
    /// pause writes without a full stop; the homelab falls back to the `true`
    /// mechanism when the binary cannot (see [`BackupPause`]). Off by
    /// default; the stack file says which services need it (kyu has the
    /// better answer, `backup_from_newest`).
    #[serde(default, skip_serializing_if = "BackupPause::is_off")]
    pub backup_pause: BackupPause,
    /// B1: `auto` = the nightly round installs the latest release when its
    /// checksum differs from the installed binary; `manual` = never.
    #[serde(default)]
    pub update_policy: UpdatePolicy,
    /// `false` = this service is deliberately not measured by Prometheus
    /// (Kenny, Phase 9 form 2026-09-27, inbox on CT 118). The fleet check
    /// then reports it as `noted` instead of as drift. Absent = measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<bool>,
    /// fix-146 (native-empty-rebuild, 2026-10-01): a shell one-liner run
    /// INSIDE the container (`pct exec <vmid> -- sh -c "<after_restore>"`)
    /// once a snapshot is unpacked and before the unit starts — the step a
    /// person restoring this service would otherwise have to do by hand
    /// (docs/OPERATIONS_RUNBOOK.md op-11).
    ///
    /// Only a unit whose archive is not already its live store needs this —
    /// `backup_from_newest` (kyu) restores a copy, not the live file, so the
    /// copy has to be put in place before the service can use it. kyu's is
    /// the restore note that used to live only in a comment: rename the
    /// newest `kyu.backup-*.db` to `kyu.db` and drop any stale `-wal`/`-shm`
    /// beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_restore: Option<String>,
}

impl NativeServiceManifest {
    /// The release asset to fetch, falling back to the unit name.
    pub fn asset_name(&self) -> &str {
        self.release_asset.as_deref().unwrap_or(&self.unit)
    }

    /// Which of the two nightly update paths the scheduler runs for this
    /// service. Decided here rather than in the scheduler loop so it is a
    /// test instead of an assumption.
    pub fn nightly_updates(&self) -> NightlyUpdates {
        // fix-58: the policy gates both paths; `manual` gets neither.
        // fix-148: one mechanism per policy — `auto` is the signed release
        // update only (the kit's own verb does not check the ecosystem
        // signature, fix-29); the unit's own verb runs only under `self`.
        // Story: docs/deployment/REGISTER.md.
        NightlyUpdates {
            release: self.update_policy == UpdatePolicy::Auto,
            own_cmd: self.update_policy == UpdatePolicy::OwnVerb,
        }
    }
}

/// What the nightly round may do to one native service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NightlyUpdates {
    /// B1: the orchestrator's own release update (signature-checked).
    pub release: bool,
    /// The service's `update_cmd`, run under the armed rollback.
    pub own_cmd: bool,
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
            "hostname '{}' must be '{}' (the hostname guard depends on it)",
            m.hostname, canonical
        ));
    }
    for p in std::iter::once(&m.binary)
        .chain(m.env_file.iter())
        .chain(m.data_dirs.iter())
    {
        // shell-strings-quoting (2026-09-27): the same alphabet as a compose
        // stack's paths, not only "absolute and no `..`".
        if !crate::manifest::is_plain_abs_path(p) {
            problems.push(format!(
                "path '{}' must be absolute and made of letters, digits, '.', '_', '-' and \
                 single '/' separators, with no '..' segment",
                p
            ));
        }
    }
    if m.data_dirs.is_empty() && !m.stateless {
        problems.push(
            "data_dirs is empty — a service with no declared state cannot be backed up; \
             declare at least one directory, or set `stateless: true` if it genuinely \
             keeps none (a unit that runs under DynamicUser with no state directory)"
                .into(),
        );
    }
    // fix-113: pausing a service that keeps nothing would stop it nightly for
    // a backup that has nothing to take.
    if m.stateless && !m.backup_pause.is_off() {
        problems.push(
            "backup_pause is set on a stateless service — there is nothing to archive, so \
             nothing to pause for"
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
    if m.update_policy == UpdatePolicy::Auto && m.release_repo.is_none() {
        problems.push(
            "update_policy: auto without a release_repo — there is nothing to fetch nightly".into(),
        );
    }
    if m.update_policy == UpdatePolicy::OwnVerb && m.update_cmd.is_none() {
        problems.push(
            "update_policy: self without an update_cmd — there is no verb to run nightly".into(),
        );
    }
    if let Some(glob) = &m.backup_from_newest {
        if !glob.starts_with('/') || glob.contains("..") || !glob.contains('*') {
            problems.push(format!(
                "backup_from_newest '{}' must be an absolute glob with a '*' — it names the \
                 service's own rotating copies, not one fixed file",
                glob
            ));
        }
        if m.stateless {
            problems.push(
                "backup_from_newest is set on a stateless service — one of the two is wrong".into(),
            );
        }
    }
    // fix-146: an empty string would run as `pct exec <vmid> -- sh -c ""`,
    // which succeeds and seeds nothing — silently.
    if let Some(cmd) = &m.after_restore {
        if cmd.trim().is_empty() {
            problems.push("after_restore is set but empty — remove it or write the command".into());
        }
        if m.stateless {
            problems.push(
                "after_restore is set on a stateless service — a restore never unpacks \
                 anything for it to run against"
                    .into(),
            );
        }
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

/// fix-159 (2026-09-29): why a deploy restarts the running native `unit`
/// (its name without `.service`), given its unit file `unit_text` and the
/// container paths the deploy wrote this run: its unit file or a drop-in of
/// it ("unit changed"), a file it reads with `EnvironmentFile=`, optional
/// ones included ("env changed"), or with `LoadCredential=` ("credential
/// changed"). None: nothing the unit reads at its start changed, and a
/// running service is left alone, as adoption leaves it.
pub fn restart_reason(unit: &str, unit_text: &str, written: &[String]) -> Option<String> {
    let unit_path = format!("/etc/systemd/system/{}.service", unit);
    let dropins = format!("{}.d/", unit_path);
    let mut env_files: Vec<String> = Vec::new();
    let mut section = String::new();
    for line in unit_text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.to_string();
            continue;
        }
        if section != "[Service]" {
            continue;
        }
        if let Some(v) = t.strip_prefix("EnvironmentFile=") {
            let v = v.trim();
            env_files.push(v.strip_prefix('-').unwrap_or(v).to_string());
        }
    }
    let credentials = unit_prereqs(unit_text).credentials;
    let mut why: Vec<&str> = Vec::new();
    if written
        .iter()
        .any(|w| *w == unit_path || w.starts_with(&dropins))
    {
        why.push("unit changed");
    }
    if written.iter().any(|w| env_files.contains(w)) {
        why.push("env changed");
    }
    if written.iter().any(|w| credentials.contains(w)) {
        why.push("credential changed");
    }
    (!why.is_empty()).then(|| why.join(" and "))
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
