//! J1: the BEFORE half of every service's own health checks.
//!
//! Taken early, and not later, because "may rise, never fall" is only true
//! while the two readings are minutes apart — and because everything after
//! the baseline can change what is being measured. A stack that does not
//! exist yet simply reads empty, which the comparison treats as "nothing to
//! compare against" rather than as a fault.

use crate::error::CoreError;
use crate::executor::pct_sh;
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    pub(super) async fn baseline(&mut self) -> Result<StepOutcome, CoreError> {
        let (_, spec, m, exec) = self.parts();
        if !self.exists || spec.checks.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        for (app, sc) in &spec.checks {
            let mut readings = Vec::new();
            // check-turn-off-1: a check turned off is never run.
            for c in sc.checks.iter().filter(|c| c.off.is_none()) {
                let out = pct_sh(exec, m.vmid, &c.command, 120)
                    .await
                    .map(|o| o.stdout.trim().to_string())
                    .unwrap_or_default();
                readings.push((c.name.clone(), out));
            }
            self.baseline.insert(app.clone(), readings);
        }
        Ok(StepOutcome::Unchanged)
    }

    pub(super) fn log_baseline(&self) {
        for (app, readings) in &self.baseline {
            for (name, value) in readings {
                log_info(
                    self.ctx,
                    format!("[baseline] {} · {} = {}", app, name, value),
                );
            }
        }
    }
}
