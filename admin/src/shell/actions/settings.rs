//! The queue's settings and small collaborators: the clock, the releases
//! source, the limits, the pause gate and the host line for reads.

use std::sync::Arc;
use std::time::Duration;

use homelab_proto::{Command, RpcResponse};

use super::{Actions, HISTORY_WINDOW_S, KEEP_JOBS, PauseGate, SettledFn};
use crate::core::actions::Refusal;

impl Actions {
    pub(super) fn now(&self) -> i64 {
        (self.inner.clock)()
    }

    /// TUI parity: where releases are read (GitHub in the app, a fake in a
    /// test). Set once.
    pub fn set_releases(&self, r: Arc<dyn crate::shell::releases::Releases>) {
        let _ = self.inner.releases.set(r);
    }

    /// fix-172: the drive session registers itself here once it exists
    /// (mount builds the driver from an `Actions` it already has, so this
    /// comes after `start`). Set once.
    pub fn on_stack_settled(&self, f: SettledFn) {
        let _ = self.inner.settled.set(f);
    }

    pub fn set_pause_gate(&self, gate: Arc<dyn PauseGate>) {
        let _ = self.inner.pause_gate.set(gate);
    }

    /// fix-172: hold the queue here, between two jobs, while a viewer has
    /// Live view paused — the boundary Pause actually promises. The job
    /// that was already running when Pause was pressed keeps running to
    /// its end (a host operation already under way cannot be held
    /// mid-step); this only ever waits before the NEXT job is taken off
    /// the queue, batch or not, since the queue runs one job at a time
    /// anyway (AR12).
    pub(super) async fn wait_while_paused(&self) {
        let Some(gate) = self.inner.pause_gate.get() else {
            return;
        };
        loop {
            let notify = gate.notified();
            let woken = notify.notified();
            tokio::pin!(woken);
            // Register as a waiter before reading `paused_by`, the same
            // order `drive::held` uses, so a Continue that lands between
            // the read and the wait is never missed.
            woken.as_mut().enable();
            if gate.paused_by().is_none() {
                return;
            }
            woken.await;
        }
    }

    /// How long "Update host" waits for the host to come back (a test
    /// shortens it). Set once; five minutes otherwise.
    pub fn set_reconnect_wait(&self, d: Duration) {
        let _ = self.inner.reconnect_wait.set(d);
    }

    /// Decision "23 constants": jobs kept and the history window, from
    /// `ActConfig` (mount only). Set once; [`KEEP_JOBS`] and
    /// [`HISTORY_WINDOW_S`] otherwise.
    pub fn set_limits(&self, keep_jobs: usize, history_window_s: i64) {
        let _ = self.inner.limits.set((keep_jobs, history_window_s));
    }

    pub(super) fn keep_jobs(&self) -> usize {
        self.inner.limits.get().map(|l| l.0).unwrap_or(KEEP_JOBS)
    }

    pub(super) fn history_window_s(&self) -> i64 {
        self.inner
            .limits
            .get()
            .map(|l| l.1)
            .unwrap_or(HISTORY_WINDOW_S)
    }

    /// A read on the host line, for the lists a form's choices come from
    /// (the manual checks, the templates).
    pub async fn ask(&self, command: Command, timeout: Duration) -> Result<RpcResponse, String> {
        self.inner.host.ask_traced(command, timeout, None).await
    }

    pub(super) fn releases(&self) -> Result<Arc<dyn crate::shell::releases::Releases>, Refusal> {
        self.inner.releases.get().cloned().ok_or_else(|| {
            Refusal::new(
                "the releases",
                "this dashboard reads no releases",
                "report this with the dashboard's log",
            )
        })
    }
}
