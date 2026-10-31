//! E2 for one app of a stack, or the whole stack: the steps in order.

use crate::executor::{Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::config::BackupCfg;
use super::plan::restore_plan_names;
use super::restore_copy::pre_restore_dir;
use super::restore_data::refuse_native_units;
use super::restore_select::restore_groups;

/// What every step of one restore reads.
pub(super) struct RestoreJob<'r, 'a> {
    pub ctx: &'r OpCtx<'a>,
    pub exec: &'r dyn Executor,
    pub m: &'r StackManifest,
    pub cfg: &'r BackupCfg,
    pub snapshot: &'r str,
    pub safety_copy: bool,
    pub op: String,
    /// D25: the repositories restored, in the manifest's order.
    pub groups: Vec<(String, Vec<String>)>,
    pub apps: Vec<String>,
    /// fix-112: which snapshot of each repository this restore takes.
    pub chosen: Vec<(String, String)>,
    pub copy_dest: String,
}

impl<'r, 'a> RestoreJob<'r, 'a> {
    /// `request` is `(snapshot, safety_copy, app)` as the caller asked.
    fn new(
        ctx: &'r OpCtx<'a>,
        exec: &'r dyn Executor,
        m: &'r StackManifest,
        cfg: &'r BackupCfg,
        groups: Vec<(String, Vec<String>)>,
        request: (&'r str, bool, Option<&str>),
    ) -> Self {
        let (snapshot, safety_copy, app) = request;
        Self {
            ctx,
            exec,
            m,
            cfg,
            snapshot,
            safety_copy,
            op: format!("restore-{}", m.stack_name),
            apps: app.map_or_else(|| m.apps.clone(), |a| vec![a.to_string()]),
            copy_dest: pre_restore_dir(ctx, m),
            chosen: groups
                .iter()
                .map(|(o, _)| (o.clone(), snapshot.to_string()))
                .collect(),
            groups,
        }
    }
}

/// Run a step; on failure end the operation with its report (A3). Unlike
/// `step!` it yields `Err(report)`, so a phase can hand the runner back.
macro_rules! phase_step {
    ($runner:expr_2021, $name:expr_2021, $body:expr_2021) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return Err($runner.finish_err($name, &e)),
        }
    };
}

/// E2 for one app of a stack (`app`), or the whole stack (None).
///
/// fix-112: three changes (story: `docs/deployment/REGISTER.md`). Only the
/// named app is stopped and restored, so rolling back one broken Sonarr no
/// longer rolls back Radarr and Jellyfin. A stack with several repositories
/// restores one NIGHT across all of them (the `run-<unix>` tag every backup
/// writes), never each repository's own newest. And with the safety copy
/// taken, each target is emptied before restic writes into it, because a
/// restore over a non-empty directory leaves every file the snapshot does
/// not have (newer Postgres WAL segments beside a restored cluster is a
/// corrupt database that "restored successfully").
pub async fn restore_app(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
    safety_copy: bool,
    app: Option<&str>,
) -> OperationReport {
    let op = format!("restore-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    // fix-171 round 3: known from `safety_copy` alone, before anything runs.
    runner.plan(&restore_plan_names(safety_copy));
    runner.log(
        Level::Warn,
        format!("[restore] {} from snapshot '{}'", m.stack_name, snapshot),
    );
    let runner = match gate_phase(ctx, exec, m, runner).await {
        Ok(runner) => runner,
        Err(report) => return report,
    };
    // D25: a stack's data lives in one repository per owning app, so a
    // restore walks all of them. fix-112: only the named app's, when named.
    let groups = match restore_groups(m, app) {
        Ok(g) => g,
        Err(e) => return runner.finish_err("select app", &e),
    };
    let mut job = RestoreJob::new(ctx, exec, m, cfg, groups, (snapshot, safety_copy, app));
    match run_phases(&mut job, runner).await {
        Ok(runner) => finish(&job, runner).await,
        Err(report) => report,
    }
}

async fn run_phases<'a>(
    job: &mut RestoreJob<'_, '_>,
    runner: Runner<'a>,
) -> Result<Runner<'a>, OperationReport> {
    let runner = select_phase(job, runner).await?;
    let runner = room_phase(job, runner).await?;
    let runner = write_phase(job, runner).await?;
    verify_phase(job, runner).await
}

