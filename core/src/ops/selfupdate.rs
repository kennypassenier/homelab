//! HOST self-update (H5). The client ships a new binary over the TLS line;
//! this op verifies it (`--selfcheck`), backs up the running binary, installs,
//! arms a rollback marker, and schedules a systemd restart of itself. The
//! rollback half lives in systemd: `OnFailure=` runs a script that — if the
//! marker is still present, meaning the new binary never came up healthy —
//! restores the backup. The freshly started binary clears the marker once it
//! is serving, which is what makes an update "accepted".

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

pub struct SelfUpdateCfg {
    /// Where the uploaded candidate binary was staged (mode 0755).
    pub staged: String,
    /// The live binary path systemd starts.
    pub current: String,
    /// Backup of the previous binary, used by the rollback unit.
    pub prev: String,
    /// Marker whose presence at failure time means "roll back"; cleared by
    /// the new binary once it serves.
    pub marker: String,
    /// The systemd service to restart.
    pub service: String,
}

impl Default for SelfUpdateCfg {
    fn default() -> Self {
        Self {
            staged: "/var/lib/homelab/staged-host".into(),
            current: "/usr/local/bin/homelab-host".into(),
            prev: "/usr/local/bin/homelab-host.prev".into(),
            marker: "/var/lib/homelab/selfupdate.pending".into(),
            service: "homelab-host".into(),
        }
    }
}

pub async fn self_update(ctx: &OpCtx<'_>, cfg: &SelfUpdateCfg) -> OperationReport {
    let mut runner = Runner::new("self-update", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    let mut new_version = String::new();

    // Gate: the candidate must prove it can run at all before it replaces
    // anything. A truncated upload or wrong-arch binary dies here.
    step!(runner, "selfcheck candidate", {
        let out = exec
            .run(&Cmd::new(&cfg.staged, &["--selfcheck"], 30))
            .await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "staged binary failed selfcheck (exit {}): {}",
                out.code,
                out.stderr.trim()
            )));
        }
        new_version = out.stdout.trim().to_string();
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "backup current", {
        run_ok(exec, &Cmd::new("cp", &["-a", &cfg.current, &cfg.prev], 30)).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "install candidate", {
        run_ok(
            exec,
            &Cmd::new("install", &["-m", "755", &cfg.staged, &cfg.current], 30),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    // Armed BEFORE the restart: if the new binary never comes up, the
    // OnFailure unit sees this marker and restores `prev`.
    step!(runner, "arm rollback marker", {
        let content = format!(
            "{{\"to_version\":\"{}\",\"armed_at\":{}}}\n",
            new_version, ctx.now_unix
        );
        exec.write_file(&cfg.marker, &content, 0o644).await?;
        Ok(StepOutcome::Changed)
    });

    // Restart via a transient unit so the reply to the client still goes out
    // before this process is killed.
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
                    &cfg.service,
                ],
                30,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Warn,
        format!(
            "[self-update] installing '{}' — restarting in 2s; rollback armed until the new binary reports healthy",
            new_version
        ),
    );
    runner.finish_ok()
}
