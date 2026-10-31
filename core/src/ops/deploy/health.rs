//! B3: no green light without proof.

use crate::error::CoreError;
use crate::executor::{pct_sh, shq};
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    pub(super) async fn verify_health(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        exec.sleep_ms(5000).await;
        for app in &m.apps {
            if let Some(reason) = self.not_running_reason(app).await? {
                let diag = self.compose_diagnostics(app).await;
                return Err(CoreError::Command {
                    rendered: format!("verify {}", app),
                    detail: format!("{}\n{}", reason, diag),
                });
            }
            log_info(ctx, format!("[gate] {} :: running", app));
        }
        Ok(StepOutcome::Unchanged)
    }

    /// fix-132/fix-133: `docker compose ps --format json` through the
    /// Unknown-carrying parser, instead of matching the plain-text
    /// table (`--status running --services`) and reading an empty
    /// answer as "nothing running" — which is also what a probe that
    /// could not even ask compose looks like.
    async fn not_running_reason(&self, app: &str) -> Result<Option<String>, CoreError> {
        let (_, _, m, exec) = self.parts();
        let running =
            crate::ops::util::compose_running_services(exec, m.vmid, &m.stack_name, app, "")
                .await?;
        Ok(match &running {
            Some(r) if !r.is_empty() => None,
            Some(_) => Some("no running services".to_string()),
            None => Some("could not read `docker compose ps` for this app".to_string()),
        })
    }

    async fn compose_diagnostics(&self, app: &str) -> String {
        let (_, _, m, exec) = self.parts();
        let dir = shq(&format!("/opt/{}/{}", m.stack_name, app));
        pct_sh(
            exec,
            m.vmid,
            &format!(
                "cd {} && docker compose ps -a && docker compose logs --tail 20",
                dir
            ),
            60,
        )
        .await
        .map(|o| o.stdout)
        .unwrap_or_default()
    }
}
