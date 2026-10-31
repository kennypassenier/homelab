//! fix-114: `homelab rollback-native <stack>/<unit>` — go back to the kept
//! previous binary by hand.

use super::scripts::{health_script, rollback_script};
use super::steps::guard_step;
use crate::error::CoreError;
use crate::executor::{Executor, TracingExecutor};
use crate::native::NativeServiceManifest;
use crate::ops::util::shq;
use crate::ops::{OpCtx, util_pct_sh};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

/// one-process-batch-2: `rollback_native`'s step plan — fixed and
/// unconditional.
pub const ROLLBACK_NATIVE_STEPS: &[&str] = &[
    "guard target",
    "a previous binary is kept",
    "roll back",
    "keep the other as previous",
    "park automatic updates",
];

/// The names a rollback works with: the unit, the kept previous binary and
/// the place the running binary waits while the previous one is tried.
struct Paths {
    unit: String,
    prev: String,
    swap: String,
}

/// "a previous binary is kept": one that differs from the running one.
async fn previous_is_kept(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
) -> Result<StepOutcome, CoreError> {
    let script = format!(
        "test -f {p} && ! cmp -s {b} {p} && echo yes || echo no",
        p = shq(&p.prev),
        b = shq(&m.binary)
    );
    let out = util_pct_sh(exec, m.vmid, &script, 60).await?;
    if out.stdout.trim() != "yes" {
        return Err(CoreError::SafetyAbort(format!(
            "no previous binary of {} is kept at {} (or it is the one running) — nothing \
             was stopped",
            m.unit, p.prev
        )));
    }
    Ok(StepOutcome::Unchanged)
}

/// Stop the unit, keep the running binary aside and copy the kept one in.
/// A failure starts the unit again on the binary it had.
async fn swap_binaries(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
) -> Result<(), CoreError> {
    let script = format!(
        "systemctl stop {u}; cp -p {b} {swap} && cp -p {p} {b}",
        u = p.unit,
        b = shq(&m.binary),
        swap = shq(&p.swap),
        p = shq(&p.prev)
    );
    let out = util_pct_sh(exec, m.vmid, &script, 180).await?;
    if out.success() {
        return Ok(());
    }
    let _ = util_pct_sh(exec, m.vmid, &format!("systemctl start {}", p.unit), 60).await;
    Err(CoreError::Other(format!(
        "could not put the kept binary of {} in place ({}) — the unit was started \
         again on the binary it had",
        m.unit,
        out.stderr.trim()
    )))
}

/// "roll back": the kept binary goes in place and is held to the same health
/// check as an update; if it does not come up, the binary that was running
/// goes back.
async fn roll_back(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
) -> Result<StepOutcome, CoreError> {
    swap_binaries(exec, m, p).await?;
    let health = util_pct_sh(exec, m.vmid, &health_script(&p.unit), 180).await?;
    if health.success() {
        return Ok(StepOutcome::Changed);
    }
    let undo = rollback_script(&p.unit, &p.swap, &m.binary);
    let back = util_pct_sh(exec, m.vmid, &undo, 180).await?;
    Err(CoreError::Other(format!(
        "the kept binary of {} did not come up healthy ({}) — {}",
        m.unit,
        health.stdout.trim(),
        if back.success() {
            "the binary that was running is back and active"
        } else {
            "putting the running binary back ALSO FAILED — the service needs hands NOW"
        }
    )))
}

/// "keep the other as previous": the version rolled back from becomes the
/// kept one, so running it again returns.
async fn keep_other_as_previous(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
) -> Result<StepOutcome, CoreError> {
    let mv = format!("mv -f {} {}", shq(&p.swap), shq(&p.prev));
    let out = util_pct_sh(exec, m.vmid, &mv, 60).await?;
    Ok(if out.success() {
        StepOutcome::Changed
    } else {
        StepOutcome::Unchanged
    })
}

/// "park automatic updates": the stack's automatic updates are parked
/// (fix-59), or the next night would reinstall the release that was just
/// rolled back; `homelab enable <stack>` resumes them.
async fn park_updates(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let now = ctx.now_unix;
    let stack = m.stack_name.clone();
    crate::state::StateStore::new(ctx.exec, &ctx.state_dir)
        .update(|s| {
            s.updates_parked.insert(stack.clone(), now);
        })
        .await?;
    Ok(StepOutcome::Changed)
}

/// fix-114: the unit is stopped, the kept binary copied into place and held
/// to the same health check as an update; if it does not come up, the binary
/// that was running goes back. The version rolled back from becomes the kept
/// one, so running it again returns. The stack's automatic updates are
/// parked (fix-59); `homelab enable <stack>` resumes them.
pub async fn rollback_native(ctx: &OpCtx<'_>, m: &NativeServiceManifest) -> OperationReport {
    let mut runner = Runner::new(&format!("rollback-{}", m.unit), ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let p = Paths {
        unit: format!("{}.service", m.unit),
        prev: format!("{}.homelab-prev", m.binary),
        swap: format!("{}.homelab-rollback", m.binary),
    };

    // fix-171 round 3: fixed and unconditional, like `destroy::STEPS`.
    runner.plan(ROLLBACK_NATIVE_STEPS);
    step!(runner, "guard target", guard_step(ctx, exec, m).await);
    step!(
        runner,
        "a previous binary is kept",
        previous_is_kept(exec, m, &p).await
    );
    step!(runner, "roll back", roll_back(exec, m, &p).await);
    step!(
        runner,
        "keep the other as previous",
        keep_other_as_previous(exec, m, &p).await
    );
    step!(runner, "park automatic updates", park_updates(ctx, m).await);

    runner.log(
        Level::Warn,
        format!(
            "[rollback] {} runs its previous binary again; automatic updates of {} are parked \
             until `homelab enable {}`",
            m.unit, m.stack_name, m.stack_name
        ),
    );
    runner.finish_ok()
}
