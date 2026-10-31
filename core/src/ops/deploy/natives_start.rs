//! Starting a native unit, or restarting a running one whose files moved.

use crate::error::CoreError;
use crate::executor::{Cmd, pct_sh, run_ok};
use crate::manifest::FileBlob;
use crate::native::{NativeServiceManifest, UnitPrereqs};
use crate::ops::OpCtx;
use crate::ops::native::EmptyUnit;

use super::natives::NativeAcc;
use super::restore_checks::record_restore_checks;
use super::run::{Run, log_info, log_warn};

impl Run<'_, '_> {
    /// Decides between starting, leaving alone and restarting `unit`.
    pub(super) async fn start_or_refresh(
        &self,
        unit: &str,
        blob: &FileBlob,
        need: &UnitPrereqs,
        written: &[String],
        acc: &mut NativeAcc,
    ) -> Result<(), CoreError> {
        let (_, spec, m, exec) = self.parts();
        let active = pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
        let running = active.stdout.trim() == "active";
        // fix-146 (native-empty-rebuild, 2026-09-27): a unit that is not
        // running and whose data is empty gets its newest snapshot back
        // before it starts. A running unit is never written under.
        if !running
            && let Some(nm) = spec.native_manifests.get(unit)
            && !self.restore_empty(unit, nm, acc).await?
        {
            return Ok(());
        }
        if running {
            self.refresh_running(unit, blob, need, written, acc).await
        } else {
            self.start_unit(unit, acc).await
        }
    }

