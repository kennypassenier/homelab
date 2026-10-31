//! Making the new container ready: systemd up, docker installed, the
//! runaway guards and log rotation in place.

use crate::error::CoreError;
use crate::executor::pct_sh;
use crate::ops::guards;
use crate::runner::StepOutcome;

use super::run::Run;

impl Run<'_, '_> {
    pub(super) async fn wait_for_systemd(&self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, exec) = self.parts();
        let mut ready = false;
        for _ in 0..30 {
            let out = pct_sh(
                exec,
                m.vmid,
                "systemctl is-system-running 2>/dev/null || true",
                20,
            )
            .await?;
            let s = out.stdout.trim();
            if s.contains("running") || s.contains("degraded") {
                ready = true;
                break;
            }
            exec.sleep_ms(4000).await;
        }
        if !ready {
            return Err(CoreError::Other(
                "container never reached running/degraded".into(),
            ));
        }
        Ok(StepOutcome::Unchanged)
    }

    pub(super) async fn bootstrap_docker(&self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, exec) = self.parts();
        // A native-only container has no docker and must not be given any:
        // installing it would change the very thing this manifest exists to
        // reproduce exactly.
        if m.native_only {
            return Ok(StepOutcome::Unchanged);
        }
        let probe = pct_sh(exec, m.vmid, "docker --version", 30).await?;
        if probe.success() {
            return Ok(StepOutcome::Unchanged);
        }
        self.install_docker().await?;
        Ok(StepOutcome::Changed)
    }

    async fn install_docker(&self) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let install = pct_sh(
            exec,
            m.vmid,
            "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq curl ca-certificates",
            600,
        )
        .await?;
        if !install.success() {
            return Err(CoreError::Command {
                rendered: "apt-get install prerequisites".into(),
                detail: install.stderr,
            });
        }
        let docker = pct_sh(exec, m.vmid, "curl -fsSL https://get.docker.com | sh", 900).await?;
        if !docker.success() {
            return Err(CoreError::Command {
                rendered: "get.docker.com".into(),
                detail: docker.stderr,
            });
        }
        pct_sh(exec, m.vmid, "systemctl enable --now docker", 120).await?;
        Ok(())
    }

    /// B2 + A7.
    pub(super) async fn runaway_guards(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        guards::apply(
            exec,
            ctx.sink,
            m.vmid,
            !m.native_only,
            ctx.registry_cache.as_ref(),
        )
        .await?;
        Ok(StepOutcome::Unchanged)
    }

    /// fix-24: rotation for logs the stack writes onto a data mount.
    pub(super) async fn log_rotation(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let changed = guards::apply_rotation(
            exec,
            m.vmid,
            &m.stack_name,
            &m.data_mounts,
            ctx.default_log_rotation.as_ref(),
        )
        .await?;
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }
}
