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

/// fix-240 (2026-10-02): how long a question waits for a person when the
/// host's configuration says nothing. Ten minutes: 120 s ran out before
/// Kenny had seen the banner of a question a headless run raised. Long
/// enough to be told by the notice, open the dashboard or a terminal and
/// decide; short enough that a question nobody will answer does not hold
/// the operation lock for the rest of the night.
pub const DEFAULT_WAIT_S: u64 = 600;

/// fix-240: a wait in the words a person reads ("10 min", "2 min 30 s",
/// "45 s").
pub fn wait_words(seconds: u64) -> String {
    match (seconds / 60, seconds % 60) {
        (0, s) => format!("{} s", s),
        (m, 0) => format!("{} min", m),
        (m, s) => format!("{} min {} s", m, s),
    }
}

/// fix-240: one question a running operation is waiting on, as the host
/// lists it for an answer that names the operation rather than the
/// question's id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenQuestion {
    pub id: u64,
    pub op: String,
    pub step: String,
    pub what: String,
}

impl OpenQuestion {
    /// One line naming it, for a refusal that lists what is open.
    pub fn line(&self) -> String {
        format!(
            "question {} · {} :: {} — {}",
            self.id, self.op, self.step, self.what
        )
    }
}

/// fix-240: which open question an answer from outside the asking session
/// is for (`homelab answer [<target>] allow|stop`, `homelab ui answer …`).
///
/// `target` matches a question by its id, by its operation's name
/// (`deploy-media`), or by the subject after the operation's verb (`media`
/// for `deploy-media`). No target answers the one open question, and only
/// when exactly one is open: with two waiting, picking one is the same
/// guess an expired question refuses to make. Every refusal lists what is
/// waiting, so the next try can name it.
pub fn pick_open<'a>(
    open: &'a [OpenQuestion],
    target: Option<&str>,
) -> Result<&'a OpenQuestion, String> {
    let listed = |qs: &[&OpenQuestion]| {
        qs.iter()
            .map(|q| format!("\n  {}", q.line()))
            .collect::<String>()
    };
    let all: Vec<&OpenQuestion> = open.iter().collect();
    if all.is_empty() {
        return Err(
            "no question is waiting for an answer: it was answered, or its wait ran out \
             (an unanswered question fails its operation)"
                .into(),
        );
    }
    let Some(target) = target.map(str::trim).filter(|t| !t.is_empty()) else {
        return match all.as_slice() {
            [only] => Ok(only),
            _ => Err(format!(
                "{} questions are waiting; name one by its operation or id:{}",
                all.len(),
                listed(&all)
            )),
        };
    };
    let suffix = format!("-{}", target);
    let hits: Vec<&OpenQuestion> = all
        .iter()
        .copied()
        .filter(|q| q.id.to_string() == target || q.op == target || q.op.ends_with(&suffix))
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "no waiting question matches '{}'; waiting now:{}",
            target,
            listed(&all)
        )),
        _ => Err(format!(
            "'{}' matches {} waiting questions; name one by its id:{}",
            target,
            hits.len(),
            listed(&hits)
        )),
    }
}
