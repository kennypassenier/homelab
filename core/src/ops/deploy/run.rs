//! The state one deploy carries from step to step.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::error::CoreError;
use crate::executor::Executor;
use crate::manifest::{DeploySpec, StackManifest};
use crate::ops::OpCtx;
use crate::ops::hardware::GpuDevices;
use crate::sink::{Level, PipelineEvent};
use crate::state::StackState;

use super::incomplete::mark_incomplete;

/// J1: per app, the service checks' readings taken before anything changed.
pub(super) type Baseline = BTreeMap<String, Vec<(String, String)>>;

/// A step that ended the deploy, and why.
pub(super) struct Failure {
    pub step: String,
    pub err: CoreError,
}

/// What the push step learned, read by the steps after it.
#[derive(Default)]
pub(super) struct Pushed {
    /// Apps with a changed config file (not the compose file itself).
    pub needs_restart: std::collections::BTreeSet<String>,
    /// Apps whose compose file changed.
    pub recreated: std::collections::BTreeSet<String>,
    /// S2: every file actually written, with the hash it should now have.
    /// Shared with the verification closure of the push step.
    pub written: Arc<Mutex<Vec<(String, String)>>>,
    /// fix-142: sha256 of EVERY non-rootfs file sent, by manifest path.
    pub file_hashes: BTreeMap<String, String>,
    /// latch-files: secret files whose content this deploy changed.
    pub secrets_changed: Vec<String>,
    /// F129: per app, where its compose lives in the container, what it said
    /// before the cache rewrite, and the mode to write it back with.
    pub cache_orig: BTreeMap<String, (String, String, String)>,
}

/// One deploy of one stack: its inputs and what the steps found out.
pub(super) struct Run<'a, 'c> {
    pub ctx: &'a OpCtx<'c>,
    pub spec: &'a DeploySpec,
    pub exec: &'a dyn Executor,
    pub op: String,
    pub exists: bool,
    pub created: bool,
    /// W1: what the host offers, read once before anything is created.
    pub gpu: Option<GpuDevices>,
    /// The record as it stood before this deploy.
    pub prior: Option<StackState>,
    pub prior_manifest: Option<StackManifest>,
    /// Native units the stack file dropped since the last deploy.
    pub dropped_natives: Vec<String>,
    pub baseline: Baseline,
    /// D60: upstreams the registry cache answers for right now.
    pub cache_up: Vec<String>,
    pub dropped_repo_files: Vec<String>,
    /// Vault copies of each retired unit, for its retired record (ask-9).
    pub unit_vault: BTreeMap<String, Vec<String>>,
    pub pushed: Pushed,
    /// fix-199: the deploying client can name the declared apps.
    pub apps_removal: bool,
}

impl<'a, 'c> Run<'a, 'c> {
    pub fn new(
        ctx: &'a OpCtx<'c>,
        spec: &'a DeploySpec,
        exec: &'a dyn Executor,
        op: String,
    ) -> Self {
        Run {
            ctx,
            spec,
            exec,
            op,
            exists: false,
            created: false,
            gpu: None,
            prior: None,
            prior_manifest: None,
            dropped_natives: Vec::new(),
            baseline: Baseline::new(),
            cache_up: Vec::new(),
            dropped_repo_files: Vec::new(),
            unit_vault: BTreeMap::new(),
            pushed: Pushed::default(),
            apps_removal: true,
        }
    }

    /// The inputs every step reads, detached from `self` so a step can
    /// still change `self` while it holds them.
    pub fn parts(
        &self,
    ) -> (
        &'a OpCtx<'c>,
        &'a DeploySpec,
        &'a StackManifest,
        &'a dyn Executor,
    ) {
        (self.ctx, self.spec, &self.spec.manifest, self.exec)
    }

    /// S2: leave a record that this stack was half-deployed, then describe
    /// the failure for the runner.
    pub async fn failed(&self, step: &str, err: CoreError) -> Failure {
        mark_incomplete(self.exec, self.ctx, &self.spec.manifest, step).await;
        Failure {
            step: step.to_string(),
            err,
        }
    }
}

/// A transcript line from the host.
pub(super) fn log_info(ctx: &OpCtx<'_>, msg: String) {
    emit(ctx, Level::Info, msg);
}

/// A5: a line at a level that reaches the notification. A native unit left
/// unstarted because its program or its secret is missing is not a detail in
/// a transcript.
pub(super) fn log_warn(ctx: &OpCtx<'_>, msg: String) {
    emit(ctx, Level::Warn, msg);
}

fn emit(ctx: &OpCtx<'_>, level: Level, msg: String) {
    ctx.sink.emit(PipelineEvent::Line {
        level,
        source: "HOST".into(),
        msg,
    });
}
