//! replace-kuma (Kenny, 2026-09-30: "Vervangen, dashboard meet"): what the
//! dashboard's minute watch decides. Pure: the shell measures, this says
//! when a thing counts as down (five minutes without an answer, the same
//! line notify-routing draws for "a service not answering") and when its
//! return is news.

use std::collections::BTreeMap;

use serde::Serialize;

/// How long something may fail before it is down, unless the host's
/// `watch_down_after_s` (or a tile's own `down_after`) says otherwise.
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
    /// When the Down notice went out (`down_told` became true); this, not
    /// `failing_since`, is what an Up notice's `was_down_s` counts from —
    /// the outage was only "down" from the notice on, not from the first
    /// flaky failure.
    pub down_since: Option<i64>,
    /// When it was last measured, and what the last failure said.
    pub checked_at: i64,
    pub why: Option<String>,
    /// Decision "deploys are known outages" (Kenny, 2026-09-30): the target
    /// is not being asked this round because its stack is known to be
    /// deploying — a real outage, but not a fault, so it is never a Down
    /// notice and it never starts (or keeps) the down timer running.
    #[serde(default)]
    pub deploying: bool,
}

/// What one round's reading changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Down { since: i64, why: String },
    Up { was_down_s: i64 },
}

/// Fold one reading into what the watch holds; a change is news to tell.
/// `down_after_s` is [`DOWN_AFTER_S`] unless the host or the tile itself
/// says otherwise.
pub fn step(
    seen: &mut Seen,
    now: i64,
    answer: Result<(), String>,
    down_after_s: i64,
) -> Option<Change> {
    seen.checked_at = now;
    seen.deploying = false;
    match answer {
        Ok(()) => {
            seen.failing_since = None;
            let down_since = seen.down_since.take();
            seen.why = None;
            if std::mem::take(&mut seen.down_told) {
                Some(Change::Up {
                    was_down_s: now - down_since.unwrap_or(now),
                })
            } else {
                None
            }
        }
        Err(why) => {
            let since = *seen.failing_since.get_or_insert(now);
            seen.why = Some(why.clone());
            if !seen.down_told && now - since >= down_after_s {
                seen.down_told = true;
                seen.down_since = Some(now);
                Some(Change::Down { since, why })
            } else {
                None
            }
        }
    }
}

/// Decision "deploys are known outages": this round is skipped because the
/// target's stack is known to be deploying. No notice, and the down timer
/// is cleared so it restarts from zero once the deploy has ended (a step
/// that answers late right after a deploy is not instantly "down").
pub fn step_deploying(seen: &mut Seen, now: i64) {
    seen.checked_at = now;
    seen.deploying = true;
    seen.failing_since = None;
    seen.down_told = false;
    seen.down_since = None;
    seen.why = None;
}

/// Owner decision "default plus per tile" (2026-09-30): is a tile last
/// checked at `last_checked_at` skipped this round, because its own
/// `watch_every_s` has not passed yet? `last_checked_at` of 0 (never
/// checked) is never too soon.
pub fn too_soon(last_checked_at: i64, now: i64, watch_every_s: i64) -> bool {
    last_checked_at != 0 && now - last_checked_at < watch_every_s
}

/// One target's state, as the home page's dot reads it.
fn state_of(s: &Seen) -> &'static str {
    if s.deploying {
        "deploying"
    } else if s.down_told {
        "down"
    } else if s.failing_since.is_some() {
        "flaky"
    } else {
        "up"
    }
}

/// The watch's state for the start page: per key, up, flaky, down or
/// deploying.
pub fn view(targets: &[Target], seen: &BTreeMap<String, Seen>) -> serde_json::Value {
    serde_json::json!(
        targets
            .iter()
            .map(|t| {
                let s = seen.get(&t.key).cloned().unwrap_or_default();
                serde_json::json!({
                    "key": t.key,
                    "name": t.name,
                    "stack": t.stack,
                    "failing_since": s.failing_since,
                    "down": s.down_told,
                    "state": state_of(&s),
                    "checked_at": s.checked_at,
                    "why": s.why,
                })
            })
            .collect::<Vec<_>>()
    )
}
