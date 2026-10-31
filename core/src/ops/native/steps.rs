//! Step bodies that several native operations run identically.

use crate::error::CoreError;
use crate::executor::Executor;
use crate::native::NativeServiceManifest;
use crate::ops::{OpCtx, guard_target};
use crate::runner::StepOutcome;

/// "validate manifest": the manifest must pass `validate_native`.
pub(super) fn validate_manifest(m: &NativeServiceManifest) -> Result<StepOutcome, CoreError> {
    crate::native::validate_native(m)
        .map_err(|p| CoreError::SafetyAbort(format!("native manifest: {}", p.join("; "))))?;
    Ok(StepOutcome::Unchanged)
}

/// "guard target": the container is the one the manifest names and is not
/// on the no-touch list.
pub(super) async fn guard_step(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
    Ok(StepOutcome::Unchanged)
}
