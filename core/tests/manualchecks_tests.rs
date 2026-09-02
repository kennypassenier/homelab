//! G17 · the questions only a person can answer, and the record of who did.

use homelab_core::ops::fleetcheck::Severity;
use homelab_core::ops::manualchecks::{
    answer, evaluate_manual, id_for, listing, register, render_listing, DEFAULT_ANSWER_MAX_AGE_S,
};
use homelab_core::state::{HostState, StackState};

const DAY: u64 = 86400;

fn state_with_stack(applied_at: u64) -> HostState {
    let mut st = HostState::default();
    st.stacks.insert(
        "media".into(),
        StackState {
            vmid: 106,
            hostname: "106-app-media".into(),
            apps: Vec::new(),
            applied_at,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: None,
            enabled: true,
            native: None,
            natives: Vec::new(),
            incomplete_step: None,
        },
    );
    st
}

fn q(app: &str, text: &str) -> (String, String) {
    (app.into(), text.into())
}

#[test]
fn the_id_is_stable_for_the_same_question_and_different_for_another() {
    let a = id_for(
        "media",
        "jellyfin",
        "does a film look right on the television",
    );
    assert_eq!(
        a,
        id_for(
            "media",
            "jellyfin",
            "does a film look right on the television"
        ),
        "the same question must keep its id across deploys, or every answer expires on redeploy"
    );
    assert_ne!(
        a,
        id_for("media", "plex", "does a film look right on the television")
    );
    assert_ne!(a, id_for("media", "jellyfin", "is the sound in sync"));
    assert_eq!(a.len(), 8, "short enough to type");
}

#[test]
fn registering_twice_does_not_forget_an_answer() {
    let mut st = state_with_stack(100);
    let qs = vec![q("jellyfin", "is the sound in sync")];
    register(&mut st, "media", &qs, 100);
    let id = id_for("media", "jellyfin", "is the sound in sync");
    assert!(answer(&mut st, &id, true, "checked on the tv", 200));

    register(&mut st, "media", &qs, 300);
    let r = &st.manual_checks[&id];
    assert_eq!(
        r.answered_at,
        Some(200),
        "a redeploy must not wipe the answer"
    );
    assert_eq!(r.ok, Some(true));
    assert_eq!(r.note, "checked on the tv");
    assert_eq!(r.registered_at, 100, "nor rewrite when it first appeared");
}

#[test]
fn a_question_removed_from_the_stack_file_disappears_and_others_survive() {
    let mut st = state_with_stack(100);
    register(
        &mut st,
        "media",
        &[
            q("jellyfin", "sound in sync"),
            q("jellyfin", "picture right"),
        ],
        100,
    );
    register(
        &mut st,
        "paperwork",
        &[q("paperless", "did the scan arrive")],
        100,
    );
    assert_eq!(st.manual_checks.len(), 3);

    // Only one of media's two questions is still in the file.
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 200);
    assert_eq!(
        st.manual_checks.len(),
        2,
        "the dropped question goes, or the list becomes a file nobody can shrink"
    );
    assert!(
        st.manual_checks
            .contains_key(&id_for("paperwork", "paperless", "did the scan arrive")),
        "a deploy of media says nothing about paperwork's questions"
    );
}

#[test]
fn answering_an_unknown_id_says_so_rather_than_doing_nothing() {
    let mut st = state_with_stack(100);
    assert!(!answer(&mut st, "deadbeef", true, "", 200));
}

#[test]
fn a_question_nobody_ever_answered_is_a_finding() {
    let mut st = state_with_stack(100);
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let f = evaluate_manual(&st, 200, DEFAULT_ANSWER_MAX_AGE_S);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Drift);
    assert_eq!(f[0].subject, "media");
    assert!(f[0].what.contains("sound in sync"), "{}", f[0].what);
}

#[test]
fn a_no_from_a_person_is_the_strongest_signal_and_stays_until_it_is_a_yes() {
    let mut st = state_with_stack(100);
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    answer(&mut st, &id, false, "half a second late", 200);

    let f = evaluate_manual(&st, 300, DEFAULT_ANSWER_MAX_AGE_S);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Broken);
    assert!(
        f[0].what.contains("half a second late"),
        "the note is the useful half: {}",
        f[0].what
    );

    answer(&mut st, &id, true, "", 400);
    assert!(
        evaluate_manual(&st, 500, DEFAULT_ANSWER_MAX_AGE_S).is_empty(),
        "and it clears when somebody says it is right"
    );
}

