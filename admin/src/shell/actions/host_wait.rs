//! dash-host-update, fix-121: after an update or restart of the host, wait
//! until the shipped version answers.

use std::time::Duration;

use homelab_client::release::{AfterUpdate, after_update};
use homelab_proto::RpcResponse;

use super::{Actions, JobView};

impl Actions {
    /// dash-host-update, fix-121: done means the shipped version answered
    /// after the restart, not that a restart was scheduled. The line drops
    /// and comes back on its own (arch-host-link); this watches the Hello.
    /// `is_restart`: the host-settings "Save and restart the host" button
    /// (owner decision 2026-09-30, item 2) — the same wait, but `expected`
    /// is always None (no new binary, so no version to match or roll back
    /// to) and the words say "restart" rather than "update".
    pub(super) async fn await_updated_host(
        &self,
        view: &JobView,
        expected: Option<String>,
        sent: RpcResponse,
        is_restart: bool,
    ) -> RpcResponse {
        let wait = self.reconnect_wait();
        let words = Words {
            is_restart,
            expected: expected.as_deref(),
            wait,
        };
        self.say(view.job, words.waiting());
        let end = tokio::time::Instant::now() + wait;
        let mut seen_down = false;
        loop {
            let (up, version) = {
                let s = self.inner.shared.read().await;
                (s.link_error.is_none(), s.host_version.clone())
            };
            seen_down |= !up;
            let answered = if up { version.as_deref() } else { None };
            match after_update(expected.as_deref(), seen_down, answered) {
                AfterUpdate::Answered => {
                    return self.answered(view.job, sent, &words, &version.unwrap_or_default());
                }
                AfterUpdate::RolledBack(v) => return failed(sent, words.rolled_back(&v)),
                AfterUpdate::Wait => {}
            }
            if tokio::time::Instant::now() >= end {
                return failed(sent, words.timed_out());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// The shipped version answered: say so and hand the host's reply on.
    fn answered(&self, job: u64, sent: RpcResponse, words: &Words, version: &str) -> RpcResponse {
        let done = words.answered(version);
        self.say(job, done.clone());
        RpcResponse {
            message: format!("{}\n{done}", sent.message),
            ..sent
        }
    }

    fn reconnect_wait(&self) -> Duration {
        self.inner
            .reconnect_wait
            .get()
            .copied()
            .unwrap_or(Duration::from_secs(300))
    }

    /// One `info` line of the job's log, from the dashboard itself.
    fn say(&self, job: u64, msg: String) {
        self.log_event(serde_json::json!({
            "job": job, "req": null, "level": "info", "source": "ADMIN",
            "msg": msg, "ts": self.now(),
        }));
    }
}

fn failed(sent: RpcResponse, message: String) -> RpcResponse {
    RpcResponse {
        ok: false,
        message,
        ..sent
    }
}

/// What the wait says, for an update or a restart.
struct Words<'a> {
    is_restart: bool,
    expected: Option<&'a str>,
    wait: Duration,
}

impl Words<'_> {
    fn waiting(&self) -> String {
        if self.is_restart {
            format!(
                "the host restarts; the dashboard reconnects and waits for it to answer again \
                 (at most {} s)",
                self.wait.as_secs()
            )
        } else {
            format!(
                "the host restarts into {}; the dashboard reconnects and waits for it (at most {} s)",
                self.expected.unwrap_or("the new binary"),
                self.wait.as_secs()
            )
        }
    }

    fn answered(&self, v: &str) -> String {
        if self.is_restart {
            format!("the host answers as {v} again: it restarted with the new settings")
        } else {
            format!("the host answers as {v}: the update is accepted")
        }
    }

    fn rolled_back(&self, v: &str) -> String {
        format!(
            "the host came back as {v}, not {}: its rollback ran; the journal on \
             pve (journalctl -u homelab-host) says why",
            self.expected.unwrap_or("the new version")
        )
    }

    fn timed_out(&self) -> String {
        if self.is_restart {
            format!(
                "the host did not come back within {} s after the restart; the \
                 settings page shows what answers now",
                self.wait.as_secs()
            )
        } else {
            format!(
                "the host did not come back as {} within {} s; the host page shows what \
                 answers now",
                self.expected.unwrap_or("the new version"),
                self.wait.as_secs()
            )
        }
    }
}
