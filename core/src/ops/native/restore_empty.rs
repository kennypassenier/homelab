//! fix-146: bringing a rebuilt native unit's empty data directories back
//! from the newest snapshot before the unit starts.

use super::helpers::quoted_dirs;
use crate::error::CoreError;
use crate::executor::{Cmd, Executor, trace_line};
use crate::native::NativeServiceManifest;
use crate::ops::backup::{BackupCfg, RESTIC_CACHE_DIR, parse_snapshots_json, restic_cmd};
use crate::ops::util_pct_sh;

/// fix-146 (native-empty-rebuild, Kenny 2026-09-27, form "Keuzes helpers"):
/// what the deploy found about a native unit's data before starting it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmptyUnit {
    /// Its data directories hold something: nothing to do.
    HasData,
    /// Empty, and its repository has no snapshot (or does not exist): a new
    /// service.
    Fresh,
    /// Empty; the newest snapshot was unpacked back into the container.
    Restored,
    /// Unpacked, but the unit must not start yet: why.
    RestoredHold(String),
    /// Empty, and whether a snapshot exists could not be read: why.
    CheckFailed(String),
    /// Empty with history, and the restore failed: why.
    RestoreFailed(String),
}

/// Whether any data directory holds something.
async fn holds_data(exec: &dyn Executor, m: &NativeServiceManifest) -> Result<bool, CoreError> {
    let probe = util_pct_sh(
        exec,
        m.vmid,
        &format!("find {} -mindepth 1 2>/dev/null | head -1", quoted_dirs(m)),
        60,
    )
    .await?;
    Ok(!probe.stdout.trim().is_empty())
}

/// The unit's snapshots, or the verdict to stop with. fix-54's rule: only an
/// empty list and restic's "repository does not exist" (exit 10) mean there
/// is nothing to restore.
async fn snapshot_history(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
) -> Result<Vec<(String, u64)>, EmptyUnit> {
    let listing = exec
        .run(&restic_cmd(cfg, &m.unit, &["snapshots", "--json"], 120))
        .await;
    match listing {
        Ok(out) if out.success() => Ok(parse_snapshots_json(&out.stdout)),
        Ok(out) if out.code == 10 => Ok(Vec::new()),
        Ok(out) => Err(EmptyUnit::CheckFailed(format!(
            "rc={} :: {}",
            out.code,
            trace_line(out.stderr.trim())
        ))),
        Err(e) => Err(EmptyUnit::CheckFailed(e.to_string())),
    }
}

/// Unpack the newest snapshot into the container; `Some` carries the
/// failure to stop with.
async fn unpack_latest(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
) -> Option<EmptyUnit> {
    let script = format!(
        "set -o pipefail; env RESTIC_REPOSITORY={base}/{unit}-config RESTIC_PASSWORD_FILE={pw} \
         RESTIC_CACHE_DIR={cache} restic dump latest /{unit}-data.tar | \
         pct exec {vmid} -- tar -xf - -C /",
        base = cfg.restic_base,
        unit = m.unit,
        pw = cfg.password_file,
        cache = RESTIC_CACHE_DIR,
        vmid = m.vmid
    );
    let out = exec
        .run(&Cmd::new("sh", &["-c", &script], cfg.restore_timeout_s))
        .await;
    match out {
        Ok(o) if o.success() => None,
        Ok(o) => Some(EmptyUnit::RestoreFailed(format!(
            "rc={} :: {}",
            o.code,
            trace_line(o.stderr.trim())
        ))),
        Err(e) => Some(EmptyUnit::RestoreFailed(e.to_string())),
    }
}

/// fix-146: `after_restore` is the generic form of what used to be a
/// manual step (op-11) — a command that turns whatever the snapshot
/// unpacked into the live store this unit expects. Run it BEFORE the
/// caller is told the unit may start; a failure here must hold the unit
/// back exactly like a failed unpack, because starting it now would run
/// the service against data the command never finished seeding.
async fn run_after_restore(exec: &dyn Executor, m: &NativeServiceManifest, cmd: &str) -> EmptyUnit {
    match util_pct_sh(exec, m.vmid, cmd, 300).await {
        Ok(o) if o.success() => EmptyUnit::Restored,
        Ok(o) => EmptyUnit::RestoreFailed(format!(
            "after_restore failed, rc={} :: {}",
            o.code,
            trace_line(o.stderr.trim())
        )),
        Err(e) => EmptyUnit::RestoreFailed(format!("after_restore failed: {}", e)),
    }
}

/// fix-146: a rebuilt container starts its native units with empty data
/// directories until somebody restores them by hand (op-11), while a compose
/// stack's empty directories are refilled on deploy (E3). The unit then
/// starts empty and fix-63 has to refuse that night's backup. Now the deploy
/// asks first: empty, with history, means the newest snapshot is unpacked
/// back into the container — the op-11 procedure, `restic dump` piped into
/// `tar` inside the container so owners stay the container's own — BEFORE
/// the unit starts.
///
/// A unit archived from its own copy (`backup_from_newest`, kyu) gets that
/// copy back, not a live store: the copy has to become the live file by hand
/// (the stack file's restore note names it), so that unit is left stopped.
pub async fn restore_empty_unit(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
) -> Result<EmptyUnit, CoreError> {
    if m.stateless || m.data_dirs.is_empty() || holds_data(exec, m).await? {
        return Ok(EmptyUnit::HasData);
    }
    match snapshot_history(exec, cfg, m).await {
        Ok(history) if history.is_empty() => return Ok(EmptyUnit::Fresh),
        Ok(_) => {}
        Err(verdict) => return Ok(verdict),
    }
    if let Some(failed) = unpack_latest(exec, cfg, m).await {
        return Ok(failed);
    }
    if let Some(cmd) = &m.after_restore {
        return Ok(run_after_restore(exec, m, cmd).await);
    }
    if let Some(glob) = &m.backup_from_newest {
        return Ok(EmptyUnit::RestoredHold(format!(
            "its archive is the service's own copy ({}), not a live store, and this service \
             declares no after_restore to seed it: put the newest copy in place as the live \
             file as the stack file's restore note says (docs/OPERATIONS_RUNBOOK.md op-11), \
             then start it",
            glob
        )));
    }
    Ok(EmptyUnit::Restored)
}
