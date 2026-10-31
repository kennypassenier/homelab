//! [`StackFiles`]: the stack files as the actions see them, and the types
//! that go with it. [`RepoFiles`] implements it (`repofiles`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use homelab_core::ops::deployguard::Ancestry;
use serde::Serialize;

use crate::core::actions::{ActionKind, Material, Refusal};

/// One commit that touched a stack, for the roll-back list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitInfo {
    pub commit: String,
    /// Unix seconds of the commit.
    pub at: i64,
    pub subject: String,
}

/// The stack files, as the actions see them. Every method blocks (files,
/// git, latch); the queue calls them off the async threads.
pub trait StackFiles: Send + Sync + 'static {
    /// What `needs` asks for, from the working copy, or from `commit` when
    /// given (feat-stacks-6). A destroy of a stack whose directory is gone
    /// answers `Material::None` (DestroyRecorded).
    fn read(
        &self,
        stack: &str,
        kind: ActionKind,
        commit: Option<&str>,
    ) -> Result<Material, Refusal>;
    /// Where the host's commit stands against the working copy's HEAD.
    fn ancestry(&self, commit: &str) -> Ancestry;
    /// Newest first.
    fn commits(&self, stack: &str, limit: usize) -> Result<Vec<CommitInfo>, Refusal>;
    /// The native units the stack's files declare.
    fn native_units(&self, stack: &str) -> Vec<String>;
    /// Whether the working copy is there at all.
    fn present(&self) -> bool;
    /// install-native: the unit's manifest, its unit file and the directory
    /// the CLI names (`stacks/kyu/kyu-runner`), from the working copy.
    fn native_release(
        &self,
        stack: &str,
        unit: Option<&str>,
    ) -> Result<(homelab_proto::NativeServiceManifest, String, String), Refusal> {
        let _ = unit;
        Err(Refusal::new(
            format!("install-native {stack}"),
            "these stack files cannot say which native services a stack holds",
            "use the dashboard with its working copy",
        ))
    }
    /// apply and drift: every declared stack of the working copy, with its
    /// intent hash (latch runs for the secrets the hash covers) and, when
    /// `with_specs`, the spec a deploy sends (programs included).
    fn local_stacks(&self, with_specs: bool) -> Result<LocalStacks, Refusal> {
        let _ = with_specs;
        Err(Refusal::new(
            "the stacks directory",
            "these stack files cannot be listed",
            "use the dashboard with its working copy",
        ))
    }
    /// The Deploy review's plan: a stack's files without secrets.
    fn stack_files(&self, stack: &str) -> Result<Vec<homelab_proto::FileBlob>, Refusal> {
        Err(Refusal::new(
            format!("the files of {stack}"),
            "these stack files cannot be read one by one",
            "use the dashboard with its working copy",
        ))
    }
}

/// What the working copy's stacks directory holds, for apply and drift.
#[derive(Debug, Clone, Default)]
pub struct LocalStacks {
    /// Every declared (not ephemeral) stack with its intent hash, or why it
    /// could not be built.
    pub hashes: Vec<(String, Result<String, String>)>,
    /// The name of every directory under stacks/, deployable or not.
    pub dirs: Vec<String>,
    /// Stacks deployed by name only, never by apply.
    pub ephemeral: Vec<String>,
    /// The specs, when asked for.
    pub specs: BTreeMap<String, homelab_proto::DeploySpec>,
    /// fix-192: each declared stack's per-component digests, for the apply
    /// plan's reason text — built on the same cheap path as `hashes` even
    /// when `specs` is empty.
    pub component_digests: BTreeMap<String, homelab_core::manifest::ComponentDigests>,
}

/// The homelab working copy (arch-state: `…/repo`).
pub struct RepoFiles {
    pub repo: PathBuf,
    /// Where a checkout of an earlier commit is made, and removed again.
    pub scratch: PathBuf,
    /// milestone edit: the working copy the dashboard keeps itself. A read
    /// first takes the remote's newest commits (best effort) and holds the
    /// copy still, so a deploy never reads a stack an edit is writing.
    pub wc: Option<Arc<crate::shell::workcopy::WorkingCopy>>,
}

/// update-flow-latch-1: does `commit` (a full id, or an abbreviation of at
/// least seven characters) name the working copy's `head`?
pub fn is_head(head: Option<&str>, commit: &str) -> bool {
    let commit = commit.trim();
    commit.len() >= 7 && head.is_some_and(|h| h.trim().starts_with(commit))
}

pub(super) fn no_repo(repo: &Path, what: String) -> Refusal {
    Refusal::new(
        what,
        format!(
            "the dashboard has no working copy of the homelab repository at {}",
            repo.display()
        ),
        "the dashboard clones it there itself once the deploy key is provisioned (the \
         working copy panel says what is missing); actions that need only the stack name work \
         without it",
    )
}