    /// `false` when the unit must NOT be started this run.
    async fn restore_empty(
        &self,
        unit: &str,
        nm: &NativeServiceManifest,
        acc: &mut NativeAcc,
    ) -> Result<bool, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let found = crate::ops::native::restore_empty_unit(exec, &ctx.backup, nm).await?;
        // fix-147: a check that could not be made is remembered
        // until a later one of the same unit succeeds.
        match &found {
            EmptyUnit::HasData => {}
            EmptyUnit::CheckFailed(why) => {
                record_restore_checks(ctx, &m.stack_name, &[(unit.to_string(), why.clone())], &[])
                    .await
            }
            _ => record_restore_checks(ctx, &m.stack_name, &[], &[unit.to_string()]).await,
        }
        self.say_empty_outcome(unit, &found);
        Ok(match found {
            EmptyUnit::Restored => {
                acc.changed = true;
                true
            }
            EmptyUnit::RestoredHold(_) => {
                acc.changed = true;
                false
            }
            EmptyUnit::RestoreFailed(_) => false,
            EmptyUnit::HasData | EmptyUnit::Fresh | EmptyUnit::CheckFailed(_) => true,
        })
    }

    fn say_empty_outcome(&self, unit: &str, found: &EmptyUnit) {
        let ctx = self.ctx;
        match found {
            EmptyUnit::HasData => {}
            EmptyUnit::Fresh => log_info(
                ctx,
                format!(
                    "[native] {}: data is empty and has no snapshot — fresh",
                    unit
                ),
            ),
            EmptyUnit::Restored => log_info(
                ctx,
                format!(
                    "[native] {}: data was empty — the newest snapshot is back",
                    unit
                ),
            ),
            held => Self::warn_empty(ctx, unit, held),
        }
    }

    /// The outcomes that keep a unit from starting normally, said loudly.
    fn warn_empty(ctx: &OpCtx<'_>, unit: &str, found: &EmptyUnit) {
        let msg = match found {
            EmptyUnit::RestoredHold(why) => format!(
                "[native] {} NOT started: its data was empty and the newest \
                 snapshot was unpacked, but {}",
                unit, why
            ),
            EmptyUnit::CheckFailed(why) => format!(
                "[native] {}: data is empty and its backup could not be checked \
                 ({}) — starting it EMPTY; if it held data, restore it by hand \
                 (op-11) before the next nightly backup",
                unit, why
            ),
            EmptyUnit::RestoreFailed(why) => format!(
                "[native] AUTO-RESTORE FAILED for {} ({}) — NOT started, so it \
                 does not begin again empty; restore it by hand (op-11), then \
                 deploy again",
                unit, why
            ),
            EmptyUnit::HasData | EmptyUnit::Fresh | EmptyUnit::Restored => return,
        };
        log_warn(ctx, msg);
    }

    /// A running unit is left running unless a file it reads moved.
    async fn refresh_running(
        &self,
        unit: &str,
        blob: &FileBlob,
        need: &UnitPrereqs,
        written: &[String],
        acc: &mut NativeAcc,
    ) -> Result<(), CoreError> {
        if let Some(reason) = self.restart_reason(unit, blob, written, acc) {
            self.restart_healthy(unit, &reason).await?;
            acc.changed = true;
        }
        // 6 · keep the vault current, so the NEXT rebuild can restore
        // what this container has.
        self.refresh_vault(need).await;
        Ok(())
    }

    /// fix-159: a running unit whose unit file, drop-in or env
    /// file this deploy changed restarts, through the health
    /// check updates and rollbacks use (restart, wait for
    /// active, NRestarts across a window), said before it
    /// happens. Unchanged: left running, as adoption leaves it.
    /// Story: docs/deployment/REGISTER.md.
    /// latch-files: a secret file that names this unit in
    /// `restarts` (a config it reads by path, not by systemd).
    fn restart_reason(
        &self,
        unit: &str,
        blob: &FileBlob,
        written: &[String],
        acc: &NativeAcc,
    ) -> Option<String> {
        let declared = self
            .spec
            .secret_files
            .iter()
            .any(|f| f.restarts.as_deref() == Some(unit) && acc.secret_paths.contains(&f.path));
        match crate::native::restart_reason(unit, &blob.content, written) {
            Some(r) if declared => Some(format!("{} and secret file changed", r)),
            None if declared => Some("secret file changed".to_string()),
            other => other,
        }
    }

    async fn restart_healthy(&self, unit: &str, reason: &str) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        log_info(ctx, format!("[native] restarts {}: {}", unit, reason));
        let svc = format!("{}.service", unit);
        let out = pct_sh(exec, m.vmid, &crate::ops::native::health_script(&svc), 180).await?;
        if !out.success() {
            let tail = pct_sh(
                exec,
                m.vmid,
                &format!("journalctl -u {} -n 20 --no-pager 2>&1 | tail -20", svc),
                60,
            )
            .await
            .map(|o| o.stdout.trim().to_string())
            .unwrap_or_default();
            return Err(CoreError::Other(format!(
                "{} did not come up healthy after its restart ({}; the check read \
                 {}) :: the new files are in place and the service does not run on \
                 them; correct the stack file and deploy again, or put the previous \
                 version back and deploy that. Last log lines: {}",
                unit,
                reason,
                match out.stdout.trim() {
                    "" => "nothing",
                    r => r,
                },
                tail
            )));
        }
        log_info(
            ctx,
            format!("[native] {} restarted and healthy ({})", unit, reason),
        );
        Ok(())
    }

    async fn start_unit(&self, unit: &str, acc: &mut NativeAcc) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        log_info(
            ctx,
            format!("[native] {} is not running — enabling and starting", unit),
        );
        run_ok(
            exec,
            &Cmd::new(
                "pct",
                &[
                    "exec",
                    &vm,
                    "--",
                    "sh",
                    "-c",
                    &format!("systemctl enable --now {}", unit),
                ],
                120,
            ),
        )
        .await?;
        let after = pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
        if after.stdout.trim() != "active" {
            return Err(CoreError::Command {
                rendered: format!("systemctl enable --now {}", unit),
                detail: format!("{} did not come up: {}", unit, after.stdout.trim()),
            });
        }
        acc.changed = true;
        Ok(())
    }
}
