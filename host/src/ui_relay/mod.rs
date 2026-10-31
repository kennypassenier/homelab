//! feat-platform-10 (milestone follow): the relay between `homelab ui` and
//! the dashboard.
//!
//! Claude drives Kenny's open dashboard without a browser: each UI step is a
//! `Command::Ui` on Claude's own scoped token, over the one host line. The
//! host does not interpret it; it hands the step to the session that said
//! `UiAttach` (the dashboard's), as `ServerMsg::Ui`, and answers the CLI with
//! whatever that session sends back as `UiReply`. No second entry point, no
//! browser, nothing past Cloudflare Access.
//!
//! Live view (decided 2026-09-29): the dashboard announces a step and holds
//! it for a short countdown, and a viewer may pause it. A step the dashboard
//! holds longer than [`RELAY_WAIT`] is kept alive with `UiHold`: the host
//! then waits the time the dashboard asks for (at most
//! [`homelab_proto::UI_HOLD_MAX_S`]) and hands its note ("paused by the
//! viewer …") to the waiting CLI.
//!
//! fix-158 (2026-09-29): the host remembers every session that attached,
//! most recent first. The newest one gets the steps; when it ends, the next
//! one still connected is attached again. One attached slot meant a
//! developer's dashboard on the same token took the steps over and, when it
//! closed, left CT 120's dashboard connected but no longer attached.

mod relay;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use homelab_proto::{Scope, ServerMsg, UI_HOLD_MAX_S, UI_RELAY_WAIT_S, UiStep};
use tokio::sync::{mpsc, oneshot};

/// How long a step waits for the dashboard's answer, unless the dashboard
/// holds it longer (`UiHold`).
pub const RELAY_WAIT: Duration = Duration::from_secs(UI_RELAY_WAIT_S);

/// A hold: wait this much longer from now, and the note for the CLI.
type Hold = (Duration, Option<String>);

struct Waiting {
    answer: oneshot::Sender<(bool, String)>,
    hold: mpsc::UnboundedSender<Hold>,
}

/// A session that said `UiAttach`.
struct Attached {
    session: u64,
    /// The token it signed in with, for the log.
    who: String,
    out: mpsc::Sender<ServerMsg>,
}

/// What a session's end did to the relay (fix-158), for the host's log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detached {
    /// It was not the attached dashboard: nothing changed for the steps.
    NotAttached,
    /// It was; this earlier session (number, token) gets the steps now.
    FellBackTo(u64, String),
    /// It was, and no other attached session is still connected.
    NoneLeft,
}

pub struct UiRelay {
    /// Every attached session, most recent first; the first one is the
    /// dashboard the steps go to (fix-158).
    attached: Mutex<Vec<Attached>>,
    pending: Mutex<HashMap<u64, Waiting>>,
    next: AtomicU64,
}

impl Default for UiRelay {
    fn default() -> Self {
        UiRelay {
            attached: Mutex::new(Vec::new()),
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        }
    }
}

/// The CLI's answer when the step never reached a dashboard, in the same
/// JSON shape the dashboard answers with.
fn refusal(what: &str, why: &str, fix: &str) -> String {
    serde_json::json!({
        "ok": false,
        "refusal": { "what": what, "why": why, "fix": fix },
        "state": null,
    })
    .to_string()
}

impl UiRelay {
    /// `session` (signed in as `who`) is the dashboard now. A newer attach
    /// goes in front of the older ones, which are kept to fall back to
    /// (fix-158); a session attaching again moves to the front.
    pub fn attach(&self, session: u64, who: &str, out: mpsc::Sender<ServerMsg>) {
        let mut a = self.attached.lock().unwrap_or_else(PoisonError::into_inner);
        a.retain(|x| x.session != session);
        a.insert(
            0,
            Attached {
                session,
                who: who.to_string(),
                out,
            },
        );
    }

    /// The session ended and is forgotten. If it was the attached
    /// dashboard, every step it still held is answered at once (a paused
    /// step would otherwise wait out its whole hold for an answer that
    /// cannot come), and the most recent earlier session whose line is
    /// still open is attached again (fix-158).
    pub fn detach(&self, session: u64) -> Detached {
        let mut a = self.attached.lock().unwrap_or_else(PoisonError::into_inner);
        let was_attached = a.first().map(|x| x.session) == Some(session);
        a.retain(|x| x.session != session);
        if !was_attached {
            return Detached::NotAttached;
        }
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        // A session whose line closed without its end reaching here yet is
        // no fallback; its own end forgets it anyway.
        a.retain(|x| !x.out.is_closed());
        match a.first() {
            Some(x) => Detached::FellBackTo(x.session, x.who.clone()),
            None => Detached::NoneLeft,
        }
    }

    fn attached_is(&self, session: u64) -> bool {
        self.attached
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .first()
            .map(|x| x.session)
            == Some(session)
    }

    /// The dashboard holds relay `relay` for `wait_s` more seconds (capped
    /// at [`UI_HOLD_MAX_S`]); `note` goes to the waiting CLI. Only the
    /// attached session may hold a step.
    pub fn hold(
        &self,
        session: u64,
        relay: u64,
        wait_s: u64,
        note: Option<String>,
    ) -> Result<(), String> {
        if !self.attached_is(session) {
            return Err("refused: this session is not the attached dashboard".into());
        }
        let pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        match pending.get(&relay) {
            Some(w) => {
                let wait = Duration::from_secs(wait_s.min(UI_HOLD_MAX_S));
                let _ = w.hold.send((wait, note));
                Ok(())
            }
            None => Err(format!(
                "no UI step {relay} is waiting (it timed out, or was answered already)"
            )),
        }
    }

    #[cfg(test)]
    pub fn attached_session(&self) -> Option<u64> {
        self.attached
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .first()
            .map(|x| x.session)
    }

    #[cfg(test)]
    pub fn attached(&self) -> bool {
        self.attached_session().is_some()
    }

    /// The dashboard's answer to relay `relay`. Only the attached session
    /// may answer: any other session is refused, so a second token cannot
    /// put words in the dashboard's mouth.
    pub fn reply(&self, session: u64, relay: u64, ok: bool, message: String) -> Result<(), String> {
        if !self.attached_is(session) {
            return Err("refused: this session is not the attached dashboard".into());
        }
        let waiting = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&relay);
        match waiting {
            Some(w) => {
                let _ = w.answer.send((ok, message));
                Ok(())
            }
            None => Err(format!(
                "no UI step {relay} is waiting (it timed out, or was answered already)"
            )),
        }
    }
}
