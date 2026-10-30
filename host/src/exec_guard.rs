//! The remote-exec guard: which commands may run, the running-exec registry and its audit line.

use crate::*;

/// The audit.log line for one remote exec, which the journal repeats.
///
/// fix-124 (expert panel, exec-logged-verbatim, 2026-09-27): the command
/// passes the shared secret masker first. It was recorded verbatim, so a
/// password typed on an exec command line stayed on pve for good. The
/// masker knows shapes (`NAME=value` for secret-looking names, URL
/// passwords, `Bearer <token>`), not every secret: a bare password argument
/// still lands, which the user guide says.
/// An `ExecIn` in flight: where it runs, the tag its `timeout` carries, and
/// whether a `CancelExec` stopped it. Keyed by the token that asked and the
/// request id, so two sessions' ids never meet.
pub(crate) struct RunningExec {
    pub(crate) vmid: u16,
    pub(crate) tag: String,
    pub(crate) cancelled: Arc<std::sync::atomic::AtomicBool>,
}

pub(crate) static RUNNING_EXECS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<(String, u64), RunningExec>>,
> = std::sync::LazyLock::new(Default::default);

/// Takes its exec out of [`RUNNING_EXECS`] when the run ends, however it ends.
pub(crate) struct RunningExecGuard(pub(crate) (String, u64));

impl Drop for RunningExecGuard {
    fn drop(&mut self) {
        RUNNING_EXECS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.0);
    }
}

pub(crate) fn exec_audit_line(ts: u64, vmid: u16, command: &str) -> String {
    format!(
        "{} exec vmid={} cmd={:?}\n",
        ts,
        vmid,
        homelab_core::executor::mask_secrets(command)
    )
}

pub(crate) fn exec_allowed(
    config: &Config,
    vmid: u16,
) -> Result<(), homelab_core::error::CoreError> {
    homelab_core::safety::exec_guard(config.exec_enabled, &config.safety, vmid)
}
