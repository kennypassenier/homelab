//! The safety copy a restore takes of the data it is about to overwrite (fix-64).

use crate::error::CoreError;
use crate::executor::{run_ok, shq};
use crate::manifest::StackManifest;
use crate::ops::OpCtx;

use crate::runner::{Runner, StepOutcome};
use crate::sink::Level;

use super::restore_app::RestoreJob;
use super::shell::sh;

/// fix-64: where a restore keeps the data it is about to overwrite.
pub(super) fn pre_restore_dir(ctx: &OpCtx<'_>, m: &StackManifest) -> String {
    format!(
        "{}/pre-restore/{}-{}",
        ctx.state_dir, m.stack_name, ctx.now_unix
    )
}

impl RestoreJob<'_, '_> {
    fn quoted_paths(&self) -> String {
        let quoted: Vec<String> = self.all_paths().iter().map(|p| shq(p)).collect();
        quoted.join(" ")
    }

    /// Every path the restore overwrites.
    pub(super) fn all_paths(&self) -> Vec<String> {
        self.groups
            .iter()
            .flat_map(|(_, paths)| paths.iter().cloned())
            .collect()
    }

    /// Whether there is room is asked before anything is stopped: the copy
    /// needs twice the data's size plus 1 GiB free.
    pub(super) async fn room_for_safety_copy(&self) -> Result<StepOutcome, CoreError> {
        let ctx = self.ctx;
        let script = format!(
            "du -sbc {} | tail -1 | cut -f1; df -B1 --output=avail {} | tail -1; \
             test -e {} && echo exists || true",
            self.quoted_paths(),
            shq(&ctx.state_dir),
            shq(&self.copy_dest)
        );
        let sizes = self.exec.run(&sh(&script, 300)).await?;
        if sizes.stdout.contains("exists") {
            return Err(CoreError::SafetyAbort(format!(
                "{} already exists, and a safety copy is never written over — nothing has \
                 been stopped",
                self.copy_dest
            )));
        }
        let nums: Vec<u64> = sizes
            .stdout
            .split_whitespace()
            .filter_map(|w| w.parse().ok())
            .collect();
        let enough = match nums[..] {
            [size, avail] => avail >= size.saturating_mul(2).saturating_add(1 << 30),
            _ => false,
        };
        if !enough {
            return Err(CoreError::SafetyAbort(format!(
                "no room for a copy of the current data under {} ({}) :: the restore \
                 overwrites it, and a copy needs twice its size plus 1 GiB free. Nothing has \
                 been stopped. Free space, or restore without the copy with \
                 `--no-safety-copy`",
                ctx.state_dir,
                sizes.stdout.trim().replace('\n', " / ")
            )));
        }
        Ok(StepOutcome::Unchanged)
    }

    /// fix-64: the copy, with the stack down so it is consistent. `Ok(true)`
    /// when a copy was taken, `Ok(false)` when none was asked for. A copy that
    /// fails stops the restore; the stack is resumed either way.
    pub(super) async fn take_safety_copy(
        &self,
        runner: &mut Runner<'_>,
    ) -> Result<bool, CoreError> {
        if !self.safety_copy {
            return Ok(false);
        }
        let script = format!(
            "mkdir -p {d} && cp -a --parents {src} {d}/",
            d = shq(&self.copy_dest),
            src = self.quoted_paths()
        );
        let cmd = sh(&script, self.cfg.restore_timeout_s);
        runner
            .step("safety copy", || async {
                run_ok(self.exec, &cmd).await?;
                Ok(StepOutcome::Changed)
            })
            .await?;
        runner.log(
            Level::Warn,
            format!(
                "[restore] the data as it was before this restore is kept at {} — nothing \
                 removes it; delete it by hand once the restore is proven",
                self.copy_dest
            ),
        );
        Ok(true)
    }
}
