//! Owner decision 2026-09-30 (item 2): one button on the host-settings
//! form, "Save and restart the host", for a `host.toml` change marked
//! `Apply::Restart` — until now it only took effect at the daemon's next
//! start, so Kenny had to trigger a second host update to load it.
//!
//! This reuses the exact restart mechanics `selfupdate::self_update` uses
//! for its own restart: a transient systemd unit, `--on-active=2`, so the
//! RPC's reply reaches the client before this process is killed. Unlike
//! self-update, nothing is replaced and no rollback marker is armed —
//! there is nothing to roll back to, only the running binary restarting
//! under the config it already wrote.

use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

/// The systemd service `restart_host` restarts.
pub const SERVICE: &str = "homelab-host";

pub async fn restart_host(ctx: &OpCtx<'_>) -> OperationReport {
    let mut runner = Runner::new("restart-host", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    // Scheduled, not run in line: the reply to the client (and every
    // notice it triggers) must go out before this process is killed, the
    // same reasoning as self-update's own "schedule restart" step.
    step!(runner, "schedule restart", {
        let unit = format!("homelab-restart-{}", ctx.now_unix);
        run_ok(
            exec,
            &Cmd::new(
                "systemd-run",
                &[
                    "--unit",
                    &unit,
                    "--on-active=2",
                    "systemctl",
                    "restart",
                    "--no-block",
                    SERVICE,
                ],
                30,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Warn,
        "[restart-host] restarting in 2s to load the saved configuration".to_string(),
    );
    runner.finish_ok()
}
