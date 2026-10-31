//! C7 update supervision: the app updates itself (`update_cmd`, Kenny's
//! route C+); around it the homelab preserves the running binary, restarts
//! into the new one only when the binary actually changed, verifies health,
//! and rolls back from OUTSIDE the app when the new version does not come up.

use super::steps::guard_step;
use super::update_current::already_current;
use super::update_steps::{
    Paths, drop_kit_copy, keep_one_previous, preserve_binary, read_installed, restart_if_changed,
    run_self_update,
};
use crate::executor::{Executor, TracingExecutor};
use crate::native::{NativeServiceManifest, UpdatePolicy};
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Runner, Scope, StepFailure};
use crate::sink::Level;

/// fix-371-1: why Update (native) did nothing for a service with no
/// `update_cmd`, in one line, pointing at what does install a newer
/// version. A `manual` service is updated only on a person's word, which is
/// Install newest (the signed release); any other names the missing verb.
pub fn update_skip_note(m: &NativeServiceManifest) -> String {
    match m.update_policy {
        UpdatePolicy::Manual => format!(
            "{} has no update of its own (update_policy: manual, no update_cmd); \
             Install newest installs its newest signed release",
            m.unit
        ),
        _ => format!(
            "{} has no update_cmd in the host's copy of its stack file; \
             Install newest installs its newest signed release",
            m.unit
        ),
    }
}

/// one-process-batch-2: `update_native`'s step plan once the unit declares an
/// `update_cmd` — fixed from there on (an already-current release skip-marks
/// the rest).
pub const UPDATE_NATIVE_STEPS: &[&str] = &[
    "guard target",
    "read the installed binary",
    "preserve binary",
    "run self-update",
    "restart if changed",
    "keep one previous binary",
    "drop the kit's own rollback copy",
];

/// one-process-batch-2: `update_native`'s step plan for one service — empty
/// without an `update_cmd` (the op then plans nothing and ends with a note).
pub fn update_native_plan_names(m: &NativeServiceManifest) -> Vec<String> {
    if m.update_cmd.is_none() {
        return Vec::new();
    }
    UPDATE_NATIVE_STEPS.iter().map(|s| s.to_string()).collect()
}

/// The steps after the guard: preserve, self-update, restart, tidy up.
async fn supervise<'a>(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    update_cmd: &str,
    before: &str,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let p = Paths {
        unit: format!("{}.service", m.unit),
        prev: format!("{}.homelab-prev", m.binary),
    };
    scoped_step!(scope, "preserve binary", preserve_binary(exec, m, &p).await);
    scoped_step!(
        scope,
        "run self-update",
        run_self_update(exec, m, update_cmd).await
    );
    scoped_step!(
        scope,
        "restart if changed",
        restart_if_changed(exec, m, &p, before).await
    );
    let mut stale_kept = false;
    scoped_step!(
        scope,
        "keep one previous binary",
        keep_one_previous(exec, m, &p, &mut stale_kept).await
    );
    if stale_kept {
        scope.log(
            Level::Warn,
            format!(
                "could not settle the kept previous binary at {} — check it and {}.old by hand",
                p.prev, p.prev
            ),
        );
    }
    tidy_phase(exec, m, scope).await
}

/// fix-10: drop the chassis kit's own `.prev` once the service has proven
/// healthy on the new binary.
async fn tidy_phase<'a>(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let kit_prev = format!("{}.prev", m.binary);
    let mut kit_kept = false;
    scoped_step!(
        scope,
        "drop the kit's own rollback copy",
        drop_kit_copy(exec, m, &kit_prev, &mut kit_kept).await
    );
    if kit_kept {
        let note = format!("could not remove {kit_prev} — the kit's copy stays on disk");
        scope.log(Level::Warn, note);
    }
    Ok(())
}

async fn update_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    update_cmd: &str,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    scoped_step!(scope, "guard target", guard_step(ctx, exec, m).await);
    let mut before = String::new();
    scoped_step!(
        scope,
        "read the installed binary",
        read_installed(exec, m, &mut before).await
    );
    if already_current(scope, exec, m, &before).await {
        return Ok(());
    }
    supervise(exec, m, update_cmd, &before, scope).await?;
    scope.log(
        Level::Info,
        format!("[update] {} self-update supervised — healthy", m.stack_name),
    );
    Ok(())
}

/// `stored_at` is when the host last wrote the manifest this runs from —
/// `StackState::applied_at`, passed in because core never reads a clock. It is
/// only used to make the skip message below say where its facts come from.
pub async fn update_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    stored_at: Option<u64>,
) -> OperationReport {
    let op = format!("update-{}", m.stack_name);
    let Some(update_cmd) = m.update_cmd.clone() else {
        return skip_without_update_cmd(ctx, m, &op, stored_at);
    };
    // fix-171 round 3: `m.update_cmd` is a manifest field, so by this point
    // the run is committed to the full fixed plan — whether it stops early
    // because the release is already current (a live GitHub decision) is
    // handled by skip-marking the rest.
    let mut scope = Scope::top(&op, ctx.sink, ctx.journal);
    scope.plan_if_top(UPDATE_NATIVE_STEPS);
    let result = update_impl(ctx, m, &update_cmd, &mut scope).await;
    scope.finish(result)
}

/// This used to read "skipped by decision", which is what an empty field
/// looks like from the inside and a deliberate choice from the outside. On
/// 2026-09-10 it said that three times about three services whose stack
/// files all carried an update_cmd: the host was reading a copy stored
/// before those files were written. A service left on an old version for
/// months while the nightly round reports every night that this is intended
/// is never found, so the message now names the copy it read and how to
/// refresh it.
fn skip_without_update_cmd(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    op: &str,
    stored_at: Option<u64>,
) -> OperationReport {
    let runner = Runner::new(op, ctx.sink, ctx.journal);
    let when = match stored_at {
        Some(t) => format!("stored on {}", crate::state::ymd(t)),
        None => "stored at an unrecorded time".to_string(),
    };
    runner.log(
        Level::Info,
        format!(
            "[update] {} skipped: the manifest the host has {} carries no update_cmd. \
             This is the host's copy, not what the repository says — if the stack file \
             has one, `homelab adopt <path to the service>` refreshes it.",
            m.stack_name, when
        ),
    );
    // fix-371-1 (CORRECTION, Kenny 2026-10-04): the run's outcome says
    // why nothing happened and what does update it.
    runner.finish_noted(update_skip_note(m))
}
