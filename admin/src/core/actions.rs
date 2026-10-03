//! feat-stacks-4 / feat-stacks-5 / feat-stacks-6: every action the CLI can
//! take on one stack (and the four host-wide ones a schedule may want), as
//! data, and the one pure step from "the browser asked for X on stack Y with
//! these arguments" to the exact `Command`s the CLI would send.
//!
//! The shell (`shell::actions`) reads what the command needs from disk (the
//! stack's manifest or deploy spec from the working copy) and sends what this
//! module built; nothing here does I/O.

use std::collections::BTreeMap;

use homelab_proto::{Command, DeploySpec, NativeServiceManifest, Scope, SecretRef, StackManifest};
use serde::{Deserialize, Serialize};

/// The target name of the four host-wide actions. Stack names are
/// `[a-z0-9-]`, so it can never be a stack.
pub const HOST_TARGET: &str = "_host";

/// arch-self: the stack the dashboard itself runs as.
pub const SELF_STACK: &str = "admin";

/// An API refusal, `{what, why, fix}` on the wire (arch-errors).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{what}: {why} ({fix})")]
pub struct Refusal {
    pub what: String,
    pub why: String,
    pub fix: String,
}

impl Refusal {
    pub fn new(what: impl Into<String>, why: impl Into<String>, fix: impl Into<String>) -> Self {
        Refusal {
            what: what.into(),
            why: why.into(),
            fix: fix.into(),
        }
    }
}

/// What the browser (or a schedule) can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    Deploy,
    /// feat-stacks-6: deploy the stack's files as they were at an earlier
    /// commit (the CLI equivalent is a checkout of that commit, then deploy).
    DeployCommit,
    Backup,
    Restore,
    /// feat-backup-2: the native-unit twin of `Restore` (a native stack has
    /// no `lxc-compose.yml` manifest to read, so it is its own action with
    /// its own `Needs`, like `BackupNative` beside `Backup`).
    RestoreNative,
    /// fix-237: restore one snapshot into the restore drill's scratch
    /// directory, judge it, report files and size, empty the scratch again;
    /// the live data is never a target.
    VerifyRestore,
    Update,
    Resize,
    Enable,
    Disable,
    Adopt,
    BackupNative,
    UpdateNative,
    ReleaseUpdateNative,
    /// feat-stacks-6: the host's kept previous binary of a native unit.
    RollbackNative,
    Guards,
    PruneOrphans,
    Destroy,
    Forget,
    Wipe,
    // Host-wide: target `_host`.
    Patch,
    ZfsReplicate,
    BackupHostMeta,
    BackupDevices,
    // TUI parity round (Kenny, 2026-09-28: "Wat nu in de TUI kan, moet nog
    // altijd kunnen in ons systeem").
    /// Host-wide: one shell command in a container (A6, the TUI's SHELL tab).
    Exec,
    /// Host-wide: the runaway guards on any container by its number.
    GuardsCt,
    /// Host-wide: bake the golden template (B8, dash-template).
    TemplateBuild,
    /// Host-wide: release-update of the host (dash-host-update).
    UpdateHost,
    /// Host-wide: a person's answer to a manual check (G17).
    AnswerCheck,
    /// A chosen release of a native service, downloaded by the host
    /// (dash-install-native).
    InstallNative,
    /// Host-wide: the whole stacks directory against the host (ask-8,
    /// dash-apply): deploy what changed, destroy only what was typed.
    Apply,
    /// Owner decision 2026-09-30 (item 2): host-wide, "Save and restart
    /// the host" on the host-settings form and its own dashboard action —
    /// restarts `homelab-host.service` so a `host.toml` change marked
    /// `Apply::Restart` takes effect without a second host update.
    RestartHost,
    /// feat-secrets-2: change one secret, writing through latch. The new
    /// value never travels as an `Arg` (nothing a job's history or a
    /// "copy as CLI command" preview would then carry it) — the shell
    /// resolves it from a short-lived stage by its one-time token, straight
    /// into the `Material` this command is built from. See
    /// `admin::shell::secrets`.
    ChangeSecret,
}

/// What the shell has to read before the command can be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Needs {
    /// The stack name is enough.
    Nothing,
    /// `lxc-compose.yml` from the working copy (no secrets, no latch).
    Manifest,
    /// The whole deploy spec: files, secrets from latch, native binaries.
    Spec,
    /// The stack's `service.yml` (adopt).
    NativeManifest,
    /// The stack's vmid, from the host's fleet.
    Vmid,
    /// The host's release, downloaded from GitHub and verified (the
    /// dashboard's "Update host").
    HostRelease,
    /// A native unit's service.yml and unit file from the working copy, and
    /// the release tag (the latest one when none is named).
    NativeRelease,
    /// Every stack's spec and the plan against the host (apply).
    Apply,
}

/// Which argument fields an action reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Arg {
    Force,
    Confirm,
    Snapshot,
    App,
    Unit,
    SkipBackup,
    SkipSafetyCopy,
    Commit,
    /// A container number (exec, guards on any container, template build).
    Vmid,
    /// One shell command line (exec).
    Command,
    /// A release tag, e.g. v3.63.0; empty: the latest.
    Tag,
    /// The template's version number.
    Version,
    /// Build the privileged template.
    Privileged,
    /// The base OS template (a vztmpl path); empty: the host's default.
    Base,
    /// A manual check's id.
    Check,
    /// ok, nok or accept.
    Verdict,
    /// For accept: how many days.
    Days,
    /// The answer's note (the reason, for accept).
    Note,
    /// apply: the gone stacks to destroy, each name typed, comma-separated.
    Destroy,
    /// feat-secrets-1/2: which secret (`SecretRef`, as JSON), from the
    /// Secrets page's own list.
    SecretRef,
    /// feat-secrets-2: the one-time token the staging endpoint returned for
    /// the new value (`POST /data/secrets/{stack}/stage`) — never the
    /// value itself.
    StageToken,
}

