//! Native backup, before anything is paused: the repository, the service's
//! own newest copy, the guard against archiving an empty rebuild over
//! history, and stale locks.

use super::helpers::quoted_dirs;
use crate::error::CoreError;
use crate::executor::{Cmd, Executor, run_ok};
use crate::native::NativeServiceManifest;
use crate::ops::backup::{
    BackupCfg, InitOutcome, init_repository, parse_snapshots_json, restic_cmd,
    same_password_as_host_meta,
};
use crate::ops::{OpCtx, util_pct_sh};
use crate::runner::StepOutcome;

/// T77: a service's own nightly copy older than this is not tonight's copy.
/// 26 h leaves room for a late run without accepting yesterday's file.
pub const MAX_OWN_COPY_AGE_S: u64 = 26 * 3600;

/// "init repo": gap-24: the answer is read, as for compose stacks and
/// host-meta.
pub(super) async fn init_repo(
    ctx: &OpCtx<'_>,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let init = restic_cmd(cfg, &m.unit, &["init"], 120);
    let outcome = init_repository(ctx.exec, &init, &m.unit).await?;
    if outcome == InitOutcome::Created {
        same_password_as_host_meta(ctx.exec, &cfg.restic_base, &cfg.password_file, &m.unit).await?;
        return Ok(StepOutcome::Changed);
    }
    Ok(StepOutcome::Unchanged)
}

/// "find the service's own newest copy": T77 (D94): a service that makes its
/// own verified copy is archived from that copy, and only when it is fresh.
/// The age is measured against the container's clock, in the same command
/// that finds the file, so a copy from a run that silently stopped cannot
/// pass as tonight's. Sets `own_copy`.
pub(super) async fn find_own_copy(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    own_copy: &mut Option<String>,
) -> Result<StepOutcome, CoreError> {
    let Some(glob) = &m.backup_from_newest else {
        return Ok(StepOutcome::Unchanged);
    };
    let script = format!(
        "f=$(ls -1t {glob} 2>/dev/null | head -1); [ -n \"$f\" ] || exit 3; \
         echo \"$f $(( $(date +%s) - $(stat -c %Y \"$f\") ))\"",
        glob = glob
    );
    let out = util_pct_sh(exec, m.vmid, &script, 60).await?;
    let line = out.stdout.trim().to_string();
    let (file, age) = match line.rsplit_once(' ') {
        Some((f, a)) if out.success() && !f.is_empty() => {
            (f.to_string(), a.parse::<u64>().unwrap_or(u64::MAX))
        }
        _ => {
            return Err(CoreError::Other(format!(
                "no copy matches {} on {} — the service's own backup has not produced \
                 one; refusing to archive nothing and call it a backup",
                glob, m.hostname
            )));
        }
    };
    if age > MAX_OWN_COPY_AGE_S {
        return Err(CoreError::Other(format!(
            "the newest copy {} is {} h old (limit {} h) — the service's own backup did \
             not run; archiving a stale copy would look exactly like success (M-D94)",
            file,
            age / 3600,
            MAX_OWN_COPY_AGE_S / 3600
        )));
    }
    *own_copy = Some(file);
    Ok(StepOutcome::Unchanged)
}

/// fix-guard-stopped-1: a stopped container has no readable data directory,
/// which looked exactly like an empty one ("SAFETY ABORT: /var/lib/inbox
/// holds no files" for CT 118 after the 2026-10-20 host crash). Asked first,
/// so the refusal names the real reason.
async fn ensure_running(exec: &dyn Executor, m: &NativeServiceManifest) -> Result<(), CoreError> {
    let status = exec
        .run(&Cmd::new("pct", &["status", &m.vmid.to_string()], 30))
        .await?;
    if status.stdout.contains("stopped") {
        return Err(CoreError::SafetyAbort(format!(
            "container {} is stopped; its data cannot be read, nothing archived :: \
             start it (pct start {}) and the next backup runs",
            m.vmid, m.vmid
        )));
    }
    Ok(())
}

/// "empty data over history": fix-63: empty directories are archived only
/// into a repository without history — a new service — and refused over one
/// that has some, so an empty rebuild can never become the latest snapshot
/// of a service with real history. Story: docs/deployment/REGISTER.md.
pub(super) async fn empty_over_history(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
    has_own_copy: bool,
) -> Result<StepOutcome, CoreError> {
    if has_own_copy || m.data_dirs.is_empty() {
        return Ok(StepOutcome::Unchanged);
    }
    ensure_running(exec, m).await?;
    let find = format!("find {} -type f 2>/dev/null | head -1", quoted_dirs(m));
    let probe = util_pct_sh(exec, m.vmid, &find, 120).await?;
    if !probe.stdout.trim().is_empty() {
        return Ok(StepOutcome::Unchanged);
    }
    let list = restic_cmd(cfg, &m.unit, &["snapshots", "--json"], 300);
    let listing = run_ok(exec, &list).await?;
    let history = parse_snapshots_json(&listing.stdout);
    let Some(newest) = history.iter().map(|(_, t)| *t).max() else {
        return Ok(StepOutcome::Unchanged);
    };
    Err(CoreError::SafetyAbort(format!(
        "{} holds no files on {} while {}-config has {} snapshot(s), the newest from {} :: \
         archiving it would make an empty state the latest snapshot and start pruning the \
         real history. This is what a rebuilt container looks like before its data is \
         put back: restore it first (docs/OPERATIONS_RUNBOOK.md op-11), and the next \
         backup runs",
        m.data_dirs.join(", "),
        m.hostname,
        m.unit,
        history.len(),
        crate::state::ymd(newest)
    )))
}

/// "clear stale locks": fix-113 (native-tar-no-quiesce, 2026-09-27): a run
/// killed mid-snapshot leaves a stale lock, as on the compose path; restic
/// only removes locks of processes that are gone, so this is always safe.
pub(super) async fn clear_stale_locks(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let _ = exec.run(&restic_cmd(cfg, &m.unit, &["unlock"], 120)).await;
    Ok(StepOutcome::Unchanged)
}
