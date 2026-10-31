//! One stack's backup: the shared state its steps read, and the steps in order.

use crate::executor::{Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Scope, StepFailure, StepOutcome};
use crate::sink::Level;

use super::config::BackupCfg;
use super::owners::owner_groups;
use super::plan::BACKUP_STEPS;
use super::trigger::run_tag;

/// What every step of one stack's backup reads. `primary` is the restic base
/// the snapshot is written to: the local repository set when there is one
/// (backup-local-first), else the remote.
pub(super) struct Job<'r, 'a> {
    pub ctx: &'r OpCtx<'a>,
    pub exec: &'r dyn Executor,
    pub m: &'r StackManifest,
    pub cfg: &'r BackupCfg,
    /// D25: one repository per owning app, in the manifest's order.
    pub groups: Vec<(String, Vec<String>)>,
    pub local_base: Option<String>,
    pub primary: String,
    /// The containers `quiesce` stopped, so `resume` starts exactly those.
    pub paused: std::sync::Mutex<Vec<String>>,
}

/// E1: snapshot a stack's /appdata paths, quiescing paused containers.
pub async fn backup(ctx: &OpCtx<'_>, m: &StackManifest, cfg: &BackupCfg) -> OperationReport {
    let mut scope = Scope::top(&format!("backup-{}", m.stack_name), ctx.sink, ctx.journal);
    scope.plan_if_top(BACKUP_STEPS);
    let result = backup_impl(ctx, m, cfg, &mut scope).await;
    scope.finish(result)
}

/// fix-step-plan-nested: the step logic behind `backup`, written to run
/// through a `Scope` — as the outermost call or nested inside `destroy`'s
/// own composed plan ("backup before destroy").
pub(crate) async fn backup_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &StackManifest,
    cfg: &BackupCfg,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let mut job = Job {
        ctx,
        exec: &texec,
        m,
        cfg,
        groups: owner_groups(m),
        local_base: None,
        primary: cfg.restic_base.clone(),
        paused: std::sync::Mutex::new(Vec::new()),
    };
    preflight(&job, scope).await?;
    job.choose_primary(scope).await;
    prepare_repos(&job, scope).await?;
    snapshot_and_resume(&job, scope).await?;
    publish(&job, scope).await?;
    scope.log(
        Level::Info,
        format!("[backup] {} snapshot complete", m.stack_name),
    );
    Ok(())
}

/// The checks that run before anything is touched.
async fn preflight(job: &Job<'_, '_>, scope: &mut Scope<'_, '_>) -> Result<(), StepFailure> {
    scoped_step!(scope, "safety gates", job.safety_gates().await);
    let mut newcomer: Option<(String, String)> = None;
    scoped_step!(scope, "owner conflict", {
        newcomer = job.owner_conflict().await?;
        Ok(StepOutcome::Unchanged)
    });
    if let Some((owner, other)) = &newcomer {
        job.warn_newcomer(scope, owner, other);
    }
    scoped_step!(scope, "in use?", job.refuse_when_in_use().await);
    scoped_step!(
        scope,
        "declared-empty paths",
        job.check_no_data_empty().await
    );
    scoped_step!(scope, "declared paths exist", job.check_paths_exist().await);
    Ok(())
}

/// Create the repositories and clear locks a killed run left behind.
async fn prepare_repos(job: &Job<'_, '_>, scope: &mut Scope<'_, '_>) -> Result<(), StepFailure> {
    scoped_step!(scope, "init repos", job.init_repos().await);
    scoped_step!(scope, "clear stale locks", job.clear_stale_locks().await);
    Ok(())
}

/// H2 hardening: the snapshot may fail, but RESUME MUST ALWAYS RUN — a
/// fail-closed abort here would leave the quiesced databases down until
/// a human noticed. So the snapshot error is captured, resume runs
/// unconditionally, and only then does the operation fail.
async fn snapshot_and_resume(
    job: &Job<'_, '_>,
    scope: &mut Scope<'_, '_>,
) -> Result<(), StepFailure> {
    scoped_step!(scope, "quiesce", job.quiesce().await);
    let tag = run_tag(job.ctx.now_unix);
    let snapshot_result = scope
        .step("snapshot", || async { job.snapshot_all(&tag).await })
        .await;
    scoped_step!(scope, "resume", job.resume().await);
    if let Err(e) = snapshot_result {
        return Err(StepFailure {
            step: scope.qualify("snapshot"),
            err: e,
        });
    }
    Ok(())
}

/// Upload what was written locally, then thin the repositories out.
async fn publish(job: &Job<'_, '_>, scope: &mut Scope<'_, '_>) -> Result<(), StepFailure> {
    scoped_step!(scope, "upload", job.upload().await);
    scoped_step!(scope, "retention", job.retention().await);
    Ok(())
}
