//! The steps of a supervised native update, one function each.

use super::scripts::{
    drop_stale_rollback_script, health_script, keep_one_previous_script, preserve_script,
    restore_set_aside_script, rollback_script,
};
use crate::error::CoreError;
use crate::executor::Executor;
use crate::native::NativeServiceManifest;
use crate::ops::util::shq;
use crate::ops::util_pct_sh;
use crate::runner::StepOutcome;

/// The names an update works with: the unit and the binary kept aside.
pub(super) struct Paths {
    pub unit: String,
    pub prev: String,
}

/// The sha256 of the installed binary.
async fn installed_sum(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<String, CoreError> {
    let sum = util_pct_sh(
        exec,
        m.vmid,
        &format!("sha256sum {} | cut -d' ' -f1", shq(&m.binary)),
        60,
    )
    .await?;
    Ok(sum.stdout.trim().to_string())
}

/// "read the installed binary": sets `before`.
pub(super) async fn read_installed(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    before: &mut String,
) -> Result<StepOutcome, CoreError> {
    *before = installed_sum(exec, m).await?;
    Ok(StepOutcome::Unchanged)
}

/// "preserve binary": no update without a rollback copy.
pub(super) async fn preserve_binary(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
) -> Result<StepOutcome, CoreError> {
    let out = util_pct_sh(exec, m.vmid, &preserve_script(&m.binary, &p.prev), 60).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "cannot preserve {} — refusing to update without a rollback copy",
            m.binary
        )));
    }
    Ok(StepOutcome::Changed)
}

/// "run self-update": the app updates itself with its own `update_cmd`.
pub(super) async fn run_self_update(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    update_cmd: &str,
) -> Result<StepOutcome, CoreError> {
    let out = util_pct_sh(exec, m.vmid, update_cmd, 900).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "'{}' failed: {} — binary untouched, service still on the old version",
            update_cmd,
            out.stderr.trim()
        )));
    }
    Ok(StepOutcome::Changed)
}

/// "restart if changed": an unchanged binary means no restart and no
/// nightly service blip; a changed one must come up healthy or is rolled
/// back from OUTSIDE the (dead) app.
pub(super) async fn restart_if_changed(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    before: &str,
) -> Result<StepOutcome, CoreError> {
    if installed_sum(exec, m).await? == before {
        return Ok(StepOutcome::Unchanged);
    }
    let out = util_pct_sh(exec, m.vmid, &health_script(&p.unit), 180).await?;
    if out.success() {
        return Ok(StepOutcome::Changed);
    }
    Err(roll_back_update(exec, m, p).await?)
}

/// Restore the preserved binary, restart, and describe the failure loudly
/// either way. The `Ok` is the error to raise.
async fn roll_back_update(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
) -> Result<CoreError, CoreError> {
    let undo = rollback_script(&p.unit, &p.prev, &m.binary);
    let rb = util_pct_sh(exec, m.vmid, &undo, 180).await?;
    // fix-114: the binary running now is the one kept before this run,
    // so the previous one from before it is the kept one again.
    let _ = util_pct_sh(exec, m.vmid, &restore_set_aside_script(&p.prev), 60).await;
    Ok(CoreError::Other(format!(
        "new {} version did not come up healthy — rolled back to the previous binary ({}); \
         investigate before the next nightly run",
        m.stack_name,
        if rb.success() {
            "service restored and active"
        } else {
            "ROLLBACK ALSO FAILED — service needs hands NOW"
        }
    )))
}

/// "keep one previous binary": fix-114: exactly one previous binary stays,
/// the way back when this release misbehaves after its health window. Sets
/// `stale_kept` when it could not be settled.
pub(super) async fn keep_one_previous(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    stale_kept: &mut bool,
) -> Result<StepOutcome, CoreError> {
    let script = keep_one_previous_script(&m.binary, &p.prev);
    let out = util_pct_sh(exec, m.vmid, &script, 60).await?;
    if !out.success() {
        *stale_kept = true;
        return Ok(StepOutcome::Unchanged);
    }
    Ok(StepOutcome::Changed)
}

/// "drop the kit's own rollback copy": fix-10: the chassis kit keeps its own
/// `.prev` beside the binary, and this run keeps `.homelab-prev` — the same
/// version twice, 73 MB of it on CT 109's small rootfs after the first was
/// already gone. Past this point the service has proven healthy on the binary
/// it runs, so the kit's copy has nothing left to return to either; Kenny,
/// 2026-09-19: the deploy removes it after a healthy update. A rolled-back
/// run never reaches here, so the copy the rollback may need is never
/// touched. Sets `kit_kept` when it could not be removed.
pub(super) async fn drop_kit_copy(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    kit_prev: &str,
    kit_kept: &mut bool,
) -> Result<StepOutcome, CoreError> {
    let out = util_pct_sh(exec, m.vmid, &drop_stale_rollback_script(kit_prev), 60).await?;
    if !out.success() {
        *kit_kept = true;
        return Ok(StepOutcome::Unchanged);
    }
    Ok(StepOutcome::Changed)
}
