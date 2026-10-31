//! Where a stack's snapshot goes first, and the repositories that must exist.

use crate::error::CoreError;
use crate::runner::{Scope, StepOutcome};
use crate::sink::Level;

use super::job::Job;
use super::repo_init::{InitOutcome, init_repository, same_password_as_host_meta, scope_log_warn};
use super::restic::restic;

impl<'a> Job<'_, 'a> {
    /// backup-local-first (Kenny, 2026-10-22, after the night the internet
    /// dropped mid-upload and Jellyfin stayed paused for 52 minutes): the
    /// snapshot is written to the local repository set first, so the apps
    /// are paused only for a disk-to-disk write; the upload to `restic_base`
    /// follows in "upload", with the apps running. A local dataset that is
    /// configured but not usable falls back to the remote, loudly: a backup
    /// straight to the remote beats none.
    pub(super) async fn choose_primary(&mut self, scope: &mut Scope<'_, 'a>) {
        self.local_base = match &self.cfg.local_dataset {
            Some(ds) => match crate::ops::secondcopy::ensure_dataset(self.exec, ds, false).await {
                Ok(path) => Some(path),
                Err(e) => {
                    scope.log(
                        Level::Warn,
                        format!(
                            "[backup] the local repository set ({ds}) is not usable, so \
                             tonight's snapshot goes straight to the remote with the apps \
                             paused :: {e}"
                        ),
                    );
                    None
                }
            },
            None => None,
        };
        self.primary = self
            .local_base
            .clone()
            .unwrap_or_else(|| self.cfg.restic_base.clone());
    }

    /// gap-24: the answer is read. An existing repository says so and is
    /// the normal case; a new one is checked against host-meta's password.
    pub(super) async fn init_repos(&self) -> Result<StepOutcome, CoreError> {
        let mut created = false;
        for (owner, _) in &self.groups {
            let (remote_created, remote_ok) = self.init_remote(owner).await?;
            created |= remote_created;
            if let Some(lb) = &self.local_base {
                created |= self.init_local_twin(lb, owner, remote_ok).await?;
            }
        }
        Ok(if created {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    /// `(created, reachable)` for the remote repository of `owner`.
    async fn init_remote(&self, owner: &str) -> Result<(bool, bool), CoreError> {
        let cfg = self.cfg;
        let init = restic(&cfg.restic_base, owner, &cfg.password_file, &["init"], 120);
        // backup-local-first: with a local set, an unreachable remote
        // no longer stops the backup; "upload" reports it instead.
        match init_repository(self.exec, &init, owner).await {
            Ok(InitOutcome::Created) => {
                same_password_as_host_meta(self.exec, &cfg.restic_base, &cfg.password_file, owner)
                    .await?;
                Ok((true, true))
            }
            Ok(InitOutcome::Existed) => Ok((false, true)),
            Err(e) if self.local_base.is_some() => {
                scope_log_warn(
                    self.ctx.sink,
                    format!(
                        "[backup] the remote repository for {owner} did not answer; the snapshot stays local until it does :: {e}"
                    ),
                );
                Ok((false, false))
            }
            Err(e) => Err(e),
        }
    }

    /// The local twin shares the remote's chunker parameters, or the copies
    /// between them would deduplicate nothing. `true` when it was created.
    async fn init_local_twin(
        &self,
        lb: &str,
        owner: &str,
        remote_ok: bool,
    ) -> Result<bool, CoreError> {
        let cfg = self.cfg;
        let remote = format!("{}/{}-config", cfg.restic_base, owner);
        let with_params = [
            "init",
            "--from-repo",
            remote.as_str(),
            "--from-password-file",
            cfg.password_file.as_str(),
            "--copy-chunker-params",
        ];
        let args: &[&str] = if remote_ok { &with_params } else { &["init"] };
        let init = restic(lb, owner, &cfg.password_file, args, 300);
        Ok(init_repository(self.exec, &init, owner).await? == InitOutcome::Created)
    }

    /// H2 hardening: a previous run killed mid-snapshot can leave a stale
    /// repo lock; restic unlock only removes locks from dead processes, so
    /// this is always safe. Best-effort (repo may not exist yet).
    pub(super) async fn clear_stale_locks(&self) -> Result<StepOutcome, CoreError> {
        for (owner, _) in &self.groups {
            let unlock = restic(
                &self.primary,
                owner,
                &self.cfg.password_file,
                &["unlock"],
                120,
            );
            let _ = self.exec.run(&unlock).await;
        }
        Ok(StepOutcome::Unchanged)
    }
}
