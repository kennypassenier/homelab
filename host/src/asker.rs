//! Questions the host asks a person mid-operation: the live asker, the notice that announces one, and the list of open ones.

use crate::*;

/// T69: the asker that reaches a watching operator over the live line.
///
/// Two things make it safe to use from code that also runs unattended.
/// First, it checks whether ANYONE is subscribed before it waits at all —
/// the nightly round at 04:00 has no client, so it answers immediately
/// instead of burning a timeout per question. Second, when somebody is
/// listening but nobody answers, it gives up after a bounded wait and says
/// `Unattended`, which is deliberately not the same answer as `Stop`.
pub(crate) struct LiveAsker<'a> {
    pub(crate) state: &'a AppState,
    pub(crate) timeout_s: u64,
    /// fix-240: raise the question as a notice (urgent: pushed) so a person
    /// is told. Off only in tests that have no state directory of their own.
    pub(crate) notify: bool,
}

#[async_trait::async_trait]
impl homelab_core::ask::Asker for LiveAsker<'_> {
    async fn ask(&self, q: &homelab_core::ask::Question) -> homelab_core::ask::Answer {
        use homelab_core::ask::Answer;
        if self.state.log_tx.receiver_count() == 0 {
            return Answer::Unattended("no client is connected to answer".into());
        }
        let id = self
            .state
            .next_ask_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let ask = ServerMsg::Ask {
            boot: Some(self.state.boot_id.clone()),
            id,
            op: q.op.clone(),
            step: q.step.clone(),
            what: q.what.clone(),
            if_allowed: q.if_allowed.clone(),
            if_stopped: q.if_stopped.clone(),
            // fix-240: the banner and the prompt show the host's own wait.
            wait_s: Some(self.timeout_s),
        };
        if let Ok(mut g) = self.state.pending_asks.lock() {
            g.insert(
                id,
                PendingAsk {
                    reply: tx,
                    ask: ask.clone(),
                },
            );
        }
        let _ = self.state.log_tx.send(ask);
        // fix-240: a person is told, the way every other event reaches him —
        // a banner nobody is looking at is not telling anybody. The wait
        // starts after the notice is written; a push that fails is recorded
        // on the notice and never fails the question.
        let asked_at = unix_now();
        if self.notify {
            publish_question_notice(self.state, q, id, self.timeout_s, asked_at).await;
        }
        let wait = homelab_core::ask::wait_words(self.timeout_s);
        let answer = match tokio::time::timeout(Duration::from_secs(self.timeout_s), rx).await {
            Ok(Ok(true)) => Answer::Allow,
            Ok(Ok(false)) => Answer::Stop,
            // The sender was dropped: the client disconnected mid-question.
            Ok(Err(_)) => Answer::Unattended("the client went away before answering".into()),
            Err(_) => Answer::Unattended(format!(
                "nobody answered within {} — the operation did not guess",
                wait
            )),
        };
        if let Ok(mut g) = self.state.pending_asks.lock() {
            g.remove(&id);
        }
        // fix-240: every session learns the question is settled, so a
        // banner or prompt for it goes where it was not answered.
        let _ = self.state.log_tx.send(ServerMsg::AskSettled {
            id,
            boot: Some(self.state.boot_id.clone()),
            how: match &answer {
                Answer::Allow => "allowed".into(),
                Answer::Stop => "stopped".into(),
                Answer::Unattended(why) => why.clone(),
            },
        });
        answer
    }
}

/// fix-240: the notice for a question that is waiting, urgent so it is
/// pushed. Its op names the question (`question-<op>-<id>`): the 20 h
/// damper judges by op and text, and a second question about the same
/// reading is a second thing a person must answer, not a repeat.
pub(crate) async fn publish_question_notice(
    state: &AppState,
    q: &homelab_core::ask::Question,
    id: u64,
    wait_s: u64,
    since: u64,
) {
    publish_notice(
        state,
        &RealExecutor,
        NoticeFacts {
            op: format!("question-{}-{}", q.op, id),
            label: "question".into(),
            ok: false,
            deferred: false,
            since,
            ex: homelab_core::notify::explain_question(q, id, wait_s),
            urgency: homelab_core::notify::urgency(&homelab_core::notify::Event::Question),
            incident: None,
            req: None,
            by: None,
            findings: Vec::new(),
        },
    )
    .await;
}

/// fix-240: the questions open right now, oldest first, as `pick_open`
/// reads them.
pub(crate) fn open_questions(state: &AppState) -> Vec<homelab_core::ask::OpenQuestion> {
    let guard = state
        .pending_asks
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut open: Vec<homelab_core::ask::OpenQuestion> = guard
        .iter()
        .filter_map(|(id, p)| match &p.ask {
            ServerMsg::Ask { op, step, what, .. } => Some(homelab_core::ask::OpenQuestion {
                id: *id,
                op: op.clone(),
                step: step.clone(),
                what: what.clone(),
            }),
            _ => None,
        })
        .collect();
    open.sort_by_key(|q| q.id);
    open
}
