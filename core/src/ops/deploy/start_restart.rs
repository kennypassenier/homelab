//! A config file changed under an app whose compose definition did not:
//! `up -d` left the container alone, so the running process is still
//! reading the old file. Restart is enough — the file is bind-mounted, so
//! the new content is already visible inside.

use crate::error::CoreError;
use crate::executor::{CmdOutput, pct_sh};

use super::compose::compose_reports_created;
use super::run::{Run, log_info, log_warn};

impl Run<'_, '_> {
    pub(super) async fn restart_if_stale(
        &self,
        app: &str,
        dir: &str,
        up: &CmdOutput,
    ) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        // fix-40: a changed compose file is not proof the container was
        // recreated (a comment-only change recreates nothing). Only
        // compose's own "Recreated"/"Created" line skips the restart.
        let compose_recreated = self.pushed.recreated.contains(app)
            && compose_reports_created(&format!("{}\n{}", up.stdout, up.stderr));
        let older_than_files = !compose_recreated && self.runs_older_files(app, dir).await;
        let restart_this =
            (self.pushed.needs_restart.contains(app) || older_than_files) && !compose_recreated;
        if !restart_this {
            return Ok(());
        }
        let r = pct_sh(
            exec,
            m.vmid,
            &format!("cd {} && docker compose restart", dir),
            300,
        )
        .await?;
        if !r.success() {
            return Err(CoreError::Command {
                rendered: format!("compose restart {}", app),
                detail: r.stderr,
            });
        }
        log_info(
            ctx,
            format!(
                "[config] {} restarted — its config changed and compose would not have",
                app
            ),
        );
        Ok(())
    }

    /// fix-40, the invariant: a container that started before a
    /// file in its app directory last changed runs an older copy of
    /// that file, whether or not this deploy pushed it (a restart that
    /// an earlier deploy missed is caught here too).
    async fn runs_older_files(&self, app: &str, dir: &str) -> bool {
        let (ctx, _, m, exec) = self.parts();
        let probe = pct_sh(
            exec,
            m.vmid,
            &format!(
                "cd {} && for c in $(docker compose ps -q); do \
                 s=$(docker inspect -f '{{{{.State.StartedAt}}}}' \"$c\"); \
                 find . -maxdepth 3 -type f -newermt \"$s\" -print -quit | grep -q . \
                 && echo stale; done; true",
                dir
            ),
            60,
        )
        .await;
        // shell-strings-quoting (2026-09-27), standing rule 12: a probe that
        // could not answer is not "not older". A restart is the safe side of
        // not knowing.
        let unreadable = |why: String| {
            log_warn(
                ctx,
                format!(
                    "[config] {} :: whether it runs older files could not be \
                     read ({}) — restarting it to be sure",
                    app, why
                ),
            );
            true
        };
        match probe {
            Ok(o) if o.success() => o.stdout.contains("stale"),
            Ok(o) => unreadable(format!("rc={}", o.code)),
            Err(e) => unreadable(e.to_string()),
        }
    }
}
