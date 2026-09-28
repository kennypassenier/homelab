//! feat-ops-2: the host's open questions, as the dashboard keeps them, and
//! the one decision about an answer: forward it, or refuse it as stale.
//!
//! The host broadcasts `ServerMsg::Ask` to every session and forgets a
//! question when it is answered or times out; it never says that it forgot.
//! So the dashboard keeps what it heard with the moment it heard it, and an
//! answer is sent only for a question that is still open here: same start
//! of the host (`boot`, arch-protocol), same operation and step, and not past
//! the host's wait. The host checks `boot` again on its side.

use homelab_proto::Command;
use serde::{Deserialize, Serialize};

/// One question the host asked, as the browser shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenAsk {
    pub id: u64,
    /// Which start of the host asked. None from a host older than the field.
    pub boot: Option<String>,
    pub op: String,
    pub step: String,
    pub what: String,
    pub if_allowed: String,
    pub if_stopped: String,
    /// Unix seconds the dashboard received it.
    pub asked_at: u64,
    /// Unix seconds after which the host no longer waits.
    pub deadline: u64,
}

/// What the browser posts: the question it saw and the choice.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AnswerRequest {
    pub id: u64,
    #[serde(default)]
    pub boot: Option<String>,
    pub op: String,
    pub step: String,
    pub allow: bool,
}

/// Why an answer is not sent. Each reads as the `why` of an API error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No such question is open (answered, or never heard).
    NotOpen,
    /// The question came from an earlier start of the host.
    OtherStart,
    /// The id is open but for another operation or step: the page is
    /// looking at an older question with a reused id.
    OtherStep,
    /// The host stopped waiting.
    TimedOut,
}

impl Refusal {
    pub fn why(&self) -> &'static str {
        match self {
            Refusal::NotOpen => "that question is not open any more: it was answered or the host stopped asking",
            Refusal::OtherStart => "that question was asked by an earlier start of the host; its id now means nothing",
            Refusal::OtherStep => "the host's open question with that id is about another step; the page showed an older one",
            Refusal::TimedOut => "the host stopped waiting for this answer; the operation went on without it",
        }
    }
}

/// The open questions, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Asks {
    open: Vec<OpenAsk>,
}

impl Asks {
    /// A question heard at `now`, open for `timeout_s`. A question from
    /// another start of the host drops every one of the old start: their
    /// ids restart at 1 and would be mistaken for the new ones.
    #[allow(clippy::too_many_arguments)]
    pub fn heard(
        &mut self,
        id: u64,
        boot: Option<String>,
        op: String,
        step: String,
        what: String,
        if_allowed: String,
        if_stopped: String,
        now: u64,
        timeout_s: u64,
    ) {
        self.open.retain(|a| a.boot == boot && a.id != id);
        self.open.push(OpenAsk {
            id,
            boot,
            op,
            step,
            what,
            if_allowed,
            if_stopped,
            asked_at: now,
            deadline: now.saturating_add(timeout_s),
        });
    }

    /// The questions still open at `now`.
    pub fn open(&self, now: u64) -> Vec<OpenAsk> {
        self.open
            .iter()
            .filter(|a| a.deadline > now)
            .cloned()
            .collect()
    }

    /// Forget the questions past their deadline; true when one went.
    pub fn prune(&mut self, now: u64) -> bool {
        let before = self.open.len();
        self.open.retain(|a| a.deadline > now);
        before != self.open.len()
    }

    /// Forget one question: answered, or the host said it is gone.
    pub fn forget(&mut self, boot: &Option<String>, id: u64) {
        self.open.retain(|a| !(a.id == id && &a.boot == boot));
    }

    /// The command to send for `req` at `now`, or why not.
    pub fn check(&self, req: &AnswerRequest, now: u64) -> Result<Command, Refusal> {
        let same_id: Vec<&OpenAsk> = self.open.iter().filter(|a| a.id == req.id).collect();
        let Some(ask) = same_id.iter().find(|a| a.boot == req.boot) else {
            return Err(if same_id.is_empty() {
                Refusal::NotOpen
            } else {
                Refusal::OtherStart
            });
        };
        if ask.op != req.op || ask.step != req.step {
            return Err(Refusal::OtherStep);
        }
        if ask.deadline <= now {
            return Err(Refusal::TimedOut);
        }
        Ok(Command::Answer {
            id: req.id,
            allow: req.allow,
            boot: req.boot.clone(),
        })
    }
}
