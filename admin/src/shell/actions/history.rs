//! The host's history, for expected durations and for a job's outcome.

use std::time::Duration;

use homelab_proto::Command;

use super::Actions;
use crate::core::actions_progress::{self, Outcome};

impl Actions {
    pub(super) async fn history(&self) -> Vec<homelab_core::history::HistoryEntry> {
        let since = (self.now() - self.history_window_s()).max(0) as u64;
        let reply = self
            .inner
            .host
            .ask_traced(
                Command::History {
                    since,
                    until: None,
                    limit: 5000,
                },
                Duration::from_secs(30),
                None,
            )
            .await;
        reply
            .ok()
            .filter(|r| r.ok)
            .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.message).ok())
            .and_then(|v| serde_json::from_value(v.get("entries")?.clone()).ok())
            .unwrap_or_default()
    }

    /// arch-self, GET `/data/actions/outcome`: what the host's history says
    /// became of `subject` (a job's `{action}-{stack}`) since `since` (the
    /// job's `started_at`). The tab that started a job restarting the
    /// dashboard has no other way to learn how it ended (`outcome_since`,
    /// `actions_progress`).
    pub async fn outcome_since(&self, subject: &str, since: i64) -> Outcome {
        let history = self.history().await;
        actions_progress::outcome_since(&history, subject, since)
    }
}
