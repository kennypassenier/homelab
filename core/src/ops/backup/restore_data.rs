//! The destructive middle of a restore: stop, write the snapshot back, start.

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok, shq};
use crate::manifest::StackManifest;
use crate::ops::util::{compose_in_app, compose_running_services};
use crate::runner::{Runner, StepOutcome};

use super::owners::owner_groups;
use super::progress::run_restic_bytes;
use super::restic::restic;
use super::restore_app::RestoreJob;
use super::shell::sh;

/// gap-28: a native unit's backup is one tar stream, `/<unit>-data.tar`
/// (native.rs), not a copy of its directories. `restic restore --target /`
/// of that repository drops the tar file at `/` on the Proxmox host, unpacks
/// nothing, and used to report success. Refused before anything is touched,
/// naming the procedure that does work.
pub(super) fn refuse_native_units(m: &StackManifest) -> Result<StepOutcome, CoreError> {
    let natives: Vec<String> = owner_groups(m)
        .into_iter()
        .map(|(owner, _)| owner)
        .filter(|o| m.natives.contains(o))
        .collect();
    if !natives.is_empty() {
        return Err(CoreError::SafetyAbort(format!(
            "{} is a native unit: its backup is one tar stream, and a restic restore would \
             drop that tar file on the host and unpack nothing :: restore it by hand with \
             `restic dump` into the container, as docs/OPERATIONS_RUNBOOK.md op-11 describes",
            natives.join(", ")
        )));
    }
    Ok(StepOutcome::Unchanged)
}

impl RestoreJob<'_, '_> {
    /// Stop the whole stack for a consistent restore.
    pub(super) async fn quiesce_stack(&self) -> Result<StepOutcome, CoreError> {
        for a in &self.apps {
            let _ = self.compose(a, "docker compose down", 120).await?;
        }
        Ok(StepOutcome::Changed)
    }

    pub(super) async fn resume_stack(&self) -> Result<StepOutcome, CoreError> {
        for a in &self.apps {
            self.compose(a, "docker compose up -d", 300).await?;
        }
        Ok(StepOutcome::Changed)
    }

    async fn compose(
        &self,
        app: &str,
        cmd: &str,
        timeout_s: u64,
    ) -> Result<crate::executor::CmdOutput, CoreError> {
        let (vmid, stack) = (self.m.vmid, &self.m.stack_name);
        compose_in_app(self.exec, vmid, stack, app, cmd, timeout_s).await
    }

    /// G4 of the Phase-7 gate: the restore may fail, but RESUME MUST ALWAYS
    /// RUN. A failed safety copy means "restore data" never runs; it is
    /// marked skipped rather than left as a silent gap.
    pub(super) async fn restore_data_step(
        &self,
        runner: &mut Runner<'_>,
        copy_result: &Result<bool, CoreError>,
    ) -> Result<StepOutcome, CoreError> {
        let Ok(copied) = copy_result else {
            runner.skip("restore data");
            return Ok(StepOutcome::Unchanged);
        };
        let copied = *copied;
        runner
            .step("restore data", || async {
                for (owner, paths) in &self.groups {
                    self.restore_repository(owner, paths, copied).await?;
                }
                Ok(StepOutcome::Changed)
            })
            .await
    }

    async fn restore_repository(
        &self,
        owner: &str,
        paths: &[String],
        copied: bool,
    ) -> Result<(), CoreError> {
        // fix-112: the data is in the safety copy, so the target can be
        // emptied and restic writes into a clean directory. Without the copy
        // nothing is deleted: that would be the only copy there is.
        if copied {
            for p in paths {
                let script = format!("find {} -mindepth 1 -delete", shq(p));
                run_ok(self.exec, &sh(&script, self.cfg.restore_timeout_s)).await?;
            }
        }
        let id = self
            .chosen
            .iter()
            .find(|(o, _)| o == owner)
            .map(|(_, id)| id.as_str())
            .unwrap_or(self.snapshot);
        let cfg = self.cfg;
        let cmd: Cmd = restic(
            &cfg.restic_base,
            owner,
            &cfg.password_file,
            &["restore", id, "--json", "--target", "/"],
            cfg.restore_timeout_s,
        );
        run_restic_bytes(self.exec, self.ctx.sink, &self.op, owner, cmd).await?;
        Ok(())
    }

    pub(super) async fn verify_health(&self) -> Result<StepOutcome, CoreError> {
        for a in &self.apps {
            // fix-132/fix-133: read via `--format json` and the Unknown
            // state, so a probe that could not answer at all is reported as
            // what it is rather than folded into "nothing is running".
            let (vmid, stack) = (self.m.vmid, &self.m.stack_name);
            match compose_running_services(self.exec, vmid, stack, a, "").await? {
                Some(running) if running.is_empty() => {
                    return Err(CoreError::Other(format!("{} not running after restore", a)));
                }
                Some(_) => {}
                None => {
                    return Err(CoreError::Other(format!(
                        "{} :: could not read whether it is running after restore",
                        a
                    )));
                }
            }
        }
        Ok(StepOutcome::Unchanged)
    }
}
