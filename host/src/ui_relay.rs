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

    /// Hand `step` to the dashboard and wait for its answer: `(ok, JSON)`.
    /// `wait` is the quiet time allowed; a `UiHold` replaces it with the
    /// time the dashboard asks for, and its note goes to `note`.
    pub async fn relay(
        &self,
        by: &str,
        scope: Scope,
        step: UiStep,
        wait: Duration,
        note: &(dyn Fn(String) + Send + Sync),
    ) -> (bool, String) {
        let what = format!("ui {}", step.verb());
        let Some(out) = self
            .attached
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .first()
            .map(|x| x.out.clone())
        else {
            return (
                false,
                refusal(
                    &what,
                    "no dashboard is attached to the host",
                    "start homelab-admin (CT 120) and wait until its host link is up; `homelab ui state` answers once it is",
                ),
            );
        };
        let relay = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, mut rx) = oneshot::channel();
        let (hold_tx, mut holds) = mpsc::unbounded_channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                relay,
                Waiting {
                    answer: tx,
                    hold: hold_tx,
                },
            );
        let msg = ServerMsg::Ui {
            relay,
            by: by.to_string(),
            scope,
            step,
        };
        if out.send(msg).await.is_err() {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&relay);
            return (
                false,
                refusal(
                    &what,
                    "the dashboard's session ended before the step reached it",
                    "wait for the dashboard to reconnect and send the step again",
                ),
            );
        }
        let mut allowed = wait;
        let mut deadline = tokio::time::Instant::now() + wait;
        let mut held_note: Option<String> = None;
        let answer = loop {
            tokio::select! {
                a = &mut rx => break a.map_err(|_| true),
                Some((more, n)) = holds.recv() => {
                    allowed = more;
                    deadline = tokio::time::Instant::now() + more;
                    if let Some(n) = n {
                        note(n.clone());
                        held_note = Some(n);
                    }
                }
                _ = tokio::time::sleep_until(deadline) => break Err(false),
            }
        };
        match answer {
            Ok(answer) => answer,
            Err(true) => (
                false,
                refusal(
                    &what,
                    "the dashboard's session ended before it answered",
                    "wait for the dashboard to reconnect; `homelab ui state` shows whether this step took effect",
                ),
            ),
            Err(false) => {
                self.pending
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&relay);
                let why = match &held_note {
                    Some(n) => format!(
                        "the dashboard held the step ({n}) and did not answer within {} s",
                        allowed.as_secs()
                    ),
                    None => format!(
                        "the dashboard did not answer within {} s",
                        allowed.as_secs()
                    ),
                };
                (
                    false,
                    refusal(
                        &what,
                        &why,
                        "`homelab ui state` shows where the dashboard is now; whether this step took effect is in it",
                    ),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn follow_without_a_dashboard_the_step_is_refused_with_what_why_fix() {
        let r = UiRelay::default();
        let (ok, msg) = r
            .relay(
                "wsl",
                Scope::Operate,
                UiStep::State,
                Duration::from_millis(50),
                &|_| {},
            )
            .await;
        assert!(!ok);
        let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["refusal"]["what"], "ui state");
        assert!(
            v["refusal"]["fix"]
                .as_str()
                .unwrap()
                .contains("homelab-admin")
        );
    }

    #[tokio::test]
    async fn follow_the_step_reaches_the_attached_session_and_its_answer_comes_back() {
        let r = std::sync::Arc::new(UiRelay::default());
        let (tx, mut rx) = mpsc::channel(4);
        r.attach(7, "admin", tx);
        let r2 = r.clone();
        let dashboard = tokio::spawn(async move {
            let Some(ServerMsg::Ui {
                relay,
                by,
                scope,
                step,
            }) = rx.recv().await
            else {
                panic!("no step")
            };
            assert_eq!(
                (by.as_str(), scope, step),
                ("wsl", Scope::Operate, UiStep::Close)
            );
            // Another session may not answer for the dashboard.
            assert!(r2.reply(8, relay, true, "forged".into()).is_err());
            r2.reply(7, relay, true, "{\"ok\":true}".into()).unwrap();
        });
        let (ok, msg) = r
            .relay(
                "wsl",
                Scope::Operate,
                UiStep::Close,
                Duration::from_secs(2),
                &|_| {},
            )
            .await;
        dashboard.await.unwrap();
        assert!(ok);
        assert_eq!(msg, "{\"ok\":true}");
        // Its end detaches it; an older session's end does not.
        r.detach(3);
        assert!(r.attached());
        r.detach(7);
        assert!(!r.attached());
    }

    /// fix-158: a second dashboard (a developer's, on the same token) that
    /// attached and then ended used to leave nobody attached, while CT 120's
    /// dashboard was still connected. The host remembers every attached
    /// session, most recent first, and falls back to the next one still
    /// connected; the last one's end leaves none.
    #[tokio::test]
    async fn fix_158_when_the_attached_dashboard_ends_the_one_before_it_is_attached_again() {
        let r = UiRelay::default();
        let (a_tx, _a_rx) = mpsc::channel(4);
        let (b_tx, _b_rx) = mpsc::channel(4);
        r.attach(1, "admin", a_tx);
        r.attach(2, "admin", b_tx);
        assert_eq!(r.attached_session(), Some(2), "the newest attach wins");
        assert_eq!(r.detach(2), Detached::FellBackTo(1, "admin".into()));
        assert_eq!(r.attached_session(), Some(1), "A is still connected");
        assert_eq!(r.detach(1), Detached::NoneLeft);
        assert_eq!(r.attached_session(), None);

        // A session whose line already closed is no fallback.
        let (a_tx, a_rx) = mpsc::channel(4);
        let (b_tx, _b_rx) = mpsc::channel(4);
        r.attach(3, "admin", a_tx);
        r.attach(4, "admin", b_tx);
        drop(a_rx);
        assert_eq!(r.detach(4), Detached::NoneLeft);
        assert_eq!(r.attached_session(), None);

        // An earlier session's end leaves the attached one attached.
        let (a_tx, _a_rx) = mpsc::channel(4);
        let (b_tx, _b_rx) = mpsc::channel(4);
        r.attach(5, "admin", a_tx);
        r.attach(6, "admin", b_tx);
        assert_eq!(r.detach(5), Detached::NotAttached);
        assert_eq!(r.attached_session(), Some(6));
        r.detach(6);
        assert_eq!(r.attached_session(), None, "5 is gone, not a fallback");
    }

    /// Live view: a paused step outlives the usual wait because the
    /// dashboard holds it; the note reaches the CLI; only the dashboard may
    /// hold; past the hold the step fails with what, why and fix.
    #[tokio::test(start_paused = true)]
    async fn follow_a_held_step_outlives_the_usual_wait_and_its_note_reaches_the_cli() {
        let r = std::sync::Arc::new(UiRelay::default());
        let (tx, mut rx) = mpsc::channel(4);
        r.attach(7, "admin", tx);
        let r2 = r.clone();
        let dashboard = tokio::spawn(async move {
            let Some(ServerMsg::Ui { relay, .. }) = rx.recv().await else {
                panic!("no step")
            };
            assert!(r2.hold(8, relay, 600, None).is_err(), "another session");
            r2.hold(7, relay, 600, Some("paused by the viewer kenny".into()))
                .unwrap();
            // Far past the usual 20 s, inside the hold.
            tokio::time::sleep(Duration::from_secs(300)).await;
            r2.reply(7, relay, true, "{\"ok\":true}".into()).unwrap();
        });
        let notes = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        let n2 = notes.clone();
        let (ok, msg) = r
            .relay(
                "wsl",
                Scope::Operate,
                UiStep::Close,
                RELAY_WAIT,
                &move |n| n2.lock().unwrap().push(n),
            )
            .await;
        dashboard.await.unwrap();
        assert!(ok, "{msg}");
        assert_eq!(
            notes.lock().unwrap().as_slice(),
            ["paused by the viewer kenny"]
        );

        // A hold that runs out answers with what, why and fix.
        let (tx, mut rx) = mpsc::channel(4);
        r.attach(9, "admin", tx);
        let r3 = r.clone();
        tokio::spawn(async move {
            if let Some(ServerMsg::Ui { relay, .. }) = rx.recv().await {
                r3.hold(9, relay, 60, Some("paused by the viewer kenny".into()))
                    .unwrap();
            }
        });
        let (ok, msg) = r
            .relay("wsl", Scope::Operate, UiStep::Close, RELAY_WAIT, &|_| {})
            .await;
        assert!(!ok);
        let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert!(
            v["refusal"]["why"]
                .as_str()
                .unwrap()
                .contains("paused by the viewer kenny"),
            "{v}"
        );
        assert!(!v["refusal"]["fix"].as_str().unwrap().is_empty());

        // The hold is capped, and the dashboard's end answers at once.
        let (tx, mut rx) = mpsc::channel(4);
        r.attach(11, "admin", tx);
        let r4 = r.clone();
        tokio::spawn(async move {
            if let Some(ServerMsg::Ui { relay, .. }) = rx.recv().await {
                r4.hold(11, relay, u64::MAX, None).unwrap();
                r4.detach(11);
            }
        });
        let started = tokio::time::Instant::now();
        let (ok, _) = r
            .relay("wsl", Scope::Operate, UiStep::Close, RELAY_WAIT, &|_| {})
            .await;
        assert!(!ok);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
