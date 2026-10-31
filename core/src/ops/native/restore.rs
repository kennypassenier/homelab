//! feat-backup-2: the operator-driven restore of one native unit from a
//! chosen snapshot, gated by typing the stack's name.

use super::restore_steps::{
    reseed_from_restore, safety_copy, safety_gates, start_unit, stop_unit, unpack_snapshot,
};
use crate::executor::{Executor, TracingExecutor};
use crate::native::NativeServiceManifest;
use crate::ops::OpCtx;
use crate::ops::backup::BackupCfg;
use crate::runner::{OperationReport, Scope, StepFailure};
use crate::sink::Level;

/// one-process-batch-2: `restore_native`'s step plan for one service, known
/// from its manifest alone — a unit that declares no data never gets past
/// "safety gates", and `after_restore` is a fixed field of the manifest. The
/// one list the op plans from and the dashboard counts.
pub fn restore_native_plan_names(m: &NativeServiceManifest) -> Vec<String> {
    let mut steps: Vec<&str> = vec!["safety gates"];
    if !(m.stateless || m.data_dirs.is_empty()) {
        steps.push("stop unit");
        steps.push("safety copy");
        steps.push("unpack snapshot");
        if m.after_restore.is_some() {
            steps.push("re-seed from restore");
        }
        steps.push("start unit");
    }
    steps.into_iter().map(String::from).collect()
}

fn restore_banner(m: &NativeServiceManifest, snapshot: &str) -> String {
    format!(
        "[restore] {} (native unit {}) from snapshot '{}'",
        m.stack_name, m.unit, snapshot
    )
}

/// feat-backup-2: `restore_empty_unit`'s pipeline, gated and chosen rather
/// than automatic-and-always-latest: the operator picks a snapshot from the
/// Backups page and confirms by typing the stack's name (the same gate
/// `backup::restore_confirmed` gives the compose path). Unlike
/// `restore_empty_unit` this stops the unit first (a live restore under a
/// running service would interleave the unpack with its own writes) and
/// takes a safety copy of the current data directories on the HOST first,
/// mirroring the compose path's `pre_restore_dir` — so a restore that turns
/// out wrong is not the second data loss of the day.
pub async fn restore_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    cfg: &BackupCfg,
    snapshot: &str,
    confirm: Option<&str>,
) -> OperationReport {
    let mut scope = Scope::top(&format!("restore-{}", m.stack_name), ctx.sink, ctx.journal);
    // fix-171 round 3: known from the manifest alone, before anything runs —
    // a unit that declares no data never gets past "safety gates", and
    // `after_restore` is a fixed field of the manifest, not a runtime
    // decision.
    scope.plan_if_top(&restore_native_plan_names(m));
    scope.log(Level::Warn, restore_banner(m, snapshot));
    let result = restore_impl(ctx, m, (cfg, snapshot), confirm, &mut scope).await;
    scope.finish(result)
}

async fn restore_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    (cfg, snapshot): (&BackupCfg, &str),
    confirm: Option<&str>,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    scoped_step!(
        scope,
        "safety gates",
        safety_gates(ctx, exec, m, confirm).await
    );
    if m.stateless || m.data_dirs.is_empty() {
        let note = format!("[restore] {} declares no data — nothing to restore", m.unit);
        scope.log(Level::Info, note);
        return Ok(());
    }
    data_phase(ctx, exec, m, (cfg, snapshot), scope).await?;
    scope.log(
        Level::Info,
        format!("[restore] {} restored and restarted", m.unit),
    );
    Ok(())
}

/// Stop the unit, copy the current data aside, unpack the snapshot over it
/// and start the unit again.
async fn data_phase<'a>(
    ctx: &OpCtx<'a>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    (cfg, snapshot): (&BackupCfg, &str),
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    scoped_step!(scope, "stop unit", stop_unit(exec, m).await);
    // fix-64's rule, repeated for the native path: the data about to be
    // overwritten is copied aside on the HOST before anything is unpacked
    // over it, into the same `pre-restore/` area the compose restore uses.
    let aside = format!(
        "{}/pre-restore/{}-{}",
        ctx.state_dir, m.stack_name, ctx.now_unix
    );
    scoped_step!(scope, "safety copy", safety_copy(exec, m, &aside).await);
    let note = format!("[restore] current data copied aside to {aside}");
    scope.log(Level::Info, note);
    scoped_step!(
        scope,
        "unpack snapshot",
        unpack_snapshot(exec, m, cfg, snapshot).await
    );
    if let Some(cmd) = &m.after_restore {
        scoped_step!(
            scope,
            "re-seed from restore",
            reseed_from_restore(exec, m, cmd).await
        );
    }
    scoped_step!(scope, "start unit", start_unit(exec, m).await);
    Ok(())
}
