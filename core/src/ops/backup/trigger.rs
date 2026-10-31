//! Why a backup ran, and the tag naming its night.

/// fix-112: the tag one night's backups carry, the same in every repository
/// of the stack.
pub fn run_tag(now_unix: u64) -> String {
    format!("run-{}", now_unix)
}

/// fix-223: why a particular restic backup ran, tagged onto the snapshot
/// itself (`trigger:<kind>`) next to the existing `run-<unix>` tag, so the
/// restore picker can show what kind of backup each snapshot is — not only
/// when it ran. Enumerated from the call sites that actually invoke
/// `backup`/`backup_native` today: the nightly round
/// (`host/src/main.rs::run_backup_batch`), an operator's on-demand backup
/// (`Rpc::BackupStack`/`Rpc::BackupNative`), and the safety backup `destroy`
/// takes before removing a container (`core/src/ops/destroy.rs`). There is
/// no pre-update backup anywhere in the codebase today, so no tag for one
/// exists either — adding one would be guessing at a trigger that does not
/// run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackupTrigger {
    /// The nightly scheduled round.
    Nightly,
    /// An operator asked for it on demand — the CLI or the admin dashboard's
    /// "Back up" / "Back up (native)" action.
    #[default]
    Manual,
    /// Taken automatically, immediately before `destroy` removes the
    /// container (`destroy_impl`, core/src/ops/destroy.rs).
    PreDestroy,
    /// Taken automatically right before a deploy changes the stack, when
    /// the deploy was asked for it (`DeploySpec::backup_first`:
    /// the dashboard's Deploy all changes and batch Deploy).
    PreDeploy,
}

impl BackupTrigger {
    /// The tag value stored on the snapshot: `restic backup --tag <this>`.
    pub fn tag(self) -> &'static str {
        match self {
            BackupTrigger::Nightly => "trigger:nightly",
            BackupTrigger::Manual => "trigger:manual",
            BackupTrigger::PreDestroy => "trigger:pre-destroy",
            BackupTrigger::PreDeploy => "trigger:pre-deploy",
        }
    }

    /// The word the picker shows for this trigger.
    pub fn label(self) -> &'static str {
        match self {
            BackupTrigger::Nightly => "nightly",
            BackupTrigger::Manual => "manual",
            BackupTrigger::PreDestroy => "pre-destroy",
            BackupTrigger::PreDeploy => "pre-deploy",
        }
    }

    /// Parse a `trigger:<kind>` tag value back into its kind. `None` for
    /// anything that is not one of the kinds above, including an unrelated
    /// tag — the caller decides what an unrecognised value means (today:
    /// "kind not recorded", same as no tag at all).
    pub fn parse_tag(tag: &str) -> Option<Self> {
        match tag.strip_prefix("trigger:")? {
            "nightly" => Some(BackupTrigger::Nightly),
            "manual" => Some(BackupTrigger::Manual),
            "pre-destroy" => Some(BackupTrigger::PreDestroy),
            "pre-deploy" => Some(BackupTrigger::PreDeploy),
            _ => None,
        }
    }
}
