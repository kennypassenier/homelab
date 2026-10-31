//! Tiered retention (G8): which snapshots a repository forgets.

use crate::error::CoreError;
use crate::executor::run_ok;
use crate::runner::StepOutcome;
use crate::sink::{Level, PipelineEvent};

use super::job::Job;
use super::restic::restic;
use super::snapshot_parse::parse_snapshot_lanes;

/// fix-238: the ids retention forgets in one repository, from the raw
/// `restic snapshots --json` listing. Every retention step goes through
/// here so the lanes below cannot be bypassed by one call site.
pub fn retention_doomed(
    raw: &str,
    tiers: &[crate::retention::RetentionTier],
    now: u64,
) -> Vec<String> {
    crate::retention::forget_list_by_lane(&parse_snapshot_lanes(raw), tiers, now)
}

impl Job<'_, '_> {
    /// G8 tiered retention: list snapshots, compute the forget-set with our
    /// own engine, forget by explicit id. Per repository, since D25 gave
    /// every app its own.
    ///
    /// W2: the stack file's own policy wins over the fleet-wide one when it
    /// states one. Resolved here rather than where the config is built, so
    /// every caller gets it without being told to.
    pub(super) async fn retention(&self) -> Result<StepOutcome, CoreError> {
        let tiers = self.m.retention.as_ref().unwrap_or(&self.cfg.tiers);
        if self.m.retention.is_some() {
            self.ctx.sink.emit(PipelineEvent::Line {
                level: Level::Info,
                source: "HOST".into(),
                msg: format!(
                    "[w2] {} keeps snapshots by its own policy ({} tier(s)), not the fleet-wide one",
                    self.m.stack_name,
                    tiers.len()
                ),
            });
        }
        let mut changed = false;
        for (owner, _) in &self.groups {
            changed |= self.forget_in(owner, tiers).await?;
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    /// backup-local-first: the repository the snapshot went to; the remote's
    /// own retention runs in the nightly offsite pass. `true` when it forgot.
    async fn forget_in(
        &self,
        owner: &str,
        tiers: &[crate::retention::RetentionTier],
    ) -> Result<bool, CoreError> {
        let pw = &self.cfg.password_file;
        let list = restic(&self.primary, owner, pw, &["snapshots", "--json"], 300);
        let out = run_ok(self.exec, &list).await?;
        let doomed = retention_doomed(&out.stdout, tiers, self.ctx.now_unix);
        if doomed.is_empty() {
            return Ok(false);
        }
        let mut args: Vec<&str> = vec!["forget"];
        args.extend(doomed.iter().map(|s| s.as_str()));
        args.push("--prune");
        run_ok(self.exec, &restic(&self.primary, owner, pw, &args, 900)).await?;
        Ok(true)
    }
}
