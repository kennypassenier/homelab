//! app-settings-enforced-1: the values a fix depends on, put back in the
//! apps' own files (the update and the nightly round do the same from the
//! state this deploy registers).

use crate::error::CoreError;
use crate::runner::StepOutcome;

use super::run::Run;

impl Run<'_, '_> {
    /// Never fails the deploy: a setting that cannot be put back is a
    /// warning naming it.
    pub(super) async fn app_settings(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, spec, m, exec) = self.parts();
        let mut put_back = false;
        for (app, sc) in &spec.checks {
            if sc.settings.is_empty() || !m.apps.contains(app) {
                continue;
            }
            put_back |= crate::ops::settings::enforce_or_warn(
                exec,
                ctx.sink,
                m.vmid,
                &m.stack_name,
                app,
                &sc.settings,
            )
            .await
                == StepOutcome::Changed;
        }
        Ok(if put_back {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }
}
