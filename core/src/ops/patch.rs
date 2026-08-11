//! Fleet OS patching (H6): apt update + dist-upgrade across every managed
//! container, one at a time (a broken mirror or hung dpkg should stop the
//! run, not brick the whole fleet in parallel). Unattended-upgrades (A7)
//! covers security patches daily; this is the explicit "patch everything
//! now" sweep, including non-security updates.

use crate::executor::{Executor, TracingExecutor};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

const PATCH_SCRIPT: &str = "export DEBIAN_FRONTEND=noninteractive; \
     apt-get update -qq && \
     apt-get dist-upgrade -y -qq -o Dpkg::Options::=--force-confold && \
     apt-get autoremove -y -qq && apt-get clean";

/// Patch the given `(stack_name, vmid)` targets sequentially. The caller
/// (host) builds the list from state.json — only managed stacks, never the
/// no-touch fleet.
pub async fn patch_fleet(ctx: &OpCtx<'_>, targets: &[(String, u16)]) -> OperationReport {
    let mut runner = Runner::new("patch-fleet", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    if targets.is_empty() {
        runner.log(
            Level::Warn,
            "[patch] no managed stacks in state — nothing to do".to_string(),
        );
        return runner.finish_ok();
    }

    for (name, vmid) in targets {
        // Defense in depth: state should never contain a no-touch vmid, but
        // patching writes to the guest, so check anyway.
        if ctx.safety.no_touch.contains(vmid) {
            runner.log(
                Level::Warn,
                format!(
                    "[patch] {} (vmid {}) is on the no-touch list — skipped",
                    name, vmid
                ),
            );
            continue;
        }
        let step_name = format!("patch {}", name);
        step!(runner, &step_name, {
            let out = super::util_pct_sh(exec, *vmid, PATCH_SCRIPT, 1800).await?;
            if !out.success() {
                return Err(crate::error::CoreError::Other(format!(
                    "apt failed in {} (vmid {}): {}",
                    name,
                    vmid,
                    out.stderr.trim()
                )));
            }
            Ok(StepOutcome::Changed)
        });
        runner.log(Level::Info, format!("[patch] {} up to date", name));
    }

    runner.finish_ok()
}
