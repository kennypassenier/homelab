//! Restore entry points: the scratch-directory restore, the typed-name
//! confirmation and the thin wrappers around `restore_app`.

use crate::error::CoreError;
use crate::executor::Executor;
use crate::manifest::StackManifest;
use crate::runner::OperationReport;

use super::config::BackupCfg;
use super::restic::restic_cmd;
use super::restore_app::restore_app;
use super::snapshot_file::valid_snapshot_ref;
use crate::ops::OpCtx;

pub async fn restore_into(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    app: &str,
    target: &str,
) -> Result<(), CoreError> {
    restore_snapshot_into(exec, cfg, app, "latest", target).await
}

/// fix-237: [`restore_into`] for one named snapshot (an id, or `latest`).
/// The snapshot reference is validated before a command is built, so it
/// cannot ride in as a restic flag.
pub async fn restore_snapshot_into(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    app: &str,
    snapshot: &str,
    target: &str,
) -> Result<(), CoreError> {
    if !valid_snapshot_ref(snapshot) {
        return Err(CoreError::Other(format!(
            "'{}' is not a snapshot :: give `latest` or an id `homelab snapshots` lists",
            snapshot
        )));
    }
    let out = exec
        .run(&restic_cmd(
            cfg,
            app,
            &["restore", snapshot, "--target", target],
            cfg.restore_timeout_s,
        ))
        .await?;
    if out.code != 0 {
        return Err(CoreError::Command {
            rendered: format!("restic restore {} --target {}", snapshot, target),
            detail: format!(
                "restic restore of {} exited {}: {}",
                app,
                out.code,
                out.stderr.trim()
            ),
        });
    }
    Ok(())
}

/// fix-64 (restore-no-confirm-no-safety-snapshot, 2026-09-27): has the
/// operator typed the stack's name for this restore? The TUI always asked;
/// the command line went straight to the host, and the host took either. A
/// restore overwrites live data, so the host refuses one that does not carry
/// the name, whoever sent it.
pub fn restore_confirmed(stack: &str, confirm: Option<&str>) -> Result<(), CoreError> {
    match confirm {
        Some(typed) if typed == stack => Ok(()),
        Some(typed) => Err(CoreError::SafetyAbort(format!(
            "restore of '{}' refused: the typed name '{}' does not match — nothing was stopped",
            stack, typed
        ))),
        None => Err(CoreError::SafetyAbort(format!(
            "restore of '{}' refused: the request carries no typed stack name — a client too old to send \
             one cannot restore; update it (`homelab restore` asks for the name, \
             `--yes` answers it for scripts)",
            stack
        ))),
    }
}

/// E2 with the safety copy (fix-64) on. See `restore_with`.
pub async fn restore(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
) -> OperationReport {
    restore_with(ctx, m, cfg, snapshot, true).await
}

/// E2, and `safety_copy` says whether the current data is copied aside
/// first (fix-64). Off only on the operator's explicit `--no-safety-copy`.
pub async fn restore_with(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
    safety_copy: bool,
) -> OperationReport {
    restore_app(ctx, m, cfg, snapshot, safety_copy, None).await
}
