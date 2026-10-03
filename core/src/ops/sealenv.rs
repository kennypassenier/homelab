//! redesign-stackhub-2 (Kenny approved the stack hub demo, 2026-10-03): the
//! hub's "no sealed env" row offers "Push the env…" as one host action. It
//! copies every secret file on the stack's container that has no copy in
//! the host's vault yet ([`super::facts::unsealed_secret_pairs`], the same
//! list the doctor and the fleet's "Env sealed" verdict read) into the
//! vault, each through the adopt flow's own "seal env file" step
//! ([`super::native::seal_one`]): read without echoing, nothing in the
//! container written.

use crate::error::CoreError;
use crate::executor::{Executor, TracingExecutor};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

pub async fn seal_env(ctx: &OpCtx<'_>, stack_name: &str) -> OperationReport {
    let op = format!("seal-env-{}", stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    // fix-171 round 3: fixed and unconditional.
    runner.plan(&["load state", "guard target", "seal env files"]);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let store = crate::state::StateStore::new(exec, &ctx.state_dir);

    let mut rec: Option<crate::state::StackState> = None;
    let mut sealed: Vec<String> = Vec::new();

    step!(runner, "load state", {
        let state = store.load().await?;
        let r = state.stacks.get(stack_name).ok_or_else(|| {
            CoreError::Other(format!("stack '{}' is not in host state", stack_name))
        })?;
        rec = Some(r.clone());
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "guard target", {
        let r = rec.as_ref().expect("loaded");
        super::guard_target(exec, &ctx.safety, r.vmid, &r.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "seal env files", {
        let r = rec.as_ref().expect("loaded");
        let pairs = super::facts::unsealed_secret_pairs(exec, &ctx.state_dir, stack_name, r).await;
        for (on_container, in_vault) in pairs {
            if super::native::seal_one(exec, r.vmid, &on_container, &in_vault).await? {
                sealed.push(on_container);
            }
        }
        Ok(if sealed.is_empty() {
            StepOutcome::Unchanged
        } else {
            StepOutcome::Changed
        })
    });

    runner.log(
        Level::Info,
        if sealed.is_empty() {
            format!(
                "[seal-env] {}: every secret file already has a vault copy",
                stack_name
            )
        } else {
            format!(
                "[seal-env] {}: sealed {} into the host's vault",
                stack_name,
                sealed.join(", ")
            )
        },
    );
    runner.finish_ok()
}
