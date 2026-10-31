//! E3/O6: every empty config dir is refilled from its own latest snapshot
//! BEFORE apps start.
//!
//! Per PATH, not per stack: the first version restored only when EVERY
//! declared path was empty, so wiping one app's config while its siblings
//! were intact restored nothing and said nothing. Backup-target trouble
//! degrades to a loud warning, never a blocked deploy (spec:
//! upgraded-by-Kenny Must).

use crate::error::CoreError;
use crate::executor::{Cmd, shq};
use crate::manifest::MountSpec;
use crate::ops::OpCtx;
use crate::runner::StepOutcome;

use super::restore_checks::record_restore_checks;
use super::run::{Run, log_info, log_warn};

/// What the walk over the declared mounts found.
#[derive(Default)]
struct Tally {
    restored_any: bool,
    failed_any: bool,
    /// fix-147: which checks failed and which succeeded, recorded afterwards.
    check_failed: Vec<(String, String)>,
    check_ok: Vec<String>,
}

impl Run<'_, '_> {
    pub(super) async fn auto_restore_check(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, _) = self.parts();
        if m.storage.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut tally = Tally::default();
        for mount in &m.storage {
            self.check_mount(mount, &mut tally).await?;
        }
        if tally.restored_any && !tally.failed_any {
            log_info(ctx, "[e3] auto-restore complete".into());
        }
        record_restore_checks(ctx, &m.stack_name, &tally.check_failed, &tally.check_ok).await;
        Ok(if tally.restored_any {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    async fn check_mount(&self, mount: &MountSpec, tally: &mut Tally) -> Result<(), CoreError> {
        let ctx = self.ctx;
        if !self.is_restorable(mount) || !self.is_empty_dir(mount).await? {
            return Ok(());
        }
        let listed = match self.snapshot_listing(mount).await {
            Ok(listed) => listed,
            Err(why) => {
                Self::note_unchecked(ctx, mount, &why, tally);
                return Ok(());
            }
        };
        tally.check_ok.push(mount.host_path.clone());
        if listed.is_empty() || listed == "[]" {
            log_info(
                ctx,
                format!(
                    "[e3] {} is empty and has no snapshot — fresh",
                    mount.host_path
                ),
            );
            return Ok(());
        }
        log_info(
            ctx,
            format!(
                "[e3] {} is empty and a snapshot exists — restoring",
                mount.host_path
            ),
        );
        self.restore_latest(mount, tally).await;
        Ok(())
    }

    /// Whether the empty-directory question applies to `mount` at all.
    fn is_restorable(&self, mount: &MountSpec) -> bool {
        let m = &self.spec.manifest;
        // An app that declares it keeps nothing is empty BY DESIGN, so
        // "empty, therefore restore it" is exactly the wrong conclusion
        // (F154, story: docs/deployment/REGISTER.md).
        //
        // fix-146: a native unit's data is archived as one tar stream,
        // not by path, so this per-path question always answered "fresh"
        // for it. The `native units` step restores it instead.
        !mount.no_data && !m.natives.iter().any(|n| n == mount.owner(&m.stack_name))
    }

    fn note_unchecked(ctx: &OpCtx<'_>, mount: &MountSpec, why: &str, tally: &mut Tally) {
        tally.failed_any = true;
        tally
            .check_failed
            .push((mount.host_path.clone(), why.to_string()));
        log_warn(
            ctx,
            format!(
                "[e3] {} is empty and its backup could not be checked ({}) — deploy continues with that dir EMPTY; if it held data, restore it by hand before the next nightly backup",
                mount.host_path, why
            ),
        );
    }

    async fn is_empty_dir(&self, mount: &MountSpec) -> Result<bool, CoreError> {
        let probe = self
            .exec
            .run(&Cmd::new(
                "sh",
                &[
                    "-c",
                    &format!("ls -A {} 2>/dev/null | head -1", shq(&mount.host_path)),
                ],
                30,
            ))
            .await?;
        Ok(probe.stdout.trim().is_empty())
    }

    /// fix-54: only two answers mean "nothing to restore": an empty
    /// snapshot list, and restic's own "repository does not exist"
    /// (exit 10 since restic 0.17; pve runs 0.18). Every other
    /// failure (Drive unreachable, an expired rclone token, a wrong
    /// password, the 120 s timeout) is a loud warning; the deploy
    /// still goes on, because backup-target trouble never blocks a
    /// deploy (E3 spec). Story: docs/deployment/REGISTER.md.
    async fn snapshot_listing(&self, mount: &MountSpec) -> Result<String, String> {
        let (ctx, _, m, exec) = self.parts();
        let has_snapshot = exec
            .run(&crate::ops::backup::restic_cmd(
                &ctx.backup,
                mount.owner(&m.stack_name),
                &["snapshots", "--last", "--json", "--path", &mount.host_path],
                120,
            ))
            .await;
        match &has_snapshot {
            Ok(out) if out.success() => Ok(out.stdout.trim().to_string()),
            Ok(out) if out.code == 10 => Ok(String::new()),
            Ok(out) => Err(format!(
                "rc={} :: {}",
                out.code,
                crate::executor::trace_line(out.stderr.trim())
            )),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn restore_latest(&self, mount: &MountSpec, tally: &mut Tally) {
        let (ctx, _, m, exec) = self.parts();
        let restored = exec
            .run(&crate::ops::backup::restic_cmd(
                &ctx.backup,
                mount.owner(&m.stack_name),
                &[
                    "restore",
                    "latest",
                    "--target",
                    "/",
                    "--path",
                    &mount.host_path,
                ],
                ctx.backup.restore_timeout_s,
            ))
            .await;
        match restored {
            Ok(o) if o.success() => tally.restored_any = true,
            _ => {
                tally.failed_any = true;
                log_warn(
                    ctx,
                    format!(
                        "[e3] AUTO-RESTORE FAILED for {} — deploy continues with that dir EMPTY; restore it by hand if it held data",
                        mount.host_path
                    ),
                );
            }
        }
    }
}
