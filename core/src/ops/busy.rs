//! O10: ask a service whether it is busy before updating it.
//!
//! Jellyfin is the only service in this house where an update lands directly
//! in somebody's evening, so it is the only one that gets asked. The check is
//! deliberately a NAMED one rather than an arbitrary command in a container
//! label: there is exactly one service that needs it, and a label that carries
//! a shell command is a place for one to appear that nobody reviewed.
//!
//! **It fails closed.** The v1 version of this check did the opposite, and it
//! is worth writing down why that was worse than having no check at all. Every
//! uncertain path in it — no API key, Jellyfin unreachable, an empty response —
//! exited 0, meaning "safe to update". So the exact conditions under which you
//! cannot tell whether someone is watching were the conditions in which it said
//! go ahead. It also grepped for a field called `IsPlaying`, which Jellyfin's
//! session objects do not have (measured 2026-08-31 against the live server:
//! they carry `NowPlayingItem` and `PlayState.IsPaused`). So its one positive
//! test could never fire either. It could not have stopped a single update.

/// What a busy-check concluded. `Unknown` is not `Idle` — that distinction is
/// the whole point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Busy {
    /// Somebody is using it; skip the update.
    Yes(String),
    /// Nobody is; go ahead.
    No,
    /// Could not tell. Treated as busy, and says why.
    Unknown(String),
}

impl Busy {
    /// Fail closed: only a definite "no" allows an update.
    pub fn may_update(&self) -> bool {
        matches!(self, Busy::No)
    }
}

/// Read Jellyfin's `/Sessions` response. A session counts as in use when it
/// has a `NowPlayingItem`, paused or not — a paused film is still somebody's
/// evening, and restarting the server drops it either way.
pub fn jellyfin_busy(sessions_json: &str) -> Busy {
    let body = sessions_json.trim();
    if body.is_empty() {
        return Busy::Unknown("Jellyfin returned nothing".into());
    }
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return Busy::Unknown(format!("Jellyfin's answer did not parse: {}", e)),
    };
    let Some(sessions) = parsed.as_array() else {
        return Busy::Unknown("Jellyfin's answer was not a list of sessions".into());
    };
    let watching: Vec<String> = sessions
        .iter()
        .filter(|s| {
            !s.get("NowPlayingItem")
                .unwrap_or(&serde_json::Value::Null)
                .is_null()
        })
        .map(|s| {
            let who = s
                .get("UserName")
                .and_then(|v| v.as_str())
                .unwrap_or("someone");
            let what = s
                .get("NowPlayingItem")
                .and_then(|i| i.get("Name"))
                .and_then(|v| v.as_str())
                .unwrap_or("something");
            let paused = s
                .get("PlayState")
                .and_then(|p| p.get("IsPaused"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            format!(
                "{} is {} {}",
                who,
                if paused { "paused on" } else { "watching" },
                what
            )
        })
        .collect();
    if watching.is_empty() {
        Busy::No
    } else {
        Busy::Yes(watching.join("; "))
    }
}

/// After this many consecutive skips the orchestrator reports instead of
/// quietly deferring forever — a service that is never idle would otherwise
/// simply stop being updated and nothing would say so.
pub const MAX_CONSECUTIVE_SKIPS: u32 = 7;
