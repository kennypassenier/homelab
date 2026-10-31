//! Host-side /appdata storage (survives container recreation).

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};
use crate::runner::StepOutcome;

use super::run::Run;

impl Run<'_, '_> {
    pub(super) async fn host_storage(&self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, exec) = self.parts();
        for mount in &m.storage {
            run_ok(exec, &Cmd::new("mkdir", &["-p", &mount.host_path], 30)).await?;
            if let Some(uid) = mount.host_owner_uid {
                let owner = format!("{}:{}", uid, uid);
                run_ok(exec, &Cmd::new("chown", &[&owner, &mount.host_path], 60)).await?;
            }
        }
        Ok(if m.storage.is_empty() {
            StepOutcome::Unchanged
        } else {
            StepOutcome::Changed
        })
    }
}
