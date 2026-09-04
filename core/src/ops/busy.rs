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

use crate::error::CoreError;
use crate::executor::Executor;

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

/// Which named check an app wants, from the `com.homelab.update.busy-check`
/// label on its running container. `None` means the app never asks — which is
/// every app but one.
///
/// The label lives on the container rather than in the manifest because that
/// is where the running truth is: a container that is not up cannot be busy,
/// and this returns `None` for it without a second call.
pub async fn wanted_check(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
) -> Result<Option<String>, CoreError> {
    let out = crate::ops::util_pct_sh(
        exec,
        vmid,
        &format!(
            "cd '/opt/{}/{}' && docker compose ps -q | head -1 | xargs -r docker inspect --format '{{{{index .Config.Labels \"com.homelab.update.busy-check\"}}}}'",
            stack, app
        ),
        60,
    )
    .await?;
    let v = out.stdout.trim();
    Ok(if v.is_empty() || v == "<no value>" {
        None
    } else {
        Some(v.to_string())
    })
}

/// Ask one app whether it is in use. `None` when it carries no busy-check
/// label; otherwise a verdict that fails closed.
///
/// Both callers go through here on purpose. The update path had this check
/// and the backup path did not, and the backup path is the one that stops
/// containers every single night: at 04:17 on 2026-09-04 it stopped Jellyfin
/// while Kenny was watching, which is the one thing the check was written to
/// prevent (F280). One question, asked from both places, cannot drift apart.
pub async fn app_busy(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
) -> Result<Option<Busy>, CoreError> {
    let Some(kind) = wanted_check(exec, vmid, stack, app).await? else {
        return Ok(None);
    };
    if kind != "jellyfin" {
        return Ok(Some(Busy::Unknown(format!(
            "the container asks for a busy-check named `{}`, and no such check exists",
            kind
        ))));
    }
    // F213: the key comes from the application itself, not from an `.env`.
    // The media stack declares no `latch_secrets` and has no `.env` at all,
    // so this used to source a file that does not exist and ask with an empty
    // token — the check could never have answered, which is half of why the
    // label was never switched on. Same source the deploy's own checks use
    // (D102), and for the same reason: a key copied anywhere goes stale
    // without saying so, and F32 was exactly that.
    let out = crate::ops::util_pct_sh(
        exec,
        vmid,
        &format!(
            "K=$(sqlite3 /appdata/{}/{}-config/data/jellyfin.db \
               'select AccessToken from ApiKeys limit 1') && \
             curl -sf -m 10 -H \"Authorization: MediaBrowser Token=$K\" \
             http://127.0.0.1:8096/Sessions",
            stack, app
        ),
        30,
    )
    .await?;
    Ok(Some(jellyfin_busy(&out.stdout)))
}

/// The sentence a caller shows when it stands aside. Kept here so the update
/// path and the backup path phrase it identically.
pub fn reason(verdict: &Busy) -> String {
    match verdict {
        Busy::Yes(who) => format!("in use — {}", who),
        Busy::Unknown(why) => format!("could not tell ({}), so treating it as in use", why),
        Busy::No => "not in use".to_string(),
    }
}