/// One row of the catalog the page draws its buttons from.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub action: ActionKind,
    /// `stack` or `host`.
    pub target: &'static str,
    pub label: &'static str,
    pub what: &'static str,
    /// The token scope the host demands (arch-tokens).
    pub scope: Scope,
    pub needs: Needs,
    pub args: &'static [Arg],
    /// The stack name must be typed into `confirm`.
    pub confirm: bool,
    /// Refused for the dashboard's own stack (arch-self).
    pub refused_for_self: bool,
    /// fix-255: deletes or overwrites something with no way back; the only
    /// thing the page paints red (never the token scope).
    pub destructive: bool,
}

impl ActionKind {
    pub const ALL: &'static [ActionKind] = &[
        ActionKind::Deploy,
        ActionKind::DeployCommit,
        ActionKind::Backup,
        ActionKind::Restore,
        ActionKind::RestoreNative,
        ActionKind::VerifyRestore,
        ActionKind::Update,
        ActionKind::Resize,
        ActionKind::Enable,
        ActionKind::Disable,
        ActionKind::Adopt,
        ActionKind::BackupNative,
        ActionKind::UpdateNative,
        ActionKind::ReleaseUpdateNative,
        ActionKind::RollbackNative,
        ActionKind::Guards,
        ActionKind::PruneOrphans,
        ActionKind::Destroy,
        ActionKind::Forget,
        ActionKind::Wipe,
        ActionKind::Patch,
        ActionKind::ZfsReplicate,
        ActionKind::BackupHostMeta,
        ActionKind::BackupDevices,
        ActionKind::Exec,
        ActionKind::GuardsCt,
        ActionKind::TemplateBuild,
        ActionKind::UpdateHost,
        ActionKind::AnswerCheck,
        ActionKind::InstallNative,
        ActionKind::Apply,
        ActionKind::RestartHost,
        ActionKind::ChangeSecret,
    ];

    /// The name in a URL: `deploy`, `backup-native`, ...
    pub fn slug(self) -> &'static str {
        use ActionKind::*;
        match self {
            Deploy => "deploy",
            DeployCommit => "deploy-commit",
            Backup => "backup",
            Restore => "restore",
            RestoreNative => "restore-native",
            VerifyRestore => "verify-restore",
            Update => "update",
            Resize => "resize",
            Enable => "enable",
            Disable => "disable",
            Adopt => "adopt",
            BackupNative => "backup-native",
            UpdateNative => "update-native",
            ReleaseUpdateNative => "release-update-native",
            RollbackNative => "rollback-native",
            Guards => "guards",
            PruneOrphans => "prune-orphans",
            Destroy => "destroy",
            Forget => "forget",
            Wipe => "wipe",
            Patch => "patch",
            ZfsReplicate => "zfs-replicate",
            BackupHostMeta => "backup-host-meta",
            BackupDevices => "backup-devices",
            Exec => "exec",
            GuardsCt => "guards-ct",
            TemplateBuild => "template-build",
            UpdateHost => "update-host",
            AnswerCheck => "answer-check",
            InstallNative => "install-native",
            Apply => "apply",
            RestartHost => "restart-host",
            ChangeSecret => "change-secret",
        }
    }

    pub fn from_slug(s: &str) -> Option<ActionKind> {
        ActionKind::ALL.iter().copied().find(|k| k.slug() == s)
    }

    pub fn host_wide(self) -> bool {
        use ActionKind::*;
        matches!(
            self,
            Patch
                | ZfsReplicate
                | BackupHostMeta
                | BackupDevices
                | Exec
                | GuardsCt
                | TemplateBuild
                | UpdateHost
                | AnswerCheck
                | Apply
                | RestartHost
        )
    }

    pub fn needs(self) -> Needs {
        use ActionKind::*;
        match self {
            Deploy | DeployCommit | PruneOrphans => Needs::Spec,
            Backup | Restore | Update | Resize => Needs::Manifest,
            // A destroy reads the manifest when the directory is there and
            // falls back to the host's record (DestroyRecorded) when not.
            Destroy => Needs::Manifest,
            Adopt | RestoreNative => Needs::NativeManifest,
            Guards => Needs::Vmid,
            UpdateHost => Needs::HostRelease,
            InstallNative => Needs::NativeRelease,
            Apply => Needs::Apply,
            _ => Needs::Nothing,
        }
    }

    pub fn args(self) -> &'static [Arg] {
        use ActionKind::*;
        match self {
            Deploy => &[Arg::Force],
            DeployCommit => &[Arg::Commit, Arg::Force],
            Restore => &[Arg::Confirm, Arg::App, Arg::Snapshot, Arg::SkipSafetyCopy],
            RestoreNative => &[Arg::Confirm, Arg::Snapshot, Arg::Unit],
            VerifyRestore => &[Arg::App, Arg::Snapshot],
            ChangeSecret => &[Arg::SecretRef, Arg::StageToken],
            Update => &[Arg::App],
            RollbackNative => &[Arg::Unit],
            PruneOrphans | Forget => &[Arg::Confirm],
            Destroy => &[Arg::Confirm, Arg::SkipBackup],
            // Without `confirm` a wipe only lists what it would delete.
            Wipe => &[Arg::Confirm],
            Exec => &[Arg::Vmid, Arg::Command],
            GuardsCt => &[Arg::Vmid],
            TemplateBuild => &[Arg::Vmid, Arg::Version, Arg::Privileged, Arg::Base],
            UpdateHost => &[Arg::Tag],
            AnswerCheck => &[Arg::Check, Arg::Verdict, Arg::Days, Arg::Note],
            InstallNative => &[Arg::Unit, Arg::Tag],
            Apply => &[Arg::SkipBackup, Arg::Destroy, Arg::Force],
            _ => &[],
        }
    }

    /// Must the stack name be typed? (Wipe asks only for the real run.)
    pub fn confirm(self) -> bool {
        use ActionKind::*;
        matches!(
            self,
            Restore | RestoreNative | PruneOrphans | Destroy | Forget
        )
    }

    /// fix-255 (design review, 2026-10-03): what can delete something
    /// with no way back: a container and its record, a retired stack's
    /// kept data, files the repository dropped, or whatever one shell
    /// command does. Red on the page means this and only this; the token
    /// scope (`scope`) is about who may ask, not about what is lost —
    /// "Update the host", "Apply repository" and "Restart the host" need
    /// full access and lose nothing (Apply destroys a stack only when its
    /// name is typed in its own dialog). Restore keeps a copy of the
    /// current data first.
    pub fn destructive(self) -> bool {
        use ActionKind::*;
        matches!(self, PruneOrphans | Destroy | Forget | Wipe | Exec)
    }

    /// arch-self: what the dashboard never does to its own stack.
    pub fn refused_for_self(self) -> bool {
        use ActionKind::*;
        matches!(self, Destroy | Forget | Wipe)
    }

    /// feat-retired-1: this action's target is a `HostState::retired` key
    /// (`stack`, or `stack/app`, `stack/unit`), never a live stack — the
    /// whole point of `Wipe` is to act on something that left the fleet.
    /// Everywhere else "the stack" means a managed one; this is the one
    /// exception both `validate` (the key format) and the Live-view driver
    /// (`drive::Applied` for `UiStep::Open`, which otherwise refuses a
    /// target absent from the fleet) need to know about.
    pub fn targets_retired(self) -> bool {
        self == ActionKind::Wipe
    }

    pub fn label(self) -> &'static str {
        use ActionKind::*;
        match self {
            Deploy => "Deploy",
            DeployCommit => "Deploy a commit",
            Backup => "Back up",
            Restore => "Restore",
            RestoreNative => "Restore (native)",
            VerifyRestore => "Verify restore",
            Update => "Update",
            Resize => "Resize",
            // feat-shell-1 (3.71.0, the approved renames, FLOWS.md §1.4):
            // Park / Unpark for Disable / Enable, Add log guards for Apply
            // guards, Deploy all changes for Apply repository.
            Enable => "Unpark",
            Disable => "Park",
            Adopt => "Adopt",
            BackupNative => "Back up (native)",
            UpdateNative => "Update (native)",
            ReleaseUpdateNative => "Install newest",
            RollbackNative => "Roll back binary",
            Guards => "Add log guards",
            PruneOrphans => "Prune orphans",
            Destroy => "Destroy",
            Forget => "Forget",
            Wipe => "Wipe",
            Patch => "Patch the fleet",
            ZfsReplicate => "ZFS replicate",
            BackupHostMeta => "Back up host state",
            BackupDevices => "Back up devices",
            Exec => "Run a command",
            GuardsCt => "Guard a container",
            TemplateBuild => "Build a template",
            UpdateHost => "Update the host",
            AnswerCheck => "Answer a check",
            InstallNative => "Install a release",
            Apply => "Deploy all changes",
            RestartHost => "Restart the host",
            ChangeSecret => "Change a secret",
        }
    }

    /// The CLI help's own sentence for the verb, so both say the same.
    pub fn what(self) -> &'static str {
        use ActionKind::*;
        match self {
            Deploy => {
                "create or reconcile the container; what the files no longer declare is removed, data stays"
            }
            DeployCommit => {
                "deploy the stack's files as they were at an earlier commit of the working copy"
            }
            Backup => "a restic snapshot of the stack now",
            Restore => {
                "restore the stack's data from a snapshot ('latest' unless named); keeps a copy of the current data first"
            }
            RestoreNative => {
                "restore an adopted service's data from a snapshot ('latest' unless named): stops the unit, keeps a copy of the current data first, unpacks the archive, restarts it; on a multi-unit stack, choose which unit (all units otherwise)"
            }
            VerifyRestore => {
                "prove a snapshot restorable without touching live data: restore it into the restore drill's scratch directory, judge it as the nightly drill does, report the files and the size, empty the scratch again (every app's latest when no app is picked)"
            }
            Update => "pull and recreate one app or all, with rollback",
            Resize => "apply the manifest's memory, cores and disk to the running container",
            Enable => "take the stack back into the nightly backup and update, and start-on-boot",
            Disable => {
                "park the stack: no nightly backup or update, start-on-boot cleared; no container is stopped"
            }
            Adopt => {
                "take over a hand-built container described by its service.yml; restarts nothing"
            }
            BackupNative => "back up an adopted service now",
            UpdateNative => "update an adopted service the way its own update policy says",
            ReleaseUpdateNative => "install the newest release of each service in the stack",
            RollbackNative => {
                "put a native service's previous binary back and park the stack's updates"
            }
            Guards => {
                "apply the runaway guards: log caps, journald limits, logrotate, a weekly prune"
            }
            PruneOrphans => "remove files the repository dropped, without a deploy",
            Destroy => {
                "back up, then destroy the container; from the host's record when the directory is gone"
            }
            Forget => "for a container already gone: drop its record and registrations",
            Wipe => {
                "delete what a retired stack kept: backups, /appdata, vault copies (without confirm: list only)"
            }
            Patch => "apt update and dist-upgrade every managed container, one at a time",
            ZfsReplicate => "run the ZFS snapshot and replication jobs now",
            BackupHostMeta => {
                "snapshot the daemon's own state now: vault, state, TLS, intent repository"
            }
            BackupDevices => "fetch each configured device's own configuration now",
            Exec => {
                "run one shell command in a container (pct exec), audit-logged on the host; the host refuses unless exec_enabled = true in host.toml"
            }
            GuardsCt => {
                "apply the runaway guards to any container by its number: log caps, journald limits, logrotate, a weekly prune"
            }
            TemplateBuild => {
                "bake the golden template (docker and the guards) on a temporary container, then turn it into a Proxmox template"
            }
            UpdateHost => {
                "install a signed homelab release on the host: the dashboard downloads it and checks its signature and checksum, the host installs it with an armed rollback and restarts, and the dashboard reconnects"
            }
            AnswerCheck => {
                "record the answer to a manual check: ok, not ok, or a not ok accepted for some days with its reason"
            }
            InstallNative => {
                "install a chosen release of a native service: the host downloads it, checks its signature and checksum, and installs it with an armed rollback (the latest release when no tag is named)"
            }
            Apply => {
                "deploy every stack whose files differ from what the host applied; a stack whose directory is gone is destroyed only when its name is typed"
            }
            RestartHost => "restarts the host daemon; running jobs are refused while a job runs",
            ChangeSecret => {
                "write one secret through latch (one .env or one latch_files entry); the other files latch holds for this stack are untouched; redeploy to apply it to the running container"
            }
        }
    }

    /// The host's scope for the command this action sends.
    pub fn scope(self) -> Scope {
        use ActionKind::*;
        match self {
            PruneOrphans | Destroy | Forget | Wipe | Exec | UpdateHost | Apply | RestartHost
            | ChangeSecret => Scope::All,
            _ => Scope::Operate,
        }
    }

    pub fn catalog_entry(self) -> CatalogEntry {
        CatalogEntry {
            action: self,
            target: if self.host_wide() { "host" } else { "stack" },
            label: self.label(),
            what: self.what(),
            scope: self.scope(),
            needs: self.needs(),
            args: self.args(),
            confirm: self.confirm(),
            refused_for_self: self.refused_for_self(),
            destructive: self.destructive(),
        }
    }
}

