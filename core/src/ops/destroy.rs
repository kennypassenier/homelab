//! Gated destroy (C2): the only operation that removes a container. Guarded
//! four ways — typed-name confirmation (checked by the caller/TUI), the A1
//! no-touch list, the A2 hostname guard (live), and it lifts the Proxmox
//! protection flag deliberately before removal. Every step is journaled (B5).

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

/// Destroy a managed container by stack name + vmid. `confirmed` must be the
/// caller's proof the user typed the stack name (the TUI enforces this); we
/// re-check it here so the core is safe on its own.
pub async fn destroy(
    ctx: &OpCtx<'_>,
    stack_name: &str,
    vmid: u16,
    confirmed_name: &str,
) -> OperationReport {
    let op = format!("destroy-{}", stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let vm = vmid.to_string();
    let expected_hostname = format!("{}-app-{}", vmid, stack_name);

    runner.log(
        Level::Warn,
        format!(
            "[destroy] requested for {} (vmid {})",
            expected_hostname, vmid
        ),
    );

    // Gate 0: typed-name confirmation.
    step!(runner, "confirm", {
        if confirmed_name != stack_name {
            return Err(CoreError::SafetyAbort(format!(
                "typed name '{}' does not match stack '{}'",
                confirmed_name, stack_name
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Gate 1: no-touch list (A1).
    step!(runner, "no-touch check", {
        if ctx.safety.no_touch.contains(&vmid) {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} is on the no-touch list — never destroyed",
                vmid
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Gate 2: live hostname guard (A2) — the container must actually be ours.
    step!(runner, "hostname guard", {
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        if !cfg.success() {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} does not exist — nothing to destroy",
                vmid
            )));
        }
        let live = cfg
            .stdout
            .lines()
            .find(|l| l.starts_with("hostname:"))
            .map(|l| l.trim_start_matches("hostname:").trim().to_string())
            .unwrap_or_default();
        if live != expected_hostname {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} has hostname '{}', expected '{}' — refusing to destroy",
                vmid, live, expected_hostname
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Stop the container (ignore "already stopped").
    step!(runner, "stop container", {
        let _ = exec.run(&Cmd::new("pct", &["stop", &vm], 120)).await?;
        Ok(StepOutcome::Changed)
    });

    // Lift the Proxmox protection flag deliberately (it exists precisely to
    // block accidental destroys).
    step!(runner, "lift protection", {
        let _ = exec
            .run(&Cmd::new("pct", &["set", &vm, "--protection", "0"], 30))
            .await?;
        Ok(StepOutcome::Changed)
    });

    // Destroy — purge removes it from all configs; the guest disk on the thin
    // pool goes with it. /appdata data on the host is intentionally kept.
    step!(runner, "destroy container", {
        run_ok(exec, &Cmd::new("pct", &["destroy", &vm, "--purge"], 120)).await?;
        Ok(StepOutcome::Changed)
    });

    // Remove the traefik route fragment if this stack had one.
    step!(runner, "remove gateway route", {
        let dest = format!(
            "{}/{}-app-{}.yml",
            ctx.safety.gateway_routes_dir, vmid, stack_name
        );
        let _ = exec
            .run(&Cmd::new(
                "pct",
                &[
                    "exec",
                    &ctx.safety.gateway_vmid.to_string(),
                    "--",
                    "rm",
                    "-f",
                    &dest,
                ],
                30,
            ))
            .await?;
        Ok(StepOutcome::Changed)
    });

    // Drop the stack from HOST state (B4). Its /appdata + secrets vault are
    // kept so a redeploy can auto-restore (E3).
    step!(runner, "update state", {
        let store = crate::state::StateStore::new(exec, &ctx.state_dir);
        let mut state = store.load().await;
        state.stacks.remove(stack_name);
        store.save(state).await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        format!(
            "[destroy] {} removed (data under /appdata kept for redeploy)",
            expected_hostname
        ),
    );
    runner.finish_ok()
}
