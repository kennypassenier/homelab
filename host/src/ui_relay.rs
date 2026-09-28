//! feat-platform-10 (milestone follow): the relay between `homelab ui` and
//! the dashboard.
//!
//! Claude drives Kenny's open dashboard without a browser: each UI step is a
//! `Command::Ui` on Claude's own scoped token, over the one host line. The
//! host does not interpret it; it hands the step to the session that said
//! `UiAttach` (the dashboard's), as `ServerMsg::Ui`, and answers the CLI with
//! whatever that session sends back as `UiReply`. No second entry point, no
//! browser, nothing past Cloudflare Access.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use homelab_proto::{Scope, ServerMsg, UiStep};
use tokio::sync::{mpsc, oneshot};

/// How long a step waits for the dashboard's answer.
pub const RELAY_WAIT: Duration = Duration::from_secs(20);

type Waiting = oneshot::Sender<(bool, String)>;

pub struct UiRelay {
    /// The attached session: its number and its outgoing channel.
    target: Mutex<Option<(u64, mpsc::Sender<ServerMsg>)>>,
    pending: Mutex<HashMap<u64, Waiting>>,
    next: AtomicU64,
}

impl Default for UiRelay {
    fn default() -> Self {
        UiRelay {
            target: Mutex::new(None),
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
    /// `session` is the dashboard now; a newer attach replaces an older one
    /// (the dashboard reconnected).
    pub fn attach(&self, session: u64, out: mpsc::Sender<ServerMsg>) {
        *self.target.lock().unwrap_or_else(PoisonError::into_inner) = Some((session, out));
    }

    /// The session ended; if it was the dashboard, nobody is attached.
    pub fn detach(&self, session: u64) {
        let mut t = self.target.lock().unwrap_or_else(PoisonError::into_inner);
        if t.as_ref().map(|(s, _)| *s) == Some(session) {
            *t = None;
        }
    }

    #[cfg(test)]
    pub fn attached(&self) -> bool {
        self.target
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// The dashboard's answer to relay `relay`. Only the attached session
    /// may answer: any other session is refused, so a second token cannot
    /// put words in the dashboard's mouth.
    pub fn reply(&self, session: u64, relay: u64, ok: bool, message: String) -> Result<(), String> {
        let current = self
            .target
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|(s, _)| *s);
        if current != Some(session) {
            return Err("refused: this session is not the attached dashboard".into());
        }
        let waiting = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&relay);
        match waiting {
            Some(w) => {
                let _ = w.send((ok, message));
                Ok(())
            }
            None => Err(format!(
                "no UI step {relay} is waiting (it timed out, or was answered already)"
            )),
        }
    }

    /// Hand `step` to the dashboard and wait for its answer: `(ok, JSON)`.
    pub async fn relay(
        &self,
        by: &str,
        scope: Scope,
        step: UiStep,
        wait: Duration,
    ) -> (bool, String) {
        let what = format!("ui {}", step.verb());
        let Some(out) = self
            .target
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|(_, o)| o.clone())
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
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(relay, tx);
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
        match tokio::time::timeout(wait, rx).await {
            Ok(Ok(answer)) => answer,
            _ => {
                self.pending
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&relay);
                (
                    false,
                    refusal(
                        &what,
                        &format!("the dashboard did not answer within {} s", wait.as_secs()),
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
            )
            .await;
        assert!(!ok);
        let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(v["refusal"]["what"], "ui state");
        assert!(v["refusal"]["fix"]
            .as_str()
            .unwrap()
            .contains("homelab-admin"));
    }

    #[tokio::test]
    async fn follow_the_step_reaches_the_attached_session_and_its_answer_comes_back() {
        let r = std::sync::Arc::new(UiRelay::default());
        let (tx, mut rx) = mpsc::channel(4);
        r.attach(7, tx);
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
            .relay("wsl", Scope::Operate, UiStep::Close, Duration::from_secs(2))
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
}
