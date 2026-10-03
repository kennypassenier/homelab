//! fix-240 (2026-10-02): a question a running operation raised expired
//! after 120 s before the person it was for had seen it. A headless run
//! cannot answer; the dashboard banner was the only place it showed, and
//! nothing told anybody to look there.
//!
//! These pin the pure halves: the wait fits a person, an answer from outside
//! the asking session finds the right question (and refuses to guess), and
//! a waiting question becomes an urgent notice that says how to answer it.

use homelab_core::ask::{DEFAULT_WAIT_S, OpenQuestion, Question, pick_open, wait_words};
use homelab_core::notify::{Event, explain_question, urgency};

fn open(id: u64, op: &str) -> OpenQuestion {
    OpenQuestion {
        id,
        op: op.into(),
        step: "service checks".into(),
        what: "a reading went down".into(),
    }
}

/// covers: fix-240
///
/// 120 s ran out before Kenny saw the banner. The default is several
/// minutes, and says so in a person's words.
#[test]
fn fix_240_the_default_wait_fits_a_person() {
    // Read through a binding so the check is on the value, not a constant
    // expression clippy folds away.
    let default = std::hint::black_box(DEFAULT_WAIT_S);
    assert!(
        default >= 300,
        "a question waits {} s by default; a person needs minutes to be told and decide",
        default
    );
    assert_eq!(wait_words(600), "10 min");
    assert_eq!(wait_words(150), "2 min 30 s");
    assert_eq!(wait_words(45), "45 s");
}

/// covers: fix-240
///
/// `homelab answer <op> allow|stop` names the operation, its subject or the
/// question's id, from a session that did not ask.
#[test]
fn fix_240_an_answer_finds_its_question_by_operation_subject_or_id() {
    let qs = vec![open(3, "deploy-alpha"), open(7, "deploy-beta")];
    assert_eq!(pick_open(&qs, Some("deploy-beta")).unwrap().id, 7);
    assert_eq!(pick_open(&qs, Some("alpha")).unwrap().id, 3);
    assert_eq!(pick_open(&qs, Some("7")).unwrap().id, 7);
    let one = vec![open(4, "deploy-alpha")];
    assert_eq!(pick_open(&one, None).unwrap().id, 4);
}

/// covers: fix-240
///
/// Two questions open and no name, a name that matches nothing, or nothing
/// open at all: refused, and the refusal lists what is waiting. Guessing
/// which question was meant is the guess an expired question refuses too.
#[test]
fn fix_240_an_answer_that_could_mean_two_questions_is_refused_not_guessed() {
    let qs = vec![open(3, "deploy-alpha"), open(7, "deploy-beta")];
    let two = pick_open(&qs, None).unwrap_err();
    assert!(
        two.contains("deploy-alpha") && two.contains("deploy-beta"),
        "{two}"
    );
    let none = pick_open(&qs, Some("gamma")).unwrap_err();
    assert!(none.contains("deploy-alpha"), "{none}");
    assert!(pick_open(&[], Some("alpha")).is_err());
    let shared = vec![open(3, "deploy-alpha"), open(8, "backup-alpha")];
    let ambiguous = pick_open(&shared, Some("alpha")).unwrap_err();
    assert!(
        ambiguous.contains("question 3") && ambiguous.contains("question 8"),
        "{ambiguous}"
    );
}

/// covers: fix-240
///
/// A waiting question is told to a person: urgent (it reaches the phone),
/// and the notice says what is asked, how long it waits, how to answer
/// from the dashboard or the command line, and that silence fails it.
#[test]
fn fix_240_a_waiting_question_is_an_urgent_notice_that_says_how_to_answer() {
    assert!(urgency(&Event::Question).urgent);
    let q = Question {
        op: "deploy-alpha".into(),
        step: "service checks".into(),
        what: "routes went from 29 to 28".into(),
        if_allowed: "the deploy goes on".into(),
        if_stopped: "the deploy fails here".into(),
    };
    let ex = explain_question(&q, 5, 600);
    assert!(ex.title.contains("deploy-alpha"), "{:?}", ex);
    assert!(ex.what.contains("routes went from 29 to 28"), "{:?}", ex);
    assert!(
        ex.remedy.contains("homelab answer deploy-alpha allow"),
        "{:?}",
        ex
    );
    assert!(ex.remedy.contains("10 min"), "{:?}", ex);
    assert!(ex.consequence.contains("fails"), "{:?}", ex);
    assert!(!ex.page.is_empty());
}
