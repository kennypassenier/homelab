//! T69: an operation can stop and put a question to whoever is watching.
//!
//! Kenny's form H1: "twee knoppen die het ofwel toelaten ofwel stoppen". The
//! case that raised it is real and recurring — a service check reports a
//! DELIBERATE drop, routes going 29 → 28 because a route was removed on
//! purpose, and the honest answer is "allow" rather than a failed deploy
//! and an incident bundle nobody needed.
//!
//! The hard part is not the buttons. It is that the SAME operations run
//! unattended: the nightly round at 04:00 has no client attached, and a
//! question asked into an empty room must not hang the night. So an asker
//! always answers — and when nobody is there it says so, rather than
//! pretending to be a person.

/// What came back from the question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A person saw it and said go on.
    Allow,
    /// A person saw it and said stop.
    Stop,
    /// Nobody was listening, or nobody answered in time.
    ///
    /// Deliberately its own value rather than folding into `Stop`. The two
    /// mean different things to a reader of the transcript — "Kenny stopped
    /// this" and "this ran at 04:00 and nobody was there" are different
    /// stories — even where they lead to the same action.
    Unattended(String),
}

impl Answer {
    /// Only a person saying yes lets an operation continue.
    ///
    /// Fail-closed, the same shape as the busy check (O10): the conditions
    /// under which you cannot tell whether anybody is watching are exactly
    /// the conditions in which continuing is a guess.
    pub fn may_continue(&self) -> bool {
        matches!(self, Answer::Allow)
    }
}

/// One question, with everything the operator needs to answer it without
/// scrolling back through the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// Which operation and step is waiting.
    pub op: String,
    pub step: String,
    /// What happened, in one sentence.
    pub what: String,
    /// What choosing "allow" sets in motion, and what choosing "stop" does.
    /// Written per question rather than generic — the same reasoning as the
    /// consequences box on Kenny's forms: a bare Allow/Stop is vocabulary,
    /// not an answer to "what happens if I press this".
    pub if_allowed: String,
    pub if_stopped: String,
}

/// How an operation reaches whoever is watching. Implemented by the host
/// over the live line; `Unattended` in every other context.
#[async_trait::async_trait]
pub trait Asker: Send + Sync {
    async fn ask(&self, q: &Question) -> Answer;
}

/// The asker used when there is nobody to ask: the nightly scheduler, a
/// test, a headless run.
///
/// It is not a stub that returns a convenient answer — it is the honest one,
/// and it carries the reason so the transcript says why the operation went
/// the way it did.
pub struct Unattended(pub &'static str);

/// The one every test and every headless path can borrow. `Unattended`
/// holds a `&'static str` rather than a String precisely so this can exist:
/// a context that borrows an asker cannot borrow a temporary.
pub static NOBODY: Unattended = Unattended("no operator is attached to this run");

#[async_trait::async_trait]
impl Asker for Unattended {
    async fn ask(&self, _q: &Question) -> Answer {
        Answer::Unattended(self.0.to_string())
    }
}
