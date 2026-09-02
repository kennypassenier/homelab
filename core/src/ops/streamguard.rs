//! A7 · do not restart a service while somebody is watching it.
//!
//! M5's third exit condition, blocked since 2026-08-31 because the Jellyfin
//! API key was refused three ways (F32). That changed: the key read out of
//! Jellyfin's own database for the front page works, and `GET /Sessions`
//! answers. Measured 2026-09-02 — one session, playing.
//!
//! Kenny chose **fail closed** (form A7): no answer means no update. The
//! trade is deliberate and asymmetric. A skipped update costs a day and the
//! next round takes it. A container restarting in the middle of an episode
//! costs the evening, and more than that it teaches the household that the
//! automation is something to work around.

/// What the shell read off the media server, so the decision stays pure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreamFact {
    /// Sessions with something loaded — playing or paused.
    pub playing: usize,
    /// Who and what, for the reason line. Never more than a few.
    pub detail: Vec<String>,
    /// The question could not be asked or could not be understood.
    pub error: Option<String>,
}

/// Whether an automatic update may go ahead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Proceed,
    /// Skipped, with the sentence that travels into the notification.
    Skip(String),
}

/// Read Jellyfin's `/Sessions` answer.
///
/// A session counts when it has a `NowPlayingItem`, paused included: a paused
/// episode is a viewing in progress, and restarting under it loses the place
/// just as thoroughly as restarting during playback.
pub fn parse_sessions(body: &str) -> Result<StreamFact, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("the answer is not JSON: {}", e))?;
    let Some(arr) = v.as_array() else {
        return Err("the answer is not a list of sessions".into());
    };
    let mut detail = Vec::new();
    let mut playing = 0usize;
    for s in arr {
        let Some(item) = s.get("NowPlayingItem") else {
            continue;
        };
        playing += 1;
        if detail.len() < 3 {
            let who = s
                .get("UserName")
                .and_then(|x| x.as_str())
                .unwrap_or("somebody");
            let what = item
                .get("Name")
                .and_then(|x| x.as_str())
                .unwrap_or("something");
            let paused = s
                .get("PlayState")
                .and_then(|p| p.get("IsPaused"))
                .and_then(|p| p.as_bool())
                .unwrap_or(false);
            detail.push(format!(
                "{} · {}{}",
                who,
                what,
                if paused { " (paused)" } else { "" }
            ));
        }
    }
    Ok(StreamFact {
        playing,
        detail,
        error: None,
    })
}

/// The decision. Fails CLOSED: anything other than a clear "nobody is
/// watching" is a skip.
pub fn gate(fact: &StreamFact) -> Gate {
    if let Some(e) = &fact.error {
        return Gate::Skip(format!(
            "the media server did not answer whether anybody is watching ({}) — \
             skipped rather than guessed, and the next round will ask again",
            e
        ));
    }
    if fact.playing == 0 {
        return Gate::Proceed;
    }
    Gate::Skip(format!(
        "{} session(s) in progress: {}",
        fact.playing,
        fact.detail.join(", ")
    ))
}
