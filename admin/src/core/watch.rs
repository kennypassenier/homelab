//! replace-kuma (Kenny, 2026-09-30: "Vervangen, dashboard meet"): what the
//! dashboard's minute watch decides. Pure: the shell measures, this says
//! when a thing counts as down (five minutes without an answer, the same
//! line notify-routing draws for "a service not answering") and when its
//! return is news.

use std::collections::BTreeMap;

use serde::Serialize;

/// How long something may fail before it is down.
pub const DOWN_AFTER_S: i64 = 300;

/// One thing the watch measures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Target {
    /// Stable id: `tile:<host>`, `stack:<name>`, `host`.
    pub key: String,
    pub name: String,
    pub stack: Option<String>,
    /// The dashboard page a notice links to.
    pub link: String,
}

/// What the watch holds about one target between rounds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Seen {
    /// When the current run of failures began; None while it answers.
    pub failing_since: Option<i64>,
    /// A Down notice went out for this run.
    pub down_told: bool,
    /// When it was last measured, and what the last failure said.
    pub checked_at: i64,
    pub why: Option<String>,
}

/// What one round's reading changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Down { since: i64, why: String },
    Up { was_down_s: i64 },
}

/// Fold one reading into what the watch holds; a change is news to tell.
pub fn step(seen: &mut Seen, now: i64, answer: Result<(), String>) -> Option<Change> {
    seen.checked_at = now;
    match answer {
        Ok(()) => {
            let was = seen.failing_since.take();
            seen.why = None;
            if std::mem::take(&mut seen.down_told) {
                Some(Change::Up {
                    was_down_s: now - was.unwrap_or(now),
                })
            } else {
                None
            }
        }
        Err(why) => {
            let since = *seen.failing_since.get_or_insert(now);
            seen.why = Some(why.clone());
            if !seen.down_told && now - since >= DOWN_AFTER_S {
                seen.down_told = true;
                Some(Change::Down { since, why })
            } else {
                None
            }
        }
    }
}

/// The watch's state for the start page: per key, up or since when down.
pub fn view(targets: &[Target], seen: &BTreeMap<String, Seen>) -> serde_json::Value {
    serde_json::json!(targets
        .iter()
        .map(|t| {
            let s = seen.get(&t.key).cloned().unwrap_or_default();
            serde_json::json!({
                "key": t.key,
                "name": t.name,
                "stack": t.stack,
                "failing_since": s.failing_since,
                "down": s.down_told,
                "checked_at": s.checked_at,
                "why": s.why,
            })
        })
        .collect::<Vec<_>>())
}
