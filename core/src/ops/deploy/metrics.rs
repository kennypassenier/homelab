//! T1: tell Prometheus this stack exists.

use crate::error::CoreError;
use crate::executor::Cmd;
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    /// Written before the apps start, removed by destroy — so the scrape list
    /// is a consequence of what runs rather than a list somebody maintains
    /// and forgets. Best-effort on purpose: a metrics stack that is down must
    /// never block a deploy.
    pub(super) async fn metrics_discovery(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let Some(dir) = ctx.metrics_targets_dir.as_deref() else {
            return Ok(StepOutcome::Unchanged);
        };
        let path = crate::ops::discovery::target_file(dir, &m.stack_name);
        if self.is_unmeasured().await {
            log_info(
                ctx,
                format!(
                    "[t1] {} is declared unmeasured (metrics: false) — no scrape target",
                    m.stack_name
                ),
            );
            return Ok(StepOutcome::Unchanged);
        }
        // A stack with apps runs docker; a native-service stack does not.
        let body =
            crate::ops::discovery::targets_json(&m.stack_name, &m.network.ip, !m.apps.is_empty());
        if matches!(exec.read_file(&path).await, Ok(existing) if existing == body) {
            return Ok(StepOutcome::Unchanged);
        }
        let _ = exec.run(&Cmd::new("mkdir", &["-p", dir], 30)).await;
        match exec.write_file(&path, &body, 0o644).await {
            Ok(()) => Ok(StepOutcome::Changed),
            Err(e) => {
                log_info(
                    ctx,
                    format!(
                        "[t1] could not write {} ({}) — this stack will not be scraped until it is",
                        path, e
                    ),
                );
                Ok(StepOutcome::Unchanged)
            }
        }
    }

    /// fix-145 (expert panel 2026-09-27, inbox-ct118-unmanaged): a native
    /// stack whose every unit is declared unmeasured (`metrics: false`,
    /// inbox under Kenny's "Grenzen zetten") runs no node exporter, so a
    /// target here would be an endpoint nothing answers on and an alert
    /// that never clears. The fleet check already respects the flag.
    async fn is_unmeasured(&self) -> bool {
        let (ctx, _, m, exec) = self.parts();
        m.apps.is_empty()
            && crate::state::StateStore::new(exec, &ctx.state_dir)
                .load()
                .await
                .ok()
                .and_then(|s| s.stacks.get(&m.stack_name).cloned())
                .is_some_and(|st| {
                    !st.natives.is_empty() && st.natives.iter().all(|n| n.metrics == Some(false))
                })
    }
}
