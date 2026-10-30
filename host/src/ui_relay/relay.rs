//! The relay itself: hand one UI step to the attached dashboard and wait for
//! its answer, letting a held step outlive the usual wait.

use super::*;

impl UiRelay {
    /// Hand `step` to the dashboard and wait for its answer: `(ok, JSON)`.
    /// `wait` is the quiet time allowed; a `UiHold` replaces it with the
    /// time the dashboard asks for, and its note goes to `note`.
    /// `client_version` (fix-185): the CLI's own build, so the dashboard can
    /// tell a stale tab from the client driving it.
    pub async fn relay(
        &self,
        by: &str,
        scope: Scope,
        step: UiStep,
        client_version: &str,
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
            client_version: client_version.to_string(),
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
