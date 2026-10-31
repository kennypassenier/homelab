//! C7 nightly backup for a native stack. The data lives INSIDE the
//! container (adoption never restarts a service, so a bind-mount to
//! /appdata was never an option); the snapshot therefore streams
//! `pct exec tar` straight into `restic --stdin` — one host-side pipeline,
//! nothing written in between. Repo naming and tiered retention are the
//! same as every compose stack: `<base>/<unit>-config` (D25: named after
//! the service, not the stack).

use super::backup_pause::{PauseState, pause_chassis, pause_unit};
use super::backup_prepare::{clear_stale_locks, empty_over_history, find_own_copy, init_repo};
use super::backup_snapshot::{effective_tiers, resume_service, retention, snapshot};
use super::staging::remove_staging_file;
use super::steps::guard_step;
use crate::executor::{Executor, TracingExecutor};
use crate::native::{BackupPause, NativeServiceManifest};
use crate::ops::OpCtx;
use crate::ops::backup::BackupCfg;
use crate::runner::{OperationReport, Scope, StepFailure};
use crate::sink::Level;

/// fix-115: a unit that declares it keeps nothing has nothing to archive.
/// Story: docs/deployment/REGISTER.md.
fn keeps_nothing(m: &NativeServiceManifest) -> bool {
    m.stateless && m.data_dirs.is_empty() && m.backup_from_newest.is_none()
}

/// one-process-batch-2: `backup_native`'s step plan for one service — empty
/// for a stateless unit that keeps nothing (the op then plans nothing). The
/// pause step's name and whether a resume step follows are decided by
/// `m.backup_pause`, a fixed field of the manifest. The one list the op plans
/// from and the dashboard counts.
pub fn backup_native_plan_names(m: &NativeServiceManifest) -> Vec<String> {
    if keeps_nothing(m) {
        return Vec::new();
    }
    let mut steps: Vec<&str> = vec![
        "guard target",
        "init repo",
        "find the service's own newest copy",
        "empty data over history",
        "clear stale locks",
    ];
    match m.backup_pause {
        BackupPause::Off => {}
        BackupPause::Unit => steps.push("pause the service"),
        BackupPause::Chassis => steps.push("pause the service (chassis)"),
    }
    steps.push("snapshot");
    if !matches!(m.backup_pause, BackupPause::Off) {
        steps.push("resume the service");
    }
    steps.push("retention");
    steps.into_iter().map(String::from).collect()
}

/// Guard, init the repository, find the service's own copy (if it keeps
/// one), refuse an empty rebuild over history and clear stale locks.
/// Returns the own copy's path.
async fn prepare_phase<'a>(
    ctx: &OpCtx<'a>,
    exec: &dyn Executor,
    (m, cfg): (&NativeServiceManifest, &BackupCfg),
    scope: &mut Scope<'_, 'a>,
) -> Result<Option<String>, StepFailure> {
    scoped_step!(scope, "guard target", guard_step(ctx, exec, m).await);
    scoped_step!(scope, "init repo", init_repo(ctx, cfg, m).await);
    let mut own_copy: Option<String> = None;
    scoped_step!(
        scope,
        "find the service's own newest copy",
        find_own_copy(exec, m, &mut own_copy).await
    );
    if let Some(f) = &own_copy {
        let note = format!("[backup] {} archives its own copy {}", m.stack_name, f);
        scope.log(Level::Info, note);
    }
    scoped_step!(
        scope,
        "empty data over history",
        empty_over_history(exec, cfg, m, own_copy.is_some()).await
    );
    scoped_step!(
        scope,
        "clear stale locks",
        clear_stale_locks(exec, cfg, m).await
    );
    Ok(own_copy)
}

