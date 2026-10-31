//! Starting the apps: registry sign-in, network, then pull and `up` per app.

use crate::error::CoreError;
use crate::executor::{pct_sh, shq};
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    pub(super) async fn start_apps(&self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, exec) = self.parts();
        // A native-only container has no docker: nothing to network, nothing
        // to start. Without this the deploy created an empty docker network
        // on CT 112 — harmless, and exactly the kind of stray act that makes
        // a reader wonder what else it did.
        if m.native_only {
            return Ok(StepOutcome::Unchanged);
        }
        self.registry_login().await?;
        let net = format!("{}_net", m.stack_name);
        pct_sh(
            exec,
            m.vmid,
            &format!("docker network create {} 2>/dev/null || true", net),
            60,
        )
        .await?;
        for app in &m.apps {
            self.start_app(app).await?;
        }
        Ok(StepOutcome::Changed)
    }

    async fn start_app(&self, app: &str) -> Result<(), CoreError> {
        let m = &self.spec.manifest;
        let dir = shq(&format!("/opt/{}/{}", m.stack_name, app));
        self.pull_images(app, &dir).await?;
        let up = self.compose_up(app, &dir).await?;
        self.restart_if_stale(app, &dir, &up).await
    }

    /// Sign in to a private registry before anything tries to pull from
    /// it. The credentials ride in an app's ordinary .env, already pushed
    /// above, so this adds no new secrets path — it only stops the login
    /// from living in somebody's memory instead of in the manifest.
    ///
    /// Read with grep rather than by sourcing the file: a token with a
    /// parenthesis or a space in it breaks `.` in dash, and that failure
    /// looks like a wrong password.
    async fn registry_login(&self) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let Some(reg) = &m.registry_login else {
            return Ok(());
        };
        let envf = format!("/opt/{}/{}/.env", m.stack_name, reg.app);
        let script = format!(
            "u=$(grep -m1 '^REGISTRY_USER=' {f} | cut -d= -f2-); \
             t=$(grep -m1 '^REGISTRY_TOKEN=' {f} | cut -d= -f2-); \
             [ -n \"$u\" ] && [ -n \"$t\" ] || {{ echo no REGISTRY_USER/REGISTRY_TOKEN in {f} >&2; exit 1; }}; \
             printf %s \"$t\" | docker login {r} -u \"$u\" --password-stdin",
            f = shq(&envf),
            // Validated as a bare host name (`registry_login.registry`).
            r = reg.registry
        );
        let out = pct_sh(exec, m.vmid, &script, 60).await?;
        if !out.success() {
            return Err(CoreError::Command {
                rendered: format!("docker login {}", reg.registry),
                detail: out.stderr,
            });
        }
        log_info(
            ctx,
            format!(
                "[registry] signed in to {} as the credentials in {} say",
                reg.registry, reg.app
            ),
        );
        Ok(())
    }

    async fn compose_up(
        &self,
        app: &str,
        dir: &str,
    ) -> Result<crate::executor::CmdOutput, CoreError> {
        let (_, _, m, exec) = self.parts();
        let up = pct_sh(
            exec,
            m.vmid,
            &format!("cd {} && docker compose up -d --remove-orphans", dir),
            300,
        )
        .await?;
        if !up.success() {
            return Err(CoreError::Command {
                rendered: format!("compose up {}", app),
                detail: up.stderr,
            });
        }
        Ok(up)
    }
}
