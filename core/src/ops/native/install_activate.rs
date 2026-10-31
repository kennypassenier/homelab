//! Install, the switch: stop, swap the binary in, start, and — when it does
//! not come up — put the old one back. Then the cleanups that only make
//! sense once the new binary has proven itself.

use super::install::Paths;
use super::install_scripts::{own_program_dir_script, unit_user};
use super::scripts::{keep_one_previous_script, restore_set_aside_script};
use crate::error::CoreError;
use crate::executor::Executor;
use crate::native::NativeServiceManifest;
use crate::ops::util::shq;
use crate::ops::util_pct_sh;
use crate::runner::StepOutcome;

/// "activate": stopping first is deliberate: replacing the file under a
/// running process leaves the old one mapped, so the service keeps running
/// the version that was just replaced and every reading afterwards lies.
pub(super) async fn activate(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    had_previous: bool,
) -> Result<StepOutcome, CoreError> {
    let script = format!(
        "systemctl stop {u} 2>/dev/null; mv -f {new} {bin} && \
         systemctl enable {u} >/dev/null 2>&1 && systemctl start {u} && \
         for i in 1 2 3 4 5; do \
           [ \"$(systemctl is-active {u})\" = active ] && exit 0; sleep 2; done; exit 1",
        u = p.unit,
        new = shq(&p.staged),
        bin = shq(&m.binary)
    );
    let out = util_pct_sh(exec, m.vmid, &script, 180).await?;
    if out.success() {
        return Ok(StepOutcome::Changed);
    }
    if !had_previous {
        return Err(first_install_failed(exec, m, &p.unit).await?);
    }
    Err(roll_back_install(exec, m, p).await?)
}

/// Nothing to roll back to, and saying so plainly matters: a rollback
/// message here would claim a safety net that does not exist. The unit is
/// left stopped rather than restart-looping. The `Ok` is the error to raise.
async fn first_install_failed(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    unit: &str,
) -> Result<CoreError, CoreError> {
    let _ = util_pct_sh(exec, m.vmid, &format!("systemctl stop {}", unit), 60).await;
    let log = util_pct_sh(
        exec,
        m.vmid,
        &format!("journalctl -u {} -n 20 --no-pager 2>&1 | tail -20", unit),
        60,
    )
    .await?;
    Ok(CoreError::Other(format!(
        "{} did not come up and there is no previous binary to return to — this was a \
         FIRST install, so the container now has the program and no working service. \
         Last log lines: {}",
        m.unit,
        log.stdout.trim()
    )))
}

/// The new binary did not come up: copy the previous one back, restart, and
/// report the failure loudly either way. The `Ok` is the error to raise.
async fn roll_back_install(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
) -> Result<CoreError, CoreError> {
    let rollback = format!(
        "cp -p {prev} {bin} && systemctl restart {u} && sleep 2 && \
         [ \"$(systemctl is-active {u})\" = active ]",
        prev = shq(&p.prev),
        bin = shq(&m.binary),
        u = p.unit
    );
    let rb = util_pct_sh(exec, m.vmid, &rollback, 180).await?;
    let _ = util_pct_sh(exec, m.vmid, &restore_set_aside_script(&p.prev), 60).await;
    Ok(CoreError::Other(format!(
        "the installed {} did not come up healthy — rolled back to the previous binary ({})",
        m.unit,
        if rb.success() {
            "service restored and active"
        } else {
            "ROLLBACK ALSO FAILED — service needs hands NOW"
        }
    )))
}

/// "own program directory": a unit with no `User=` runs as root and already
/// owns everything.
pub(super) async fn own_program_dir(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    unit_file: &str,
) -> Result<StepOutcome, CoreError> {
    let Some(user) = unit_user(unit_file) else {
        return Ok(StepOutcome::Unchanged);
    };
    let script = own_program_dir_script(&user, &m.binary);
    let out = util_pct_sh(exec, m.vmid, &script, 120).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "{} is installed and running but its program directory is still root's ({}) — \
             the service cannot update itself from here",
            m.unit,
            out.stderr.trim()
        )));
    }
    Ok(StepOutcome::Changed)
}

/// "keep one previous binary": fix-114, as a supervised update does. Not
/// fatal when it fails: the service is up and correct, this only leaves a
/// copy on disk. Sets `stale_kept` so it is reported after the step rather
/// than failing a good install over it.
pub(super) async fn keep_one_previous(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    had_previous: bool,
    stale_kept: &mut bool,
) -> Result<StepOutcome, CoreError> {
    if !had_previous {
        return Ok(StepOutcome::Unchanged);
    }
    let script = keep_one_previous_script(&m.binary, &p.prev);
    let out = util_pct_sh(exec, m.vmid, &script, 60).await?;
    if !out.success() {
        *stale_kept = true;
        return Ok(StepOutcome::Unchanged);
    }
    Ok(StepOutcome::Changed)
}
