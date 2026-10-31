//! The steps of the operator-driven native restore, one function each.

use super::helpers::quoted_dirs;
use super::steps::guard_step;
use crate::error::CoreError;
use crate::executor::{Cmd, Executor};
use crate::native::NativeServiceManifest;
use crate::ops::backup::{BackupCfg, RESTIC_CACHE_DIR, restore_confirmed};
use crate::ops::util::shq;
use crate::ops::{OpCtx, util_pct_sh};
use crate::runner::StepOutcome;

/// "safety gates": the operator typed the stack's name, and the target is
/// the container the manifest names.
pub(super) async fn safety_gates(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    confirm: Option<&str>,
) -> Result<StepOutcome, CoreError> {
    restore_confirmed(&m.stack_name, confirm)?;
    guard_step(ctx, exec, m).await
}

/// `systemctl <verb> <unit>` inside the container; a failure names the verb.
async fn systemctl(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    verb: &str,
) -> Result<StepOutcome, CoreError> {
    let out = util_pct_sh(
        exec,
        m.vmid,
        &format!("systemctl {} {}", verb, shq(&m.unit)),
        60,
    )
    .await?;
    if !out.success() {
        return Err(CoreError::Command {
            rendered: format!("systemctl {} {}", verb, m.unit),
            detail: out.stderr.trim().to_string(),
        });
    }
    Ok(StepOutcome::Changed)
}

/// "stop unit": a live restore under a running service would interleave the
/// unpack with its own writes.
pub(super) async fn stop_unit(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    systemctl(exec, m, "stop").await
}

/// "start unit".
pub(super) async fn start_unit(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    systemctl(exec, m, "start").await
}

/// "safety copy": the current data directories, copied to `dest` on the
/// HOST.
pub(super) async fn safety_copy(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    dest: &str,
) -> Result<StepOutcome, CoreError> {
    let script = format!(
        "mkdir -p {dest} && pct exec {vmid} -- tar -cf - {dirs} | tar -xf - -C {dest}",
        dest = shq(dest),
        vmid = m.vmid,
        dirs = quoted_dirs(m),
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 1800)).await?;
    if !out.success() {
        return Err(CoreError::Command {
            rendered: "safety copy of the current data".into(),
            detail: out.stderr.trim().to_string(),
        });
    }
    Ok(StepOutcome::Changed)
}

/// "unpack snapshot": `restic dump` piped into `tar` inside the container.
pub(super) async fn unpack_snapshot(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    cfg: &BackupCfg,
    snapshot: &str,
) -> Result<StepOutcome, CoreError> {
    let script = format!(
        "set -o pipefail; env RESTIC_REPOSITORY={base}/{unit}-config \
         RESTIC_PASSWORD_FILE={pw} RESTIC_CACHE_DIR={cache} restic dump {snap} \
         /{unit}-data.tar | pct exec {vmid} -- tar -xf - -C /",
        base = cfg.restic_base,
        unit = m.unit,
        pw = cfg.password_file,
        cache = RESTIC_CACHE_DIR,
        snap = snapshot,
        vmid = m.vmid,
    );
    let out = exec
        .run(&Cmd::new("sh", &["-c", &script], cfg.restore_timeout_s))
        .await?;
    if !out.success() {
        return Err(CoreError::Command {
            rendered: format!("restic dump {} | tar -x", snapshot),
            detail: out.stderr.trim().to_string(),
        });
    }
    Ok(StepOutcome::Changed)
}

/// "re-seed from restore": fix-146: the same re-seed step the automatic
/// empty-rebuild path runs (`restore_empty_unit`) — a unit whose archive is
/// not already its live store (`backup_from_newest`, kyu) needs this before
/// it is safe to start, and a hand-triggered restore from the dashboard is
/// exactly as much "a restore" as the automatic one.
pub(super) async fn reseed_from_restore(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    cmd: &str,
) -> Result<StepOutcome, CoreError> {
    let out = util_pct_sh(exec, m.vmid, cmd, 300).await?;
    if !out.success() {
        return Err(CoreError::Command {
            rendered: "after_restore".into(),
            detail: out.stderr.trim().to_string(),
        });
    }
    Ok(StepOutcome::Changed)
}