/// The catalog, in the order the page lists it.
pub fn catalog() -> Vec<CatalogEntry> {
    ActionKind::ALL.iter().map(|k| k.catalog_entry()).collect()
}

/// The JSON body of `POST /data/actions/{stack}/{action}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionArgs {
    /// arch-deploy-guard: deploy over a commit this working copy lacks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
    /// The stack name, typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_backup: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_safety_copy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// A container number, as typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vmid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub privileged: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub days: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// apply: the names typed, comma-separated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destroy: Option<String>,
    /// feat-secrets-2: which secret, as `SecretRef` JSON (the Secrets page
    /// sends back exactly what it listed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<String>,
    /// feat-secrets-2: the one-time staging token for the new value — never
    /// the value (arch-secrets-no-args, this module's own doc comment on
    /// `ActionKind::ChangeSecret`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_token: Option<String>,
    /// redesign-stacks-6: back the stack up before the deploy changes it
    /// (`DeploySpec::backup_first`). Never read from a request body: the
    /// server sets it for every deploy of a batch (`validate_batch`), the
    /// one place the page promises "each backed up first"; Apply derives
    /// it from `skip_backup` instead.
    #[serde(skip)]
    pub backup_first: bool,
}

impl ActionArgs {
    /// The argument fields that are set.
    fn set(&self) -> Vec<Arg> {
        let mut v = Vec::new();
        if self.force {
            v.push(Arg::Force);
        }
        if self.confirm.is_some() {
            v.push(Arg::Confirm);
        }
        if self.snapshot.is_some() {
            v.push(Arg::Snapshot);
        }
        if self.app.is_some() {
            v.push(Arg::App);
        }
        if self.unit.is_some() {
            v.push(Arg::Unit);
        }
        if self.skip_backup {
            v.push(Arg::SkipBackup);
        }
        if self.skip_safety_copy {
            v.push(Arg::SkipSafetyCopy);
        }
        if self.commit.is_some() {
            v.push(Arg::Commit);
        }
        let opt = [
            (self.vmid.is_some(), Arg::Vmid),
            (self.command.is_some(), Arg::Command),
            (self.tag.is_some(), Arg::Tag),
            (self.version.is_some(), Arg::Version),
            (self.privileged, Arg::Privileged),
            (self.base.is_some(), Arg::Base),
            (self.check.is_some(), Arg::Check),
            (self.verdict.is_some(), Arg::Verdict),
            (self.days.is_some(), Arg::Days),
            (self.note.is_some(), Arg::Note),
            (self.destroy.is_some(), Arg::Destroy),
            (self.secret_ref.is_some(), Arg::SecretRef),
            (self.stage_token.is_some(), Arg::StageToken),
        ];
        v.extend(opt.into_iter().filter(|(on, _)| *on).map(|(_, a)| a));
        v
    }

