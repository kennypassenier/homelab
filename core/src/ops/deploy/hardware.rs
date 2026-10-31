//! W1: refuse hardware this host cannot give, and read the group ids
//! instead of assuming them.

use crate::error::CoreError;
use crate::executor::Cmd;
use crate::runner::StepOutcome;

use super::memory::{MIN_MEMORY_AVAILABLE, memory_available_fraction};
use super::run::{Run, log_info};

impl Run<'_, '_> {
    /// Before the storage step, because a stack that cannot work here should
    /// not leave directories behind either.
    pub(super) async fn hardware_readiness(&mut self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, exec) = self.parts();
        if m.lxc.gpu {
            self.gpu = Some(crate::ops::hardware::check_gpu(exec, &m.stack_name).await?);
        }
        if m.lxc.vpn {
            crate::ops::hardware::check_tun(exec, &m.stack_name).await?;
        }
        // M1: the directories this stack borrows rather than owns.
        crate::ops::hardware::check_data_mounts(exec, &m.stack_name, &m.data_mounts).await?;
        self.check_memory().await?;
        Ok(StepOutcome::Unchanged)
    }

    /// Correction corr-kuma-memory (Kenny, 2026-09-27): a deploy that
    /// (re)starts services in a container already short of memory tips it
    /// into thrashing; on 2026-09-27 CT 107 stopped answering for eight
    /// minutes that way. An existing container with under 15% available
    /// is refused before anything moves. A container that does not exist
    /// yet answers nothing here and is not held back.
    async fn check_memory(&self) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let meminfo = exec
            .run(&Cmd::new(
                "pct",
                &["exec", &m.vmid.to_string(), "--", "cat", "/proc/meminfo"],
                20,
            ))
            .await;
        if let Ok(out) = meminfo
            && let Some(avail) = memory_available_fraction(&out.stdout)
            && avail < MIN_MEMORY_AVAILABLE
        {
            return Err(CoreError::SafetyAbort(format!(
                "{} has {:.0}% of its memory available, under the {:.0}% a deploy \
                 needs :: raise memory_mb in the stack file and run `homelab resize \
                 stacks/{}` first, then deploy",
                m.hostname,
                avail * 100.0,
                MIN_MEMORY_AVAILABLE * 100.0,
                m.stack_name
            )));
        }
        Ok(())
    }

    /// Says which group ids the GPU passthrough will use.
    pub(super) fn log_gpu(&self) {
        if let Some(g) = &self.gpu {
            log_info(
                self.ctx,
                format!(
                    "[w1] {} gid {} · {} gid {}",
                    g.card, g.card_gid, g.render, g.render_gid
                ),
            );
        }
    }
}