#[test]
fn an_answer_older_than_the_deploy_that_followed_it_is_asked_again() {
    let mut st = state_with_stack(1_000);
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    // Answered at 500, deploy applied at 1000: the deploy may have broken the
    // very thing the question is about.
    answer(&mut st, &id, true, "", 500);

    let f = evaluate_manual(&st, 1_100, DEFAULT_ANSWER_MAX_AGE_S);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(f[0].what.contains("1 check(s)"), "{}", f[0].what);
}

#[test]
fn an_answer_going_stale_is_noted_not_shouted() {
    let mut st = state_with_stack(100);
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    let answered = 10 * DAY;
    answer(&mut st, &id, true, "", answered);

    assert!(
        evaluate_manual(&st, answered + 89 * DAY, DEFAULT_ANSWER_MAX_AGE_S).is_empty(),
        "89 days is still good"
    );
    let f = evaluate_manual(&st, answered + 91 * DAY, DEFAULT_ANSWER_MAX_AGE_S);
    assert_eq!(f.len(), 1);
    assert_eq!(
        f[0].severity,
        Severity::Noted,
        "nothing is wrong; it is time to look again"
    );
}

#[test]
fn the_listing_names_the_id_the_status_and_the_question() {
    let mut st = state_with_stack(100);
    register(
        &mut st,
        "media",
        &[
            q("jellyfin", "sound in sync"),
            q("sonarr", "did tonight's episode arrive"),
        ],
        100,
    );
    let id = id_for("media", "jellyfin", "sound in sync");
    answer(&mut st, &id, true, "", 10 * DAY);

    let out = render_listing(&listing(&st), 12 * DAY);
    assert!(out.contains(&id), "the id is what you type back: {}", out);
    assert!(out.contains("sound in sync"), "{}", out);
    assert!(out.contains("did tonight's episode arrive"), "{}", out);
    assert!(out.contains("unanswered"), "{}", out);
    assert!(out.contains("ok, 2d ago"), "{}", out);
    assert!(
        out.contains("1 answered ok, 1 open"),
        "the count is the point of a list: {}",
        out
    );
    assert!(
        out.contains("homelab checks answer"),
        "and it must say how to answer, or it is another page nobody acts on: {}",
        out
    );
}

#[test]
fn an_empty_register_says_so_instead_of_printing_nothing() {
    let st = HostState::default();
    let out = render_listing(&listing(&st), 0);
    assert!(out.contains("no manual checks"), "{}", out);
}

/// The whole reason for aggregating: 94 findings a night is a wall nobody
/// reads, which is the failure this gap exists to fix.
#[test]
fn many_open_questions_on_one_stack_are_one_line_with_a_count_and_examples() {
    let mut st = state_with_stack(100);
    let qs: Vec<(String, String)> = (0..12)
        .map(|i| ("jellyfin".to_string(), format!("question number {:02}", i)))
        .collect();
    register(&mut st, "media", &qs, 100);
    register(
        &mut st,
        "paperwork",
        &[q("paperless", "did the scan arrive")],
        100,
    );

    let f = evaluate_manual(&st, 200, DEFAULT_ANSWER_MAX_AGE_S);
    assert_eq!(
        f.len(),
        2,
        "one line per stack, not one per question: {:?}",
        f
    );
    let media = f.iter().find(|x| x.subject == "media").unwrap();
    assert!(media.what.contains("12 check(s)"), "{}", media.what);
    assert!(
        media.what.contains("question number 00") && media.what.contains("and 10 more"),
        "a count with no example says nothing you can act on: {}",
        media.what
    );
    assert!(
        media.remedy.contains("homelab checks"),
        "and it must say how to answer"
    );
}

/// A person's "no" is never folded into a count — it is the one signal worth
/// its own line.
#[test]
fn a_no_keeps_its_own_line_even_among_many_open_ones() {
    let mut st = state_with_stack(100);
    let mut qs: Vec<(String, String)> = (0..5)
        .map(|i| ("jellyfin".to_string(), format!("question {}", i)))
        .collect();
    qs.push(q("jellyfin", "is the sound in sync"));
    register(&mut st, "media", &qs, 100);
    let id = id_for("media", "jellyfin", "is the sound in sync");
    answer(&mut st, &id, false, "half a second late", 200);

    let f = evaluate_manual(&st, 300, DEFAULT_ANSWER_MAX_AGE_S);
    let broken: Vec<_> = f
        .iter()
        .filter(|x| x.severity == Severity::Broken)
        .collect();
    assert_eq!(broken.len(), 1, "{:?}", f);
    assert!(
        broken[0].what.contains("half a second late"),
        "{}",
        broken[0].what
    );
    assert_eq!(broken[0].subject, "media/jellyfin", "and it names the app");
}
