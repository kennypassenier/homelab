//! C7 adoption: take over a hand-built native-service container without
//! restarting it. Verification first — the container must really be what
//! the manifest claims — then the stack is recorded in state so the
//! nightly machinery (backup, update supervision) picks it up.

use super::adopt_record::{describe, record_state, seal_env_file, tag_as_managed};
use super::adopt_verify::{verify_paths, verify_service};
use super::steps::{guard_step, validate_manifest};
use crate::executor::{Executor, TracingExecutor};
use crate::native::NativeServiceManifest;
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Scope, StepFailure};
use crate::sink::Level;

/// fix-171 round 3: adoption's own fixed step plan, announced before its
/// first step. Every step here is unconditional — a precondition that does
/// not hold (no env file to seal) makes the step a no-op, never an absent
/// one — so this list is the same for every run, like `INSTALL_STEPS`.
pub const ADOPT_STEPS: &[&str] = &[
    "validate manifest",
    "guard target",
    "verify service",
    "verify paths",
    "tag as managed",
    "describe",
    "seal env file",
    "record state",
];

/// fix-step-plan-nested: `adopt`'s own step names, qualified the way they
/// are marked when `adopt` runs nested inside another op (`install_native`
/// always ends with one) — the single source both that composer and
/// `admin`'s batch-total use, so neither can drift from what `adopt_impl`
/// actually marks.
pub fn adopt_plan_names(stack_name: &str) -> Vec<String> {
    let prefix = format!("adopt-{}", stack_name);
    ADOPT_STEPS
        .iter()
        .map(|s| format!("{prefix} :: {s}"))
        .collect()
}

/// fix-step-plan-nested: the step logic behind `adopt`, written to run
/// through a `Scope` — as the outermost call (`adopt`, below) or nested
/// inside `install_native`'s own composed plan, sharing its `Runner` and
/// qualifying every mark with `"adopt-<stack>"`.
pub(super) async fn adopt_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    scoped_step!(scope, "validate manifest", validate_manifest(m));
    scoped_step!(scope, "guard target", guard_step(ctx, exec, m).await);
    scoped_step!(scope, "verify service", verify_service(exec, m).await);
    scoped_step!(scope, "verify paths", verify_paths(exec, m).await);
    scoped_step!(scope, "tag as managed", tag_as_managed(exec, m).await);
    scoped_step!(scope, "describe", describe(exec, m).await);
    scoped_step!(scope, "seal env file", seal_env_file(ctx, exec, m).await);
    scoped_step!(scope, "record state", record_state(ctx, exec, m).await);

    scope.log(
        Level::Info,
        format!(
            "[adopt] {} ({}) is now managed — service untouched, nightly backup + update \
             supervision from tonight",
            m.stack_name, m.hostname
        ),
    );
    Ok(())
}

/// Adopt an existing container as a managed native-service stack. Never
/// starts, stops or restarts anything — a running production service is
/// exactly what must stay untouched while the homelab takes ownership.
pub async fn adopt(ctx: &OpCtx<'_>, m: &NativeServiceManifest) -> OperationReport {
    let mut scope = Scope::top(&format!("adopt-{}", m.stack_name), ctx.sink, ctx.journal);
    scope.plan_if_top(ADOPT_STEPS);
    let result = adopt_impl(ctx, m, &mut scope).await;
    scope.finish(result)
}
