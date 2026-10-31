//! Writing the snapshot of each repository, and uploading it afterwards.

use crate::error::CoreError;
use crate::executor::Stall;
use crate::runner::StepOutcome;

use super::job::Job;
use super::progress::{run_restic_bytes, run_restic_copy, snapshot_is_empty};
use super::repo_init::scope_log_warn;
use super::restic::restic;

impl Job<'_, '_> {
    /// Snapshot every repository of the stack.
    pub(super) async fn snapshot_all(&self, run_tag: &str) -> Result<StepOutcome, CoreError> {
        if self.groups.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        for (owner, paths) in &self.groups {
            self.snapshot_one(owner, paths, run_tag).await?;
        }
        Ok(StepOutcome::Changed)
    }

    /// ask-kp11be-5: --json without --quiet, so restic reports its status
    /// (bytes done of total) every few seconds. Those lines go to the sink as
    /// byte counters and are dropped from the output (`run_restic_bytes`), so
    /// the operation log stays free of progress json; the summary is the line
    /// kept. fix-112: every repository of one stack's night carries the same
    /// tag, so a restore can take one night across all of them.
    async fn snapshot_one(
        &self,
        owner: &str,
        paths: &[String],
        run_tag: &str,
    ) -> Result<(), CoreError> {
        let cfg = self.cfg;
        let mut args = vec![
            "backup",
            "--json",
            "--tag",
            run_tag,
            "--tag",
            cfg.trigger.tag(),
        ];
        args.extend(paths.iter().map(String::as_str));
        let cmd = restic(
            &self.primary,
            owner,
            &cfg.password_file,
            &args,
            cfg.snapshot_timeout_s,
        )
        .stall(Stall::new(cfg.stall_s));
        let op = format!("backup-{}", self.m.stack_name);
        let out = run_restic_bytes(self.exec, self.ctx.sink, &op, owner, cmd).await?;
        refuse_empty_snapshot(&out.stdout, owner, paths)
    }

    /// backup-local-first: the snapshot leaves the house with the apps
    /// running. A failed or stalled upload is reported and does not fail the
    /// backup: the snapshot exists locally, and the nightly offsite pass
    /// (`secondcopy::copy_all`) copies whatever the remote still lacks.
    pub(super) async fn upload(&self) -> Result<StepOutcome, CoreError> {
        let Some(lb) = &self.local_base else {
            return Ok(StepOutcome::Unchanged);
        };
        let mut failed = Vec::new();
        for (owner, _) in &self.groups {
            if let Err(e) = self.upload_one(lb, owner).await {
                failed.push(format!("{owner} ({e})"));
            }
        }
        if failed.is_empty() {
            return Ok(StepOutcome::Changed);
        }
        scope_log_warn(
            self.ctx.sink,
            format!(
                "[upload] {} stays local tonight; the nightly offsite copy tries again :: {}",
                self.m.stack_name,
                failed.join("; ")
            ),
        );
        Ok(StepOutcome::Unchanged)
    }

    async fn upload_one(&self, lb: &str, owner: &str) -> Result<(), CoreError> {
        let cfg = self.cfg;
        let local = format!("{}/{}-config", lb, owner);
        let unlock = restic(
            &cfg.restic_base,
            owner,
            &cfg.password_file,
            &["unlock"],
            120,
        );
        let _ = self.exec.run(&unlock).await;
        let copy = restic(
            &cfg.restic_base,
            owner,
            &cfg.password_file,
            &[
                "copy",
                "--from-repo",
                &local,
                "--from-password-file",
                &cfg.password_file,
            ],
            cfg.snapshot_timeout_s,
        )
        .stall(Stall::new(cfg.stall_s));
        run_restic_copy(self.exec, copy).await
    }
}

/// A restic run over a directory that exists and is empty succeeds, writes a
/// snapshot containing nothing, and reports success. The record then says the
/// stack is backed up and the restore has nothing to give back — a green
/// result that proves the wrong thing. A path that does not exist already
/// fails loudly (rc=1); an empty one is the case nothing else catches.
fn refuse_empty_snapshot(stdout: &str, owner: &str, paths: &[String]) -> Result<(), CoreError> {
    if !snapshot_is_empty(stdout) {
        return Ok(());
    }
    Err(CoreError::Command {
        rendered: format!("restic backup {}", owner),
        detail: format!(
            "the snapshot for '{}' contains no files :: it covered {} — check the path holds what you think it does, because a restore from this gives back nothing",
            owner,
            paths.join(", ")
        ),
    })
}
