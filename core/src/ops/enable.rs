//! H8 (light variant): per-stack enabled flag. Disabling a stack (a) makes
//! the nightly scheduler skip it and (b) clears onboot so a parked service
//! stays parked across a host reboot — the one thing a manual `pct stop`
//! cannot give you. Enabling restores onboot to what the manifest wants.
//! The flag NEVER starts or stops containers: manual Proxmox actions are
//! always respected.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
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

pub async fn set_enabled(ctx: &OpCtx<'_>, stack_name: &str, enabled: bool) -> OperationReport {
    let op = format!(
        "{}-{}",
        if enabled { "enable" } else { "disable" },
        stack_name
    );
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let store = crate::state::StateStore::new(exec, &ctx.state_dir);

    let mut vmid = 0u16;
    let mut hostname = String::new();
    let mut want_onboot = true;

    step!(runner, "load state", {
        let state = store.load().await?;
        let rec = state.stacks.get(stack_name).ok_or_else(|| {
            CoreError::Other(format!("stack '{}' is not in host state", stack_name))
        })?;
        vmid = rec.vmid;
        hostname = rec.hostname.clone();
        // On enable, onboot goes back to whatever the manifest declares.
        want_onboot = rec.manifest.as_ref().map(|m| m.boot.onboot).unwrap_or(true);
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "guard target", {
        super::guard_target(exec, &ctx.safety, vmid, &hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "set onboot", {
        let vm = vmid.to_string();
        let onboot = if enabled && want_onboot { "1" } else { "0" };
        run_ok(
            exec,
            &Cmd::new("pct", &["set", &vm, "--onboot", onboot], 30),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "persist flag", {
        let mut state = store.load().await?;
        let rec = state.stacks.get_mut(stack_name).ok_or_else(|| {
            CoreError::Other(format!("stack '{}' vanished from host state", stack_name))
        })?;
        if rec.enabled == enabled {
            return Ok(StepOutcome::Unchanged);
        }
        rec.enabled = enabled;
        store.save(state).await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        if enabled {
            format!(
                "[enable] {} back in the nightly rotation (onboot restored)",
                stack_name
            )
        } else {
            format!(
                "[disable] {} parked — nightly runs skip it, onboot off; containers left as they are",
                stack_name
            )
        },
    );
    runner.finish_ok()
}
