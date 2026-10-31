//! D3: garbage-collect what the stack's files no longer declare.
//!
//! Files the container has and the repository does not. The garbage
//! collection works per APP DIRECTORY; a FILE that leaves an app used to
//! take nothing with it — thirteen Grafana dashboards deleted from the
//! repository on 2026-09-01 were still on CT 104 two deploys later, one
//! with the same uid as its replacement (F161).
//!
//! D84 reported them and left the removing to `homelab prune-orphans`.
//! Kenny's ask-8 (`Automatisch bij deploy`, 2026-09-27) reverses that:
//! what the files no longer declare, the deploy removes — one transcript
//! line per file, so the removal is as visible as the report was. `.env`
//! files come from the vault, never from the repository, and directories
//! the orchestrator fills on behalf of other stacks (the gateway's
//! generated dashboards and routes) are not this stack's files at all.

use crate::error::CoreError;
use crate::executor::{Cmd, pct_sh, run_ok, shq};
use crate::manifest;
use crate::runner::StepOutcome;
use crate::state::StateStore;

use super::orphans::{generated_dirs, orphan_files_keeping};
use super::run::{Run, log_info};

impl Run<'_, '_> {
    /// fix-199: same field-keeping rule — a client whose manifest schema
    /// predates the stack's `apps:` list being diffable for departures must
    /// not have every app it cannot name treated as "left the stack".
    pub(super) fn apps_removal_known(&self) -> bool {
        let known = manifest::client_knows(self.spec.client_schema, "apps");
        if !known {
            log_info(
                self.ctx,
                format!(
                    "[orphan files] skipped — the deploying client (schema {}) predates the \
                     declared-apps list; nothing was retired on its behalf",
                    self.spec.client_schema
                ),
            );
        }
        known
    }

    pub(super) async fn orphan_files(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        if m.native_only || !self.apps_removal {
            return Ok(StepOutcome::Unchanged);
        }
        let orphans = self.unclaimed_orphans().await;
        if orphans.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let vm = m.vmid.to_string();
        for o in &orphans {
            let path = format!("/opt/{}/{}", m.stack_name, o);
            // -f, never -r: FILES the repository dropped. A directory would
            // take whatever else is under it.
            run_ok(
                exec,
                &Cmd::new("pct", &["exec", &vm, "--", "rm", "-f", &path], 60),
            )
            .await?;
            log_info(
                ctx,
                format!(
                    "[orphans] removed {} — no longer in the stack's files",
                    path
                ),
            );
        }
        Ok(StepOutcome::Changed)
    }

    /// Orphan files of apps that are staying; an app that left the stack is
    /// the garbage collector's below: its compose file has to still be there
    /// when `docker compose down` runs in its directory, or its containers
    /// keep running with nothing left to stop them by.
    async fn unclaimed_orphans(&self) -> Vec<String> {
        let (ctx, spec, m, exec) = self.parts();
        let keep = generated_dirs(&ctx.safety, m.vmid);
        let leaving: Vec<&String> = self
            .prior
            .as_ref()
            .map(|p| p.apps.iter().filter(|a| !m.apps.contains(a)).collect())
            .unwrap_or_default();
        orphan_files_keeping(exec, m, spec, &keep)
            .await
            .into_iter()
            .filter(|o| {
                let app = o.split('/').next().unwrap_or("");
                !leaving.iter().any(|a| a.as_str() == app)
            })
            .collect()
    }

    /// D3: stop and remove the compose project and /opt dir of apps removed
    /// from intent; /appdata config dirs are kept.
    pub(super) async fn garbage_collect(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        // Nor is there anything to garbage-collect: an app directory under
        // /opt on a native-only container was never put there by a deploy.
        // fix-199: nor while the deploying client predates the declared-apps
        // list (`apps_removal`) — the same field-keeping rule.
        if m.native_only || !self.apps_removal {
            return Ok(StepOutcome::Unchanged);
        }
        let state = StateStore::new(exec, &ctx.state_dir).load().await?;
        let removed: Vec<String> = state
            .stacks
            .get(&m.stack_name)
            .map(|s| {
                s.apps
                    .iter()
                    .filter(|a| !m.apps.contains(a))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if removed.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        for app in &removed {
            self.remove_app(app).await?;
            log_info(
                ctx,
                format!("[gc] app '{}' removed (config dirs kept)", app),
            );
        }
        Ok(StepOutcome::Changed)
    }

    /// shell-strings-quoting (2026-09-27): the exit code was thrown
    /// away and "removed" logged regardless. It is `rm`'s (the last
    /// command); a failure fails the step, which leaves the app on
    /// record so the next deploy tries again.
    async fn remove_app(&self, app: &str) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let out = pct_sh(
            exec,
            m.vmid,
            &format!(
                "cd {d} && docker compose down --remove-orphans; rm -rf {d}",
                d = shq(&format!("/opt/{}/{}", m.stack_name, app))
            ),
            300,
        )
        .await?;
        if !out.success() {
            return Err(CoreError::Command {
                rendered: format!("remove /opt/{}/{}", m.stack_name, app),
                detail: format!("rc={} :: {}", out.code, out.stderr.trim()),
            });
        }
        Ok(())
    }
}