    /// The container number, parsed.
    pub fn vmid_number(&self) -> Option<u16> {
        self.vmid.as_deref().and_then(|v| v.trim().parse().ok())
    }

    /// apply: the typed names, in the order given.
    pub fn destroy_names(&self) -> Vec<String> {
        self.destroy
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect()
    }
}

/// One requested action, as validated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRequest {
    pub stack: String,
    pub action: ActionKind,
    #[serde(default)]
    pub args: ActionArgs,
}

pub fn valid_stack_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
}

/// feat-retired-1: a `Wipe` target — a stack name on its own (a whole stack
/// retired by destroy or forget), or `stack/name` (an app or native unit
/// retired out of a stack that still exists), exactly the two shapes
/// `HostState::retired` keys on (`ops::retired::retire_stack/app/unit`).
pub fn valid_retired_key(s: &str) -> bool {
    match s.split_once('/') {
        Some((stack, name)) => valid_stack_name(stack) && valid_word(name),
        None => valid_stack_name(s),
    }
}

fn valid_word(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn valid_commit(s: &str) -> bool {
    (7..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Everything that can be said about a request before anything is read:
/// the target, the arguments this action takes, the typed name, and
/// arch-self. The order of checks is the order a person fixes them in.
pub fn validate(stack: &str, action: &str, args: ActionArgs) -> Result<ActionRequest, Refusal> {
    let Some(kind) = ActionKind::from_slug(action) else {
        return Err(Refusal::new(
            format!("action {action:?}"),
            "the dashboard knows no such action",
            "GET /data/actions/catalog lists every action it can run",
        ));
    };
    if kind.host_wide() {
        if stack != HOST_TARGET {
            return Err(Refusal::new(
                format!("{} on {stack}", kind.slug()),
                "this action is for the whole host, not one stack",
                format!("send it to /data/actions/{HOST_TARGET}/{}", kind.slug()),
            ));
        }
    } else if kind.targets_retired() {
        if !valid_retired_key(stack) {
            return Err(Refusal::new(
                format!("{} on {stack:?}", kind.slug()),
                "a retired key is a stack name, or stack/app, stack/unit",
                "pick the key from the Retired page, or the stack's own Wipe button",
            ));
        }
    } else if !valid_stack_name(stack) {
        return Err(Refusal::new(
            format!("{} on {stack:?}", kind.slug()),
            "a stack name is lowercase letters, digits and dashes",
            "use the name the fleet page shows",
        ));
    }
    let what = format!("{} {}", kind.slug(), stack);
    let stray: Vec<Arg> = args
        .set()
        .into_iter()
        .filter(|a| !kind.args().contains(a))
        .collect();
    if !stray.is_empty() {
        let names: Vec<String> = stray
            .iter()
            .map(|a| {
                serde_json::to_value(a)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default()
            })
            .collect();
        return Err(Refusal::new(
            what,
            format!("{} does not take {}", kind.slug(), names.join(", ")),
            "send only the arguments the catalog lists for this action",
        ));
    }
    if stack == SELF_STACK && kind.refused_for_self() {
        return Err(Refusal::new(
            what,
            "the dashboard does not destroy, forget or wipe its own stack (arch-self)",
            "use the CLI for that: `homelab destroy admin` from a workstation",
        ));
    }
    let needs_name = kind.confirm() || (kind == ActionKind::Wipe && args.confirm.is_some());
    if needs_name && args.confirm.as_deref() != Some(stack) {
        return Err(Refusal::new(
            what,
            "the typed name does not match the stack",
            format!("type {stack:?} exactly to confirm"),
        ));
    }
    if let Some(s) = &args.snapshot
        && !valid_word(s)
    {
        return Err(Refusal::new(
            what,
            format!("{s:?} is not a snapshot id"),
            "pick a snapshot from the backup page, or leave it out for 'latest'",
        ));
    }
    for (field, v) in [("app", &args.app), ("unit", &args.unit)] {
        if let Some(v) = v
            && !valid_word(v)
        {
            return Err(Refusal::new(
                what,
                format!("{v:?} is not a valid {field} name"),
                format!("use the {field} name the stack page shows"),
            ));
        }
    }
    if kind == ActionKind::DeployCommit {
        match &args.commit {
            Some(c) if valid_commit(c) => {}
            _ => {
                return Err(Refusal::new(
                    what,
                    "deploy-commit needs the commit to deploy (7 to 40 hex characters)",
                    "pick one from GET /data/actions/{stack}/rollback-options",
                ));
            }
        }
    }
    validate_parity(kind, &what, &args)?;
    Ok(ActionRequest {
        stack: stack.to_string(),
        action: kind,
        args,
    })
}

/// A release tag as the dashboard passes it on (`v3.63.0`).
pub fn valid_tag(s: &str) -> bool {
    homelab_core::ops::native::valid_tag(s)
}

/// A base OS template as `pveam` names it
/// (`local:vztmpl/debian-13-standard_13.1-2_amd64.tar.zst`).
fn valid_base(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'/'))
        && !s.contains("..")
}

/// The longest command line the exec form takes.
pub const EXEC_MAX: usize = 4096;
/// The longest note an answer takes.
pub const NOTE_MAX: usize = 500;

/// The TUI parity round's arguments (exec, guards on any container, the
/// template build, the host update, a check's answer, install-native,
/// apply), checked before anything is read.
fn validate_parity(kind: ActionKind, what: &str, args: &ActionArgs) -> Result<(), Refusal> {
    use ActionKind::*;
    let refuse = |why: String, fix: &str| Err(Refusal::new(what, why, fix));
    if kind.args().contains(&Arg::Vmid) {
        match args.vmid.as_deref().map(str::trim) {
            None | Some("") => {
                return refuse(
                    format!("{} needs the container's number", kind.slug()),
                    "type the vmid, e.g. 105",
                );
            }
            Some(v) => match v.parse::<u16>() {
                Ok(n) if n >= 100 => {}
                _ => {
                    return refuse(
                        format!("{v:?} is not a container number (100 or more)"),
                        "type the vmid the host page lists, e.g. 105",
                    );
                }
            },
        }
    }
    if kind == Exec {
        let c = args.command.as_deref().unwrap_or("").trim();
        if c.is_empty() {
            return refuse(
                "exec needs a command".into(),
                "type the command line to run, e.g. df -h",
            );
        }
        if c.len() > EXEC_MAX || c.contains(['\n', '\r', '\0']) {
            return refuse(
                format!("a command is one line of at most {EXEC_MAX} characters"),
                "send one line at a time; the shell page runs them one after the other",
            );
        }
    }
    if let Some(t) = args.tag.as_deref().map(str::trim).filter(|t| !t.is_empty())
        && !valid_tag(t)
    {
        return refuse(
            format!("{t:?} is not a release tag"),
            "type a tag such as v3.63.0, or leave it empty for the latest release",
        );
    }
    if kind == TemplateBuild {
        match args
            .version
            .as_deref()
            .map(str::trim)
            .map(str::parse::<u32>)
        {
            Some(Ok(v)) if (1..=999).contains(&v) => {}
            _ => {
                return refuse(
                    "the template's version is a whole number from 1 to 999".into(),
                    "type the next version, e.g. 5 when debian-13-homelab-v4 is the newest",
                );
            }
        }
        if let Some(b) = args
            .base
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            && !valid_base(b)
        {
            return refuse(
                format!("{b:?} is not an OS template"),
                "pick one from the templates list, or leave it empty for the host's default",
            );
        }
    }
    if kind == AnswerCheck {
        let id = args.check.as_deref().unwrap_or("").trim();
        if !valid_word(id) {
            return refuse(
                "the answer needs the check's id".into(),
                "pick the check on the checks page",
            );
        }
        let note = args.note.as_deref().unwrap_or("").trim();
        if note.len() > NOTE_MAX || note.contains(['\n', '\r']) {
            return refuse(
                format!("a note is one line of at most {NOTE_MAX} characters"),
                "shorten the note",
            );
        }
        match args.verdict.as_deref().map(str::trim) {
            Some("ok") | Some("nok") => {
                if args.days.as_deref().is_some_and(|d| !d.trim().is_empty()) {
                    return refuse(
                        "days go with accept only".into(),
                        "leave the days empty, or answer accept",
                    );
                }
            }
            Some("accept") => {
                match args.days.as_deref().map(str::trim).map(str::parse::<u32>) {
                    Some(Ok(d)) if (1..=3650).contains(&d) => {}
                    _ => {
                        return refuse(
                            "accept needs the number of days (1 to 3650)".into(),
                            "type for how many days the not ok is accepted",
                        );
                    }
                }
                if note.is_empty() {
                    return refuse(
                        "accept needs the reason in the note".into(),
                        "say in the note why the not ok is accepted",
                    );
                }
            }
            _ => {
                return refuse(
                    "the answer is ok, nok or accept".into(),
                    "pick one of the three",
                );
            }
        }
    }
    if kind == ChangeSecret {
        let sref = args.secret_ref.as_deref().unwrap_or("");
        if serde_json::from_str::<SecretRef>(sref).is_err() {
            return refuse(
                "change-secret needs secret_ref: the exact entry the Secrets page listed".into(),
                "pick the secret from the stack's Secrets page; do not type it by hand",
            );
        }
        let token = args.stage_token.as_deref().unwrap_or("");
        if !valid_word(token) {
            return refuse(
                "change-secret needs stage_token: the id POST /data/secrets/{stack}/stage \
                 returned for the new value"
                    .into(),
                "stage the new value first, then send the token it returns",
            );
        }
    }
    if kind == Apply {
        let names = args.destroy_names();
        let mut seen = std::collections::BTreeSet::new();
        for n in &names {
            if !valid_stack_name(n) {
                return refuse(
                    format!("{n:?} is not a stack name"),
                    "type the names the plan lists under 'gone from the files', comma-separated",
                );
            }
            if n == SELF_STACK {
                return refuse(
                    "the dashboard does not destroy its own stack (arch-self)".into(),
                    "leave admin out; `homelab destroy admin` from a workstation",
                );
            }
            if !seen.insert(n.clone()) {
                return refuse(format!("{n} is typed twice"), "type each name once");
            }
        }
    }
    Ok(())
}

/// What the shell read for the command.
#[derive(Debug, Clone)]
pub enum Material {
    None,
    Manifest(Box<StackManifest>),
    Spec(Box<DeploySpec>),
    Native(Box<NativeServiceManifest>),
    Vmid(u16),
    /// The host binary of release `tag`, verified, base64.
    HostRelease {
        tag: String,
        binary_b64: String,
        /// redesign-host-4: the signed checksum list it came with.
        proof: Option<homelab_proto::ReleaseProof>,
    },
    /// install-native: the unit's manifest and unit file from the working
    /// copy, the tag to install, and the directory the CLI names
    /// (`stacks/kyu` or `stacks/kyu/kyu-runner`).
    NativeRelease {
        manifest: Box<NativeServiceManifest>,
        unit_file: String,
        tag: String,
        dir: String,
    },
    /// apply: the specs to deploy, in order, and the gone stacks to destroy.
    Apply {
        deploy: Vec<DeploySpec>,
        destroy: Vec<String>,
    },
    /// feat-secrets-2: which secret, and its new value — resolved by the
    /// shell from the staged token right before the command is built, held
    /// only for that moment (never part of `ActionArgs`/`JobView`).
    Secret {
        secret: SecretRef,
        content: String,
    },
}

fn wrong_material(req: &ActionRequest) -> Refusal {
    Refusal::new(
        format!("{} {}", req.action.slug(), req.stack),
        "the stack's files were not read (internal)",
        "report this with the dashboard's log",
    )
}

fn name_mismatch(req: &ActionRequest, found: &str) -> Refusal {
    Refusal::new(
        format!("{} {}", req.action.slug(), req.stack),
        format!("the stack files in the working copy name the stack {found:?}"),
        "fix stack_name in the stack's lxc-compose.yml",
    )
}

/// The commands, in order, the CLI would send for this request. A deploy of
/// a stack with native programs stages each binary first (T85) and then
/// sends the spec with those entries emptied, exactly as `homelab deploy`.
pub fn commands(req: &ActionRequest, material: Material) -> Result<Vec<Command>, Refusal> {
    use ActionKind::*;
    let stack = req.stack.clone();
    let a = &req.args;
    let manifest = |m: &StackManifest| -> Result<Box<StackManifest>, Refusal> {
        if m.stack_name != req.stack {
            return Err(name_mismatch(req, &m.stack_name));
        }
        Ok(Box::new(m.clone()))
    };
    Ok(match (req.action, material) {
        (Deploy | DeployCommit, Material::Spec(spec)) => {
            if spec.manifest.stack_name != req.stack {
                return Err(name_mismatch(req, &spec.manifest.stack_name));
            }
            let mut spec = *spec;
            spec.backup_first = a.backup_first;
            deploy_commands(spec)
        }
        (Backup, Material::Manifest(m)) => vec![Command::BackupStack(manifest(&m)?)],
        (RestoreNative, Material::Native(m)) => {
            if m.stack_name != req.stack {
                return Err(name_mismatch(req, &m.stack_name));
            }
            vec![Command::RestoreNative {
                stack,
                snapshot: a.snapshot.clone().unwrap_or_else(|| "latest".into()),
                confirm: a.confirm.clone(),
                unit: a.unit.clone(),
            }]
        }
        (ChangeSecret, Material::Secret { secret, content }) => vec![Command::SetSecret {
            stack,
            secret,
            content,
        }],
        (Restore, Material::Manifest(m)) => vec![Command::RestoreStack {
            manifest: manifest(&m)?,
            snapshot: a.snapshot.clone().unwrap_or_else(|| "latest".into()),
            confirm: a.confirm.clone(),
            skip_safety_copy: a.skip_safety_copy,
            app: a.app.clone(),
        }],
        (VerifyRestore, _) => vec![Command::VerifyRestore {
            stack,
            app: a.app.clone(),
            snapshot: a.snapshot.clone().unwrap_or_else(|| "latest".into()),
        }],
        (Update, Material::Manifest(m)) => vec![Command::UpdateStack {
            manifest: manifest(&m)?,
            app: a.app.clone(),
        }],
        (Resize, Material::Manifest(m)) => vec![Command::ApplyResources(manifest(&m)?)],
        (Enable, _) => vec![Command::SetStackEnabled {
            stack,
            enabled: true,
        }],
        (Disable, _) => vec![Command::SetStackEnabled {
            stack,
            enabled: false,
        }],
        (Adopt, Material::Native(m)) => {
            if m.stack_name != req.stack {
                return Err(name_mismatch(req, &m.stack_name));
            }
            vec![Command::AdoptService(m)]
        }
        (BackupNative, _) => vec![Command::BackupNative { stack }],
        (UpdateNative, _) => vec![Command::UpdateNative { stack }],
        (ReleaseUpdateNative, _) => vec![Command::ReleaseUpdateNative { stack }],
        (RollbackNative, _) => vec![Command::RollbackNative {
            stack,
            unit: a.unit.clone(),
        }],
        (Guards, Material::Vmid(vmid)) => vec![Command::ApplyGuards { vmid }],
        (PruneOrphans, Material::Spec(spec)) => {
            if spec.manifest.stack_name != req.stack {
                return Err(name_mismatch(req, &spec.manifest.stack_name));
            }
            vec![Command::PruneOrphans {
                manifest: Box::new(spec.manifest.clone()),
                spec,
                confirm: stack,
            }]
        }
        (Destroy, Material::Manifest(m)) => vec![Command::DestroyStack {
            manifest: manifest(&m)?,
            confirm: stack,
            skip_backup: a.skip_backup,
        }],
        // ask-8: no directory, so from the manifest the host recorded.
        (Destroy, Material::None) => vec![Command::DestroyRecorded {
            confirm: stack.clone(),
            stack,
            skip_backup: a.skip_backup,
        }],
        (Forget, _) => vec![Command::ForgetStack { stack }],
        (Wipe, _) => vec![Command::WipeRetired {
            name: stack,
            confirm: a.confirm.clone(),
        }],
        (Exec, _) => vec![Command::ExecIn {
            vmid: a.vmid_number().ok_or_else(|| wrong_material(req))?,
            command: a.command.clone().unwrap_or_default().trim().to_string(),
        }],
        (GuardsCt, _) => vec![Command::ApplyGuards {
            vmid: a.vmid_number().ok_or_else(|| wrong_material(req))?,
        }],
        (TemplateBuild, _) => vec![Command::BuildTemplate {
            temp_vmid: a.vmid_number().ok_or_else(|| wrong_material(req))?,
            version: a
                .version
                .as_deref()
                .and_then(|v| v.trim().parse().ok())
                .ok_or_else(|| wrong_material(req))?,
            unprivileged: !a.privileged,
            base_template: a
                .base
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .map(str::to_string),
        }],
        (AnswerCheck, _) => {
            let verdict = a.verdict.as_deref().unwrap_or("").trim();
            vec![Command::AnswerManualCheck {
                check_id: a.check.clone().unwrap_or_default().trim().to_string(),
                ok: verdict == "ok",
                note: a.note.clone().unwrap_or_default().trim().to_string(),
                accept_days: (verdict == "accept")
                    .then(|| a.days.as_deref().and_then(|d| d.trim().parse().ok()))
                    .flatten(),
            }]
        }
        (
            UpdateHost,
            Material::HostRelease {
                binary_b64, proof, ..
            },
        ) => {
            vec![Command::SelfUpdateHost { binary_b64, proof }]
        }
        (
            InstallNative,
            Material::NativeRelease {
                manifest,
                unit_file,
                tag,
                ..
            },
        ) => {
            if manifest.stack_name != req.stack {
                return Err(name_mismatch(req, &manifest.stack_name));
            }
            vec![Command::InstallNativeRelease {
                manifest,
                unit_file,
                tag,
            }]
        }
        (Apply, Material::Apply { deploy, destroy }) => {
            let mut out = Vec::new();
            for mut spec in deploy {
                // redesign-stacks-6: "each stack is backed up first", the
                // same opt-out as the destroys below.
                spec.backup_first = !a.skip_backup;
                out.extend(deploy_commands(spec));
            }
            for name in destroy {
                out.push(Command::DestroyRecorded {
                    confirm: name.clone(),
                    stack: name,
                    skip_backup: a.skip_backup,
                });
            }
            out
        }
        (Patch, _) => vec![Command::PatchFleet],
        (RestartHost, _) => vec![Command::RestartHost],
        (ZfsReplicate, _) => vec![Command::ZfsReplicate],
        (BackupHostMeta, _) => vec![Command::BackupHostMeta],
        (BackupDevices, _) => vec![Command::BackupDevices],
        _ => return Err(wrong_material(req)),
    })
}

/// T85: each native binary on its own, then the deploy with the map emptied
/// (the host fills it back in from what was staged).
pub fn deploy_commands(mut spec: DeploySpec) -> Vec<Command> {
    let mut out = Vec::new();
    let staged: BTreeMap<String, String> = std::mem::take(&mut spec.native_binaries);
    for (unit, binary_b64) in staged {
        if binary_b64.is_empty() {
            spec.native_binaries.insert(unit, String::new());
            continue;
        }
        out.push(Command::StageNativeBinary {
            stack: spec.manifest.stack_name.clone(),
            unit: unit.clone(),
            binary_b64,
        });
        spec.native_binaries.insert(unit, String::new());
    }
    out.push(Command::DeployStack(Box::new(spec)));
    out
}

/// The CLI line for the actions whose command does not say everything the
/// verb takes (feat-stacks-7): the host update names its tag, install-native
/// the directory of the unit's service.yml, apply its flags. None: the line
/// comes from the command (`actions_cli`).
pub fn cli_override(req: &ActionRequest, material: &Material) -> Option<String> {
    match (req.action, material) {
        (ActionKind::UpdateHost, Material::HostRelease { tag, .. }) => {
            Some(format!("homelab release-update {tag}"))
        }
        (ActionKind::InstallNative, Material::NativeRelease { tag, dir, .. }) => {
            Some(format!("homelab install-native {dir} {tag}"))
        }
        (ActionKind::Apply, _) => {
            let mut line = "homelab apply --yes".to_string();
            if req.args.skip_backup {
                line.push_str(" --no-backup");
            }
            if req.args.force {
                line.push_str(" --force");
            }
            Some(line)
        }
        _ => None,
    }
}

/// arch-self: actions on the dashboard's own stack that restart it. The
/// page says so before the press; the outcome is read back after the
/// restart (the reply cannot arrive on a line that went away). A deploy
/// restarts it when its unit or env file changed (fix-159; the plan names
/// the reason), so Deploy stays on the list.
pub fn restarts_dashboard(stack: &str, kind: ActionKind) -> bool {
    use ActionKind::*;
    stack == SELF_STACK
        && matches!(
            kind,
            Deploy
                | DeployCommit
                | Restore
                | Update
                | Resize
                | UpdateNative
                | ReleaseUpdateNative
                | RollbackNative
                | InstallNative
        )
}

/// feat-stacks-5: a batch, validated as a whole before anything runs, so a
/// typo in the fifth stack does not leave four done and one refused.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub action: String,
    pub stacks: Vec<String>,
    #[serde(default)]
    pub args: ActionArgs,
    /// For a destructive action each stack's own name is its confirmation;
    /// the page collects one typed name per stack here.
    #[serde(default)]
    pub confirms: BTreeMap<String, String>,
}

