//! Who holds the operation lock and what it is doing: the lock, the busy mark and the progress line.

use crate::*;

/// fix-104 (see REGISTER.md): take the operation lock, and when it is taken,
/// tell whoever is watching what this command waits for before waiting.
pub(crate) async fn lock_ops(state: &AppState) -> tokio::sync::MutexGuard<'_, ()> {
    if let Ok(g) = state.op_lock.try_lock() {
        return g;
    }
    let holder = state.busy.lock().ok().and_then(|b| b.clone());
    let msg = homelab_core::oplock::waiting_message(holder.as_ref(), unix_now());
    info!("{}", msg);
    let _ = state.log_tx.send(ServerMsg::Log {
        req: None,
        step: None,
        ts: None,
        by: None,
        plan: None,
        level: homelab_proto::LogLevel::Warn,
        source: "HOST".into(),
        msg,
    });
    state.op_lock.lock().await
}

/// fix-104: one progress line of a long read (check, doctor, today) to
/// whoever is watching. Nobody connected, nothing sent.
pub(crate) fn progress_line(state: &AppState, line: &str) {
    let _ = state.log_tx.send(ServerMsg::Log {
        req: None,
        step: None,
        ts: None,
        by: None,
        plan: None,
        level: homelab_proto::LogLevel::Info,
        source: "CHECK".into(),
        msg: line.to_string(),
    });
}

/// fix-104: records what holds the operation lock for as long as it lives.
pub(crate) struct BusyMark<'a> {
    pub(crate) state: &'a AppState,
}

impl<'a> BusyMark<'a> {
    pub(crate) fn set(state: &'a AppState, what: &str, total: usize, stack: Option<&str>) -> Self {
        if let Ok(mut b) = state.busy.lock() {
            *b = Some(homelab_core::oplock::Holder {
                what: what.to_string(),
                started_unix: unix_now(),
                done: 0,
                total,
                stack: stack.map(str::to_string),
            });
        }
        BusyMark { state }
    }

    /// One more item of a batch is finished.
    pub(crate) fn step(state: &AppState) {
        if let Ok(mut b) = state.busy.lock()
            && let Some(h) = b.as_mut()
        {
            h.done += 1;
        }
    }
}

impl Drop for BusyMark<'_> {
    fn drop(&mut self) {
        if let Ok(mut b) = self.state.busy.lock() {
            *b = None;
        }
    }
}