/// fix-113: a service declared `backup_pause` is quiesced for the tar, so
/// its store is not archived mid-write.
async fn pause_phase<'a>(
    exec: &dyn Executor,
    (m, cfg): (&NativeServiceManifest, &BackupCfg),
    unit: &str,
    scope: &mut Scope<'_, 'a>,
) -> Result<PauseState, StepFailure> {
    let mut state = PauseState::default();
    match m.backup_pause {
        BackupPause::Off => {}
        BackupPause::Unit => {
            scoped_step!(
                scope,
                "pause the service",
                pause_unit(exec, m, unit, &mut state).await
            );
        }
        BackupPause::Chassis => {
            scoped_step!(
                scope,
                "pause the service (chassis)",
                pause_chassis(exec, cfg, m, unit, &mut state).await
            );
        }
    }
    if let Some(note) = &state.pause_note {
        scope.log(Level::Info, format!("[backup] {} {}", unit, note));
    }
    Ok(state)
}

/// The snapshot, and the resume of whatever the pause quiesced — which
/// happens whatever the snapshot did, before its error (if any) is
/// reported.
async fn snapshot_phase<'a>(
    exec: &dyn Executor,
    (m, cfg): (&NativeServiceManifest, &BackupCfg),
    unit: &str,
    own_copy: Option<&str>,
    state: &mut PauseState,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let snapshot_result = scope
        .step("snapshot", || async {
            snapshot(exec, cfg, m, own_copy, state).await
        })
        .await;
    // fix-113 ADDENDUM: a staged tar must not survive the run (rule 20) —
    // removed here regardless of whether the snapshot step succeeded.
    if let Some(stage_path) = &state.staged_file {
        remove_staging_file(exec, stage_path).await;
    }
    if state.needs_resume {
        scoped_step!(
            scope,
            "resume the service",
            resume_service(exec, m, unit, state).await
        );
    } else if matches!(m.backup_pause, BackupPause::Chassis) {
        // fix-171 round 3: "resume the service" is in the plan whenever
        // pausing is possible at all (BackupPause != Off), but Chassis can
        // decide there was nothing running to pause (`NothingRunning`) — a
        // live decision the manifest alone cannot predict. Marked skipped
        // rather than left silent, so n still reaches the announced m.
        scope.skip("resume the service");
    }
    if let Some(note) = &state.resume_note {
        scope.log(Level::Info, format!("[backup] {} {}", unit, note));
    }
    snapshot_result.map(|_| ()).map_err(|err| StepFailure {
        step: scope.qualify("snapshot"),
        err,
    })
}

async fn backup_impl<'a>(
    ctx: &OpCtx<'a>,
    (m, cfg): (&NativeServiceManifest, &BackupCfg),
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let unit = format!("{}.service", m.unit);

    let own_copy = prepare_phase(ctx, exec, (m, cfg), scope).await?;
    let mut state = pause_phase(exec, (m, cfg), &unit, scope).await?;
    snapshot_phase(
        exec,
        (m, cfg),
        &unit,
        own_copy.as_deref(),
        &mut state,
        scope,
    )
    .await?;

    let tiers = effective_tiers(ctx, cfg, m).await;
    scoped_step!(
        scope,
        "retention",
        retention(exec, cfg, m, &tiers, ctx.now_unix).await
    );
    scope.log(
        Level::Info,
        format!(
            "[backup] {} (native, in-container) snapshot complete",
            m.stack_name
        ),
    );
    Ok(())
}

/// C7 nightly backup for one native service (see the module doc).
pub async fn backup_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    cfg: &BackupCfg,
) -> OperationReport {
    let mut scope = Scope::top(&format!("backup-{}", m.stack_name), ctx.sink, ctx.journal);
    if keeps_nothing(m) {
        scope.log(
            Level::Info,
            format!(
                "[backup] {} is stateless — nothing to archive, no repository",
                m.unit
            ),
        );
        return scope.finish(Ok(()));
    }
    // fix-171 round 3: the pause step's own name, and whether a resume step
    // can ever follow it, are both decided by `m.backup_pause` — a fixed
    // field of the manifest, known before anything runs. Whether "resume
    // the service" actually fires (Chassis can find nothing running to
    // pause) is a live decision, so a run that skips it still marks it.
    scope.plan_if_top(&backup_native_plan_names(m));
    let result = backup_impl(ctx, (m, cfg), &mut scope).await;
    scope.finish(result)
}