pub const BATCH_MAX: usize = 64;

pub fn validate_batch(b: BatchRequest) -> Result<Vec<ActionRequest>, Refusal> {
    if b.stacks.is_empty() || b.stacks.len() > BATCH_MAX {
        return Err(Refusal::new(
            format!("{} on several stacks", b.action),
            format!(
                "a batch holds 1 to {BATCH_MAX} stacks, this one {}",
                b.stacks.len()
            ),
            "pick the stacks again",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for stack in &b.stacks {
        if !seen.insert(stack.clone()) {
            return Err(Refusal::new(
                format!("{} on several stacks", b.action),
                format!("{stack} is in the batch twice"),
                "name each stack once",
            ));
        }
        let mut args = b.args.clone();
        if let Some(c) = b.confirms.get(stack) {
            args.confirm = Some(c.clone());
        }
        let mut req = validate(stack, &b.action, args)?;
        // redesign-stacks-6: the batch Deploy promises each stack is backed
        // up before it changes; the server keeps that promise, not the page.
        if req.action == ActionKind::Deploy {
            req.args.backup_first = true;
        }
        out.push(req);
    }
    if out.iter().any(|r| r.action.host_wide()) {
        return Err(Refusal::new(
            format!("{} on several stacks", b.action),
            "a host-wide action runs once, not per stack",
            format!("send it to /data/actions/{HOST_TARGET}/{}", b.action),
        ));
    }
    Ok(out)
}
