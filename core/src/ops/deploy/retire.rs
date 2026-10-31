//! step-22 / ask-8: what left the stack's files leaves the container.
//!
//! The intent repo's copy of the stack is the record of what the last
//! deploy placed, so it is read BEFORE this deploy overwrites it: a file it
//! has and the stack no longer has is a file the stack dropped. Two kinds
//! are acted on here, before anything later in the deploy can fail and
//! leave the record already rewritten:
//!
//! * a `rootfs/` file (a unit, a timer, a script on PATH) is removed from
//!   the container — a unit or timer is disabled and stopped first, and
//!   systemd reloads afterwards (`Automatisch bij deploy`, replacing D84's
//!   report-then-prune);
//! * a native unit dropped from `natives:` is stopped, disabled, its unit
//!   file and program removed (`Stoppen, data bewaren`) — its data dirs
//!   inside the container and its restic repository are kept.
//!
//! Everything else the copy has that the stack does not is removed from
//! the repo by the commit step, so `git log` records the removal.

use std::collections::BTreeSet;

use crate::error::CoreError;
use crate::executor::{Cmd, pct_sh, run_ok};
use crate::manifest;
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    /// Where the intent repo keeps this stack's last-placed files.
    pub(super) fn repo_stack_dir(&self) -> String {
        format!(
            "{}/repo/stacks/{}",
            self.ctx.state_dir, self.spec.manifest.stack_name
        )
    }

    /// fix-199: an older client's `DeploySpec` may have no field at all for
    /// something a newer one added, so an empty/absent `files` would read as
    /// "none declared, remove the rest" rather than "this client cannot even
    /// say" (`client/src/link.rs`'s 2026-08-31 incident). `client_knows`
    /// gates the whole diff on it; a client too old to know this field skips
    /// the step rather than retiring what it cannot declare.
    pub(super) fn files_removal_known(&self) -> bool {
        let known = manifest::client_knows(self.spec.client_schema, "files");
        if !known {
            log_info(
                self.ctx,
                format!(
                    "[retire dropped] skipped — the deploying client (schema {}) predates this \
                     declared-files list; nothing was retired on its behalf",
                    self.spec.client_schema
                ),
            );
        }
        known
    }

    pub(super) async fn retire_dropped(&mut self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, exec) = self.parts();
        self.dropped_repo_files = self.list_dropped_repo_files().await?;
        let mut changed = false;
        let mut reload = false;
        for rel in &self.dropped_repo_files {
            if let Some(needs_reload) = self.retire_rootfs_file(rel).await? {
                reload |= needs_reload;
                changed = true;
            }
        }
        for unit in self.dropped_natives.clone() {
            let vault = self.retire_native(&unit).await?;
            self.unit_vault.insert(unit, vault);
            reload = true;
            changed = true;
        }
        if reload {
            pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    /// Files the intent repo copy has that the stack no longer declares.
    async fn list_dropped_repo_files(&self) -> Result<Vec<String>, CoreError> {
        let (_, spec, _, exec) = self.parts();
        let listing = exec
            .run(&Cmd::new(
                "sh",
                &[
                    "-c",
                    &format!(
                        "cd {} 2>/dev/null && find . -type f -printf '%P\\n' 2>/dev/null || true",
                        crate::ops::util::shq(&self.repo_stack_dir())
                    ),
                ],
                60,
            ))
            .await?;
        let declared: BTreeSet<&str> = spec.files.iter().map(|f| f.path.as_str()).collect();
        Ok(listing
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !declared.contains(*l))
            .map(str::to_string)
            .collect())
    }

    /// Removes one dropped `rootfs/` file. `None`: not a rootfs file (or one
    /// the validator would refuse today), nothing done. `Some(reload)`:
    /// removed, and whether systemd needs a `daemon-reload`.
    async fn retire_rootfs_file(&self, rel: &str) -> Result<Option<bool>, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        if !rel.starts_with(manifest::ROOTFS_PREFIX) {
            return Ok(None);
        }
        // A path the validator would refuse today was never placed.
        let Ok((dest, _)) = manifest::file_destination(&m.stack_name, rel) else {
            return Ok(None);
        };
        let mut reload = false;
        if let Some(name) = dest.strip_prefix("/etc/systemd/system/") {
            reload = true;
            let is_unit = !name.contains('/')
                && [".service", ".timer", ".socket", ".path"]
                    .iter()
                    .any(|x| name.ends_with(x));
            if is_unit {
                self.disable_now(name).await?;
            }
        }
        let vm = m.vmid.to_string();
        run_ok(
            exec,
            &Cmd::new("pct", &["exec", &vm, "--", "rm", "-f", &dest], 60),
        )
        .await?;
        log_info(
            ctx,
            format!("[orphans] removed {} — the stack no longer places it", dest),
        );
        Ok(Some(reload))
    }

    pub(super) async fn disable_now(&self, unit: &str) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        pct_sh(
            exec,
            m.vmid,
            &format!(
                "systemctl disable --now {} 2>&1 || true",
                crate::ops::util::shq(unit)
            ),
            120,
        )
        .await?;
        Ok(())
    }
}
