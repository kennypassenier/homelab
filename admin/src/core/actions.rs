//! feat-stacks-4 / feat-stacks-5 / feat-stacks-6: every action the CLI can
//! take on one stack (and the four host-wide ones a schedule may want), as
//! data, and the one pure step from "the browser asked for X on stack Y with
//! these arguments" to the exact `Command`s the CLI would send.
//!
//! The shell (`shell::actions`) reads what the command needs from disk (the
//! stack's manifest or deploy spec from the working copy) and sends what this
//! module built; nothing here does I/O.

use std::collections::BTreeMap;

use homelab_proto::{Command, DeploySpec, NativeServiceManifest, Scope, StackManifest};
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
}

impl ActionKind {
    pub const ALL: &'static [ActionKind] = &[
        ActionKind::Deploy,
        ActionKind::DeployCommit,
        ActionKind::Backup,
        ActionKind::Restore,
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
    ];

    /// The name in a URL: `deploy`, `backup-native`, ...
    pub fn slug(self) -> &'static str {
        use ActionKind::*;
        match self {
            Deploy => "deploy",
            DeployCommit => "deploy-commit",
            Backup => "backup",
            Restore => "restore",
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
        }
    }

    pub fn from_slug(s: &str) -> Option<ActionKind> {
        ActionKind::ALL.iter().copied().find(|k| k.slug() == s)
    }

    pub fn host_wide(self) -> bool {
        use ActionKind::*;
        matches!(self, Patch | ZfsReplicate | BackupHostMeta | BackupDevices)
    }

    pub fn needs(self) -> Needs {
        use ActionKind::*;
        match self {
            Deploy | DeployCommit | PruneOrphans => Needs::Spec,
            Backup | Restore | Update | Resize => Needs::Manifest,
            // A destroy reads the manifest when the directory is there and
            // falls back to the host's record (DestroyRecorded) when not.
            Destroy => Needs::Manifest,
            Adopt => Needs::NativeManifest,
            Guards => Needs::Vmid,
            _ => Needs::Nothing,
        }
    }

    pub fn args(self) -> &'static [Arg] {
        use ActionKind::*;
        match self {
            Deploy => &[Arg::Force],
            DeployCommit => &[Arg::Commit, Arg::Force],
            Restore => &[Arg::Confirm, Arg::Snapshot, Arg::App, Arg::SkipSafetyCopy],
            Update => &[Arg::App],
            RollbackNative => &[Arg::Unit],
            PruneOrphans | Forget => &[Arg::Confirm],
            Destroy => &[Arg::Confirm, Arg::SkipBackup],
            // Without `confirm` a wipe only lists what it would delete.
            Wipe => &[Arg::Confirm],
            _ => &[],
        }
    }

    /// Must the stack name be typed? (Wipe asks only for the real run.)
    pub fn confirm(self) -> bool {
        use ActionKind::*;
        matches!(self, Restore | PruneOrphans | Destroy | Forget)
    }

    /// arch-self: what the dashboard never does to its own stack.
    pub fn refused_for_self(self) -> bool {
        use ActionKind::*;
        matches!(self, Destroy | Forget | Wipe)
    }

    pub fn label(self) -> &'static str {
        use ActionKind::*;
        match self {
            Deploy => "Deploy",
            DeployCommit => "Deploy an earlier commit",
            Backup => "Back up",
            Restore => "Restore",
            Update => "Update",
            Resize => "Resize",
            Enable => "Enable",
            Disable => "Disable",
            Adopt => "Adopt",
            BackupNative => "Back up (native)",
            UpdateNative => "Update (native, own policy)",
            ReleaseUpdateNative => "Install newest release",
            RollbackNative => "Roll back binary",
            Guards => "Apply guards",
            PruneOrphans => "Prune orphans",
            Destroy => "Destroy",
            Forget => "Forget",
            Wipe => "Wipe",
            Patch => "Patch the fleet",
            ZfsReplicate => "ZFS replicate",
            BackupHostMeta => "Back up the host's own state",
            BackupDevices => "Back up devices",
        }
    }

    /// The CLI help's own sentence for the verb, so both say the same.
    pub fn what(self) -> &'static str {
        use ActionKind::*;
        match self {
            Deploy => "create or reconcile the container; what the files no longer declare is removed, data stays",
            DeployCommit => "deploy the stack's files as they were at an earlier commit of the working copy",
            Backup => "a restic snapshot of the stack now",
            Restore => "restore the stack's data from a snapshot ('latest' unless named); keeps a copy of the current data first",
            Update => "pull and recreate one app or all, with rollback",
            Resize => "apply the manifest's memory, cores and disk to the running container",
            Enable => "take the stack back into the nightly backup and update, and start-on-boot",
            Disable => "park the stack: no nightly backup or update, start-on-boot cleared; no container is stopped",
            Adopt => "take over a hand-built container described by its service.yml; restarts nothing",
            BackupNative => "back up an adopted service now",
            UpdateNative => "update an adopted service the way its own update policy says",
            ReleaseUpdateNative => "install the newest release of each service in the stack",
            RollbackNative => "put a native service's previous binary back and park the stack's updates",
            Guards => "apply the runaway guards: log caps, journald limits, logrotate, a weekly prune",
            PruneOrphans => "remove files the repository dropped, without a deploy",
            Destroy => "back up, then destroy the container; from the host's record when the directory is gone",
            Forget => "for a container already gone: drop its record and registrations",
            Wipe => "delete what a retired stack kept: backups, /appdata, vault copies (without confirm: list only)",
            Patch => "apt update and dist-upgrade every managed container, one at a time",
            ZfsReplicate => "run the ZFS snapshot and replication jobs now",
            BackupHostMeta => "snapshot the daemon's own state now: vault, state, TLS, intent repository",
            BackupDevices => "fetch each configured device's own configuration now",
        }
    }

    /// The host's scope for the command this action sends.
    pub fn scope(self) -> Scope {
        use ActionKind::*;
        match self {
            PruneOrphans | Destroy | Forget | Wipe => Scope::All,
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
        v
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
    if let Some(s) = &args.snapshot {
        if !valid_word(s) {
            return Err(Refusal::new(
                what,
                format!("{s:?} is not a snapshot id"),
                "pick a snapshot from the backup page, or leave it out for 'latest'",
            ));
        }
    }
    for (field, v) in [("app", &args.app), ("unit", &args.unit)] {
        if let Some(v) = v {
            if !valid_word(v) {
                return Err(Refusal::new(
                    what,
                    format!("{v:?} is not a valid {field} name"),
                    format!("use the {field} name the stack page shows"),
                ));
            }
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
                ))
            }
        }
    }
    Ok(ActionRequest {
        stack: stack.to_string(),
        action: kind,
        args,
    })
}

/// What the shell read for the command.
#[derive(Debug, Clone)]
pub enum Material {
    None,
    Manifest(Box<StackManifest>),
    Spec(Box<DeploySpec>),
    Native(Box<NativeServiceManifest>),
    Vmid(u16),
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
            deploy_commands(*spec)
        }
        (Backup, Material::Manifest(m)) => vec![Command::BackupStack(manifest(&m)?)],
        (Restore, Material::Manifest(m)) => vec![Command::RestoreStack {
            manifest: manifest(&m)?,
            snapshot: a.snapshot.clone().unwrap_or_else(|| "latest".into()),
            confirm: a.confirm.clone(),
            skip_safety_copy: a.skip_safety_copy,
            app: a.app.clone(),
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
        (Patch, _) => vec![Command::PatchFleet],
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

/// arch-self: actions on the dashboard's own stack that restart it. The
/// page says so before the press; the outcome is read back after the
/// restart (the reply cannot arrive on a line that went away).
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
        out.push(validate(stack, &b.action, args)?);
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