/// A1/A2: restore composes down and writes over the target — full gate.
async fn gate_phase<'a>(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    m: &StackManifest,
    mut runner: Runner<'a>,
) -> Result<Runner<'a>, OperationReport> {
    phase_step!(runner, "safety gates", {
        crate::manifest::validate_manifest(m)?;
        crate::ops::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });
    phase_step!(runner, "native units", refuse_native_units(m));
    Ok(runner)
}

/// Which snapshot of each repository this restore takes, validated before
/// anything is stopped.
async fn select_phase<'a>(
    job: &mut RestoreJob<'_, '_>,
    mut runner: Runner<'a>,
) -> Result<Runner<'a>, OperationReport> {
    let mut picked = None;
    phase_step!(runner, "validate snapshot", {
        picked = job.validate_snapshot().await?;
        Ok(StepOutcome::Unchanged)
    });
    if let Some((ids, note)) = picked {
        job.chosen = ids;
        if let Some(note) = note {
            runner.log(Level::Warn, format!("[restore] {}", note));
        }
    }
    if job.chosen.len() > 1 {
        let listed: Vec<String> = job
            .chosen
            .iter()
            .map(|(o, id)| format!("{} {}", o, id))
            .collect();
        runner.log(
            Level::Info,
            format!(
                "[restore] one night across {} repositories: {}",
                job.chosen.len(),
                listed.join(", ")
            ),
        );
    }
    Ok(runner)
}

/// fix-64: when the damage happened before last night's backup, `latest`
/// IS the damage, and the restore used to overwrite the only other copy
/// of the data. The current data is copied aside first, with the stack
/// down, into a directory of its own that nothing ever removes.
async fn room_phase<'a>(
    job: &RestoreJob<'_, '_>,
    mut runner: Runner<'a>,
) -> Result<Runner<'a>, OperationReport> {
    if job.safety_copy {
        phase_step!(runner, "room for a safety copy", {
            job.room_for_safety_copy().await
        });
    } else {
        runner.log(
            Level::Warn,
            format!(
                "[restore] --no-safety-copy: the current data of {} is overwritten without a \
                 copy, and restic writes over it in place — files the snapshot does not have \
                 stay behind",
                job.m.stack_name
            ),
        );
    }
    Ok(runner)
}

/// Stop, copy aside, restore, and resume — the resume runs whatever happened
/// before it, so a failed restore never leaves the stack composed down.
async fn write_phase<'a>(
    job: &RestoreJob<'_, '_>,
    mut runner: Runner<'a>,
) -> Result<Runner<'a>, OperationReport> {
    phase_step!(runner, "quiesce stack", job.quiesce_stack().await);
    let copy_result = job.take_safety_copy(&mut runner).await;
    let restore_result = job.restore_data_step(&mut runner, &copy_result).await;
    phase_step!(runner, "resume stack", job.resume_stack().await);
    if let Err(e) = copy_result {
        return Err(runner.finish_err("safety copy", &e));
    }
    if let Err(e) = restore_result {
        return Err(runner.finish_err("restore data", &e));
    }
    Ok(runner)
}

async fn verify_phase<'a>(
    job: &RestoreJob<'_, '_>,
    mut runner: Runner<'a>,
) -> Result<Runner<'a>, OperationReport> {
    phase_step!(runner, "verify health", job.verify_health().await);
    Ok(runner)
}

/// fix-147: a restore is a check of these directories that succeeded; a
/// deploy's earlier failed check of them stops standing.
async fn finish(job: &RestoreJob<'_, '_>, runner: Runner<'_>) -> OperationReport {
    let restored = job.all_paths();
    crate::ops::deploy::record_restore_checks(job.ctx, &job.m.stack_name, &[], &restored).await;
    runner.log(
        Level::Info,
        format!("[restore] {} restored and verified", job.m.stack_name),
    );
    runner.finish_ok()
}
