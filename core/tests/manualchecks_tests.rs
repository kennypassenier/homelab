//! G17 · the questions only a person can answer, and the record of who did.

use std::collections::BTreeMap;

use homelab_core::manifest::ComponentDigests;
use homelab_core::ops::fleetcheck::Severity;
use homelab_core::ops::manualchecks::{
    PASSWORD_CHAIN_APP, PASSWORD_CHAIN_RECUR_DAYS, PASSWORD_CHAIN_TEXT, Question, STANDING_STACK,
    answer, ensure_standing, ensure_standing_checks, evaluate_manual, id_for, register,
    render_listing,
};
use homelab_core::state::{HostState, StackState};

const DAY: u64 = 86400;

fn state_with_stack(applied_at: u64) -> HostState {
    let mut st = HostState::default();
    st.stacks.insert(
        "media".into(),
        StackState {
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
            applied_source: None,
            vmid: 106,
            hostname: "106-app-media".into(),
            apps: Vec::new(),
            applied_at,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: None,
            enabled: true,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
        },
    );
    st
}

fn q(app: &str, text: &str) -> Question {
    Question {
        app: app.into(),
        text: text.into(),
        once: false,
        url: None,
        id: None,
        replaces: Vec::new(),
    }
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

/// fix-182: the scenario fix-178 actually hit. A check answered under the
/// old `hash(stack/app/dutch_text)` id must end up under its new, explicit
/// id once `checks.yml` gives it one and names the old hash in `replaces`
/// — the whole point being that a translation (or any other wording fix)
/// no longer throws the answer away.
#[test]
fn a_check_given_an_id_carries_its_answer_over_from_the_replaced_hash() {
    let mut st = state_with_stack(100);

    // The check as it was before fix-178: no id, so it lived under the hash
    // of its (here, stand-in) Dutch text.
    let old_hash = id_for("kp-soft", "jobtracker", "Registreer een passkey …");
    register(
        &mut st,
        "kp-soft",
        &[q("jobtracker", "Registreer een passkey …")],
        100,
    );
    assert!(answer(&mut st, &old_hash, true, "done", 150));
    assert_eq!(st.manual_checks.len(), 1);

    // The translated check, now carrying an explicit id and `replaces`
    // naming that same old hash — exactly what fix-182 adds to checks.yml.
    let new_question = Question {
        app: "jobtracker".into(),
        text: "Register a passkey …".into(),
        once: true,
        url: None,
        id: Some("jobtracker-passkey-registered".into()),
        replaces: vec![old_hash.clone()],
    };
    register(&mut st, "kp-soft", &[new_question], 200);

    assert!(
        !st.manual_checks.contains_key(&old_hash),
        "the old record must not survive the migration under its own id"
    );
    let r = st
        .manual_checks
        .get("jobtracker-passkey-registered")
        .expect("the answer must now sit under the new, explicit id");
    assert_eq!(r.ok, Some(true), "the answer itself carries over");
    assert_eq!(r.answered_at, Some(150), "and when it was given");
    assert_eq!(r.note, "done");
    assert_eq!(
        st.manual_checks.len(),
        1,
        "one check, not two — the old record must not linger: {:?}",
        st.manual_checks.keys().collect::<Vec<_>>()
    );

    // A second deploy after the migration must not re-read the (now gone)
    // old record and must not disturb the carried-over answer.
    let new_question_again = Question {
        app: "jobtracker".into(),
        text: "Register a passkey …".into(),
        once: true,
        url: None,
        id: Some("jobtracker-passkey-registered".into()),
        replaces: vec![old_hash],
    };
    register(&mut st, "kp-soft", &[new_question_again], 300);
    let r = &st.manual_checks["jobtracker-passkey-registered"];
    assert_eq!(r.ok, Some(true));
    assert_eq!(r.answered_at, Some(150), "still the original answer time");
}

/// A check with an explicit `id` and no `replaces` (the normal case for a
/// check that never had answered state, or whose migration already ran)
/// behaves exactly like one with no id at all, just under a nicer name.
#[test]
fn an_id_with_no_replaces_is_just_a_nicer_name() {
    let mut st = state_with_stack(100);
    let question = Question {
        app: "jellyfin".into(),
        text: "is the picture right".into(),
        once: false,
        url: None,
        id: Some("jellyfin-picture-ok".into()),
        replaces: Vec::new(),
    };
    register(&mut st, "media", &[question], 100);
    assert!(st.manual_checks.contains_key("jellyfin-picture-ok"));
    assert!(answer(&mut st, "jellyfin-picture-ok", true, "", 200));
    register(
        &mut st,
        "media",
        &[Question {
            app: "jellyfin".into(),
            text: "is the picture right".into(),
            once: false,
            url: None,
            id: Some("jellyfin-picture-ok".into()),
            replaces: Vec::new(),
        }],
        300,
    );
    assert_eq!(
        st.manual_checks["jellyfin-picture-ok"].answered_at,
        Some(200),
        "a redeploy must not forget the answer, same as the hash-id path"
    );
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
    let f = evaluate_manual(&st, 200);
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

    let f = evaluate_manual(&st, 300);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Broken);
    assert!(
        f[0].what.contains("half a second late"),
        "the note is the useful half: {}",
        f[0].what
    );

    answer(&mut st, &id, true, "", 400);
    assert!(
        evaluate_manual(&st, 500).is_empty(),
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
    // fix-65: that holds for a deploy that changed the stack's files.
    st.stacks.get_mut("media").unwrap().applied_hash = "changed".into();

    let f = evaluate_manual(&st, 1_100);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(f[0].what.contains("1 check(s)"), "{}", f[0].what);
}

/// checks-interval (Kenny, 2026-09-30): an answer has no age limit. Only a
/// deploy that really changes the stack asks again; the 90-day window, which
/// reopened a year of "ok" every quarter, is gone.
#[test]
fn an_answer_does_not_expire_with_time() {
    let mut st = state_with_stack(100);
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    answer(&mut st, &id, true, "", 10 * DAY);
    assert!(
        evaluate_manual(&st, 10 * DAY + 3650 * DAY).is_empty(),
        "ten years on, still answered"
    );
}

/// checks-onetime (Kenny, 2026-09-30): a `once` question answered ok stays
/// answered, even after a deploy that changed the stack; answered not ok,
/// or never, it is open like any other.
#[test]
fn a_once_question_answered_ok_is_answered_for_good() {
    let mut st = state_with_stack(100);
    let mut once = q("jobtracker", "register a passkey");
    once.once = true;
    register(&mut st, "media", &[once.clone()], 100);
    let id = id_for("media", "jobtracker", "register a passkey");
    assert_eq!(evaluate_manual(&st, 200).len(), 1, "unanswered is open");
    answer(&mut st, &id, true, "", 300);
    st.stacks.get_mut("media").unwrap().applied_hash = "changed".into();
    assert!(
        evaluate_manual(&st, 400).is_empty(),
        "a changed stack does not reopen it"
    );
    // The flag follows the stack file: dropped there, the rule is back.
    let mut plain = once;
    plain.once = false;
    register(&mut st, "media", &[plain], 500);
    assert_eq!(evaluate_manual(&st, 600).len(), 1);
}

/// checks-link (Kenny, 2026-09-30): the finding names where the application
/// is opened, so the notification leads straight to it.
#[test]
fn an_open_question_carries_the_address_of_its_application() {
    let mut st = state_with_stack(100);
    let mut with_url = q("sonarr", "recent episode in the right folder");
    with_url.url = Some("https://son.kp-soft.dev".into());
    register(&mut st, "media", &[with_url], 100);
    let f = evaluate_manual(&st, 200);
    assert!(
        f[0].what.contains("(https://son.kp-soft.dev)"),
        "{}",
        f[0].what
    );
    let id = id_for("media", "sonarr", "recent episode in the right folder");
    assert_eq!(
        st.manual_checks[&id].url.as_deref(),
        Some("https://son.kp-soft.dev")
    );
    assert!(render_listing(&st, 200).contains("https://son.kp-soft.dev"));
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

    let out = render_listing(&st, 12 * DAY);
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
    let out = render_listing(&st, 0);
    assert!(out.contains("no manual checks"), "{}", out);
}

/// The whole reason for aggregating: 94 findings a night is a wall nobody
/// reads, which is the failure this gap exists to fix.
#[test]
fn many_open_questions_on_one_stack_are_one_line_with_a_count_and_examples() {
    let mut st = state_with_stack(100);
    let qs: Vec<Question> = (0..12)
        .map(|i| q("jellyfin", &format!("question number {:02}", i)))
        .collect();
    register(&mut st, "media", &qs, 100);
    register(
        &mut st,
        "paperwork",
        &[q("paperless", "did the scan arrive")],
        100,
    );

    let f = evaluate_manual(&st, 200);
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
    let mut qs: Vec<Question> = (0..5)
        .map(|i| q("jellyfin", &format!("question {}", i)))
        .collect();
    qs.push(q("jellyfin", "is the sound in sync"));
    register(&mut st, "media", &qs, 100);
    let id = id_for("media", "jellyfin", "is the sound in sync");
    answer(&mut st, &id, false, "half a second late", 200);

    let f = evaluate_manual(&st, 300);
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

// ── fix-195: an answer is about the files its check is about ───────────────

/// fix-195: today, five stacks' manual checks reopened only because fix-178
/// translated `checks.yml` to English and because the derived manifest's
/// own serialization changed — neither is anything the questions are
/// about. Once the stack carries `component_digests` (fix-192), an answer
/// is reopened only when the check's own app files, the stack's rootfs, or
/// the app's env actually changed; a manifest-only move, or a change to
/// another app's files, must not reopen it.
#[test]
fn fix_195_only_the_files_a_check_is_about_can_reopen_its_answer() {
    let mut st = state_with_stack(100);
    let cd = ComponentDigests {
        manifest: "m1".into(),
        files: BTreeMap::from([
            ("jellyfin/compose.yml".into(), "f1".into()),
            ("rootfs/etc/foo".into(), "r1".into()),
            ("sonarr/compose.yml".into(), "s1".into()),
        ]),
        env: BTreeMap::from([("jellyfin".into(), "e1".into())]),
        secret_files: BTreeMap::new(),
        built_by: String::new(),
    };
    {
        let s = st.stacks.get_mut("media").unwrap();
        s.component_digests = cd;
        s.applied_hash = "h1".into();
    }

    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    assert!(answer(&mut st, &id, true, "", 200));
    assert!(evaluate_manual(&st, 300).is_empty(), "answered ok, silent");

    // Stand-in for fix-178 (translating checks.yml) and fix-192 (the
    // manifest's own serialization moving): the combined `applied_hash`
    // moves and the manifest digest moves, but nothing this question is
    // about changed.
    {
        let s = st.stacks.get_mut("media").unwrap();
        s.component_digests.manifest = "m2".into();
        s.applied_hash = "h2".into();
    }
    assert!(
        evaluate_manual(&st, 300).is_empty(),
        "a manifest-only difference must not reopen an answer about files"
    );

    // Another app's file in the same stack changes — still not what this
    // question is about.
    st.stacks
        .get_mut("media")
        .unwrap()
        .component_digests
        .files
        .insert("sonarr/compose.yml".into(), "s2".into());
    assert!(
        evaluate_manual(&st, 300).is_empty(),
        "another app's file must not reopen this question"
    );

    // This app's own file changes — reopened, and said why.
    st.stacks
        .get_mut("media")
        .unwrap()
        .component_digests
        .files
        .insert("jellyfin/compose.yml".into(), "f2".into());
    let f = evaluate_manual(&st, 300);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(f[0].what.contains("reopened"), "{}", f[0].what);
}

/// fix-195: the rootfs and the app's own env count too, not only the app's
/// named files.
#[test]
fn fix_195_rootfs_and_env_changes_reopen_too() {
    let mut st = state_with_stack(100);
    let cd = ComponentDigests {
        manifest: "m1".into(),
        files: BTreeMap::from([("rootfs/etc/foo".into(), "r1".into())]),
        env: BTreeMap::from([("jellyfin".into(), "e1".into())]),
        secret_files: BTreeMap::new(),
        built_by: String::new(),
    };
    {
        let s = st.stacks.get_mut("media").unwrap();
        s.component_digests = cd;
        s.applied_hash = "h1".into();
    }
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    assert!(answer(&mut st, &id, true, "", 200));
    assert!(evaluate_manual(&st, 300).is_empty());

    st.stacks
        .get_mut("media")
        .unwrap()
        .component_digests
        .env
        .insert("jellyfin".into(), "e2".into());
    assert_eq!(
        evaluate_manual(&st, 300).len(),
        1,
        "the app's own env changed"
    );

    // Answer again, then move the rootfs digest instead.
    assert!(answer(&mut st, &id, true, "", 400));
    st.stacks
        .get_mut("media")
        .unwrap()
        .component_digests
        .files
        .insert("rootfs/etc/foo".into(), "r2".into());
    assert_eq!(evaluate_manual(&st, 500).len(), 1, "rootfs changed");
}

/// fix-195: a record with no `answered_digests` (an answer from before this
/// field existed), or one made while the stack carried no
/// `component_digests` yet, keeps the old `answered_hash` rule — this is
/// the pre-existing `fix_65_*` behaviour, unaffected by fix-195.
#[test]
fn fix_195_an_old_record_without_digests_keeps_the_old_rule() {
    let mut st = state_with_stack(100);
    st.stacks.get_mut("media").unwrap().applied_hash = "h1".into();
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    assert!(answer(&mut st, &id, true, "", 200));
    assert!(
        st.manual_checks[&id].answered_digests.is_none(),
        "no component_digests on the stack: nothing to record"
    );

    // The stack gains component_digests only now (e.g. the first deploy
    // through a host that has fix-192) — the OLD answer still falls back to
    // answered_hash, since it never recorded the new field.
    st.stacks.get_mut("media").unwrap().component_digests = ComponentDigests {
        manifest: "m1".into(),
        ..Default::default()
    };
    assert!(
        evaluate_manual(&st, 300).is_empty(),
        "applied_hash unchanged: still answered under the old rule"
    );
    st.stacks.get_mut("media").unwrap().applied_hash = "h2".into();
    assert_eq!(
        evaluate_manual(&st, 300).len(),
        1,
        "applied_hash moved: the old rule still reopens it"
    );
}

// ── fix-65: the nightly report was red every night by design ───────────────

/// fix-65 (nightly-report-always-red, 2026-09-27): an answer was reopened by
/// any deploy after it, and `applied_at` moves on every deploy, so answers
/// given in the morning were open again by the evening. Only a deploy that
/// changed the stack's files may reopen one, and the finding says so.
#[test]
fn fix_65_a_redeploy_that_changed_nothing_does_not_reopen_an_answer() {
    let mut st = state_with_stack(100);
    st.stacks.get_mut("media").unwrap().applied_hash = "h1".into();
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    answer(&mut st, &id, true, "", 500);

    // Redeployed at 1000 with the same files: the answer stands.
    st.stacks.get_mut("media").unwrap().applied_at = 1_000;
    assert!(
        evaluate_manual(&st, 1_100).is_empty(),
        "{:?}",
        evaluate_manual(&st, 1_100)
    );

    // Redeployed with changed files: asked again, and told why.
    st.stacks.get_mut("media").unwrap().applied_hash = "h2".into();
    let f = evaluate_manual(&st, 1_100);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(f[0].what.contains("files changed"), "{}", f[0].what);
}

/// fix-65: kp-soft.dev's deliberate `nok` (D56) was Broken every night with
/// no way to say "known, and accepted". An accepted answer is Noted until
/// its date, then Broken again.
#[test]
fn fix_65_a_deliberate_nok_can_be_accepted_until_a_date() {
    use homelab_core::ops::manualchecks::accept;
    let mut st = state_with_stack(100);
    register(
        &mut st,
        "media",
        &[q("jellyfin", "reachable from outside")],
        100,
    );
    let id = id_for("media", "jellyfin", "reachable from outside");
    assert!(accept(&mut st, &id, 30 * DAY, "by design, D56", 500));

    let f = evaluate_manual(&st, 10 * DAY);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Noted, "{:?}", f[0]);
    assert!(f[0].what.contains("accepted until"), "{}", f[0].what);
    assert!(f[0].what.contains("by design, D56"), "{}", f[0].what);

    let f = evaluate_manual(&st, 31 * DAY);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Broken, "the acceptance ran out");
}

/// fix-65: the same red report went out every night, so a real new problem
/// arrived in an envelope Kenny had learned to ignore. It goes out when the
/// alarming set changes, and once a week while it stands.
#[test]
fn fix_65_the_nightly_report_goes_out_when_the_set_changes_or_weekly() {
    use homelab_core::ops::fleetcheck::{NIGHTLY_REPORT_REPEAT_S, nightly_report_due};
    let now = 1_800_000_000u64;
    assert!(
        nightly_report_due("Broken|media", "", 0, now, NIGHTLY_REPORT_REPEAT_S),
        "first time"
    );
    assert!(
        !nightly_report_due(
            "Broken|media",
            "Broken|media",
            now - DAY,
            now,
            NIGHTLY_REPORT_REPEAT_S
        ),
        "the same set as last night stays quiet"
    );
    assert!(
        nightly_report_due(
            "Broken|media\nDrift|paperwork",
            "Broken|media",
            now - DAY,
            now,
            NIGHTLY_REPORT_REPEAT_S
        ),
        "something new is sent"
    );
    assert!(
        nightly_report_due(
            "Broken|media",
            "Broken|media",
            now - 7 * DAY,
            now,
            NIGHTLY_REPORT_REPEAT_S
        ),
        "and a standing problem is repeated weekly"
    );
}

/// password-chain-bus-factor (owner decision "Alleen de 90-dagen-controle",
/// 2026-10-01): a standing check — one no deploy ever registers or reopens
/// — is due again `recur_days` after it was last answered, whatever the
/// answer was. This is the opposite of `checks-interval`'s rule for an
/// ordinary per-app check (answered ok and not `once` stays silent until a
/// deploy changes the stack), which is exactly why it needs its own field
/// (`recur_days`) rather than reusing that one's age logic.
#[test]
fn password_chain_bus_factor_a_standing_check_recurs_on_its_own_clock() {
    let now = 2_000_000_000u64;
    let mut st = HostState::default();

    // Registering it twice (two nightly ticks) must not disturb an answer.
    ensure_standing_checks(&mut st, now);
    ensure_standing_checks(&mut st, now + 3600);
    assert_eq!(st.manual_checks.len(), 1, "idempotent: one record, not two");

    let id = id_for(STANDING_STACK, PASSWORD_CHAIN_APP, PASSWORD_CHAIN_TEXT);
    assert!(st.manual_checks.contains_key(&id));

    // Never answered: due, same as any unanswered check — aggregated under
    // STANDING_STACK, which is not a real stack name an app-knowledge scan
    // would ever see deployed.
    let findings = evaluate_manual(&st, now);
    assert!(
        findings
            .iter()
            .any(|f| f.subject == STANDING_STACK && f.severity == Severity::Drift),
        "{:?}",
        findings
    );

    // Answered ok: silent immediately after.
    assert!(answer(&mut st, &id, true, "", now));
    assert!(evaluate_manual(&st, now).is_empty());

    // Just under the recurrence window: still silent.
    let almost = now + (PASSWORD_CHAIN_RECUR_DAYS * 86400) - 1;
    assert!(
        evaluate_manual(&st, almost).is_empty(),
        "not due a second early"
    );

    // At the window: due again, with no deploy involved at all.
    let due = now + PASSWORD_CHAIN_RECUR_DAYS * 86400;
    let findings = evaluate_manual(&st, due);
    assert!(
        findings
            .iter()
            .any(|f| f.subject == STANDING_STACK && f.severity == Severity::Drift),
        "a standing check must become due on its own clock: {:?}",
        findings
    );

    // Answering it again resets the clock.
    assert!(answer(&mut st, &id, true, "", due));
    assert!(evaluate_manual(&st, due).is_empty());
    assert!(evaluate_manual(&st, due + PASSWORD_CHAIN_RECUR_DAYS * 86400 - 1).is_empty());

    // A non-standing check (recur_days unset) is unaffected by this logic —
    // the ordinary "answered ok, no deploy since" rule still applies.
    let mut st2 = state_with_stack(now);
    register(&mut st2, "media", &[q("media", "looks right?")], now);
    let other_id = id_for("media", "media", "looks right?");
    assert!(answer(&mut st2, &other_id, true, "", now));
    assert!(
        evaluate_manual(&st2, now + 10 * PASSWORD_CHAIN_RECUR_DAYS * 86400).is_empty(),
        "an ordinary per-app check must not gain an age limit from this feature"
    );

    // `ensure_standing` itself never clobbers an existing answer (the same
    // idempotence `register` gives a deploy's questions).
    ensure_standing(
        &mut st,
        PASSWORD_CHAIN_APP,
        PASSWORD_CHAIN_TEXT,
        90,
        due + 1,
    );
    assert_eq!(
        st.manual_checks.get(&id).and_then(|r| r.ok),
        Some(true),
        "re-ensuring must not reset the answer"
    );
}

// ── fix-195 round 2: the listing and the report disagreed ──────────────────

/// fix-195 round 2 (live, host 3.70.7, 2026-10-03): `homelab today` said
/// media's answer was "reopened because the stack's files changed since"
/// (jellyfin's compose file did change: the 12.1 upgrade) while `homelab
/// checks` printed the same answer as "ok, 1d ago". The listing must ask the
/// rule the report uses.
///
/// covers: fix-195
/// fail-first: written against the old `render_listing(&listing(&st), now)`
/// first, where it failed on the "ok, 1d ago" assertion (2026-10-03); the
/// fix changed that signature, so the file no longer builds on the old code.
#[test]
fn fix_195_the_listing_shows_an_answer_the_report_reopened() {
    let mut st = state_with_stack(100);
    st.stacks.get_mut("media").unwrap().component_digests = ComponentDigests {
        manifest: "m1".into(),
        files: BTreeMap::from([("jellyfin/docker-compose.yml".into(), "a".into())]),
        ..Default::default()
    };
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    assert!(answer(&mut st, &id, true, "", DAY));
    st.stacks
        .get_mut("media")
        .unwrap()
        .component_digests
        .files
        .insert("jellyfin/docker-compose.yml".into(), "b".into());
    assert_eq!(
        evaluate_manual(&st, 2 * DAY).len(),
        1,
        "the report reopens it"
    );
    let out = render_listing(&st, 2 * DAY);
    assert!(
        !out.contains("ok, 1d ago"),
        "the listing must not call a reopened answer ok: {}",
        out
    );
    assert!(out.contains("reopened"), "{}", out);
    assert!(out.contains("0 answered ok, 1 open"), "{}", out);
}

/// fix-195 round 2: an answer given under the old whole-stack rule (no
/// `answered_digests`) was reopened by the next deploy whatever it changed.
/// At that deploy, while the previous record is still in state, an answer
/// that was still current against it takes that record's digests, so a
/// firewall-only deploy leaves it answered and a change to its app does not.
///
/// covers: fix-195
/// fail-first: `pin_pre_digest_answers` is new; with its body stubbed to
/// return at once (the old behaviour: nothing pinned) this failed on the
/// "a firewall-only deploy reopened it" assertion (2026-10-03).
#[test]
fn fix_195_a_deploy_moves_a_current_old_rule_answer_to_its_digests() {
    use homelab_core::ops::manualchecks::pin_pre_digest_answers;
    let digests = |compose: &str| ComponentDigests {
        manifest: "m1".into(),
        files: BTreeMap::from([("supersync/docker-compose.yml".into(), compose.into())]),
        ..Default::default()
    };
    let mut st = state_with_stack(100);
    {
        let s = st.stacks.get_mut("media").unwrap();
        s.applied_hash = "h1".into();
        s.component_digests = digests("a");
    }
    register(&mut st, "media", &[q("supersync", "sync works")], 100);
    let id = id_for("media", "supersync", "sync works");
    assert!(answer(&mut st, &id, true, "", 200));
    // As an answer recorded by a host from before fix-195 looks.
    st.manual_checks.get_mut(&id).unwrap().answered_digests = None;

    pin_pre_digest_answers(&mut st, "media");
    // The deploy: only the manifest (the firewall) changed.
    {
        let s = st.stacks.get_mut("media").unwrap();
        s.applied_hash = "h2".into();
        s.component_digests = ComponentDigests {
            manifest: "m2".into(),
            ..digests("a")
        };
    }
    assert!(
        evaluate_manual(&st, 300).is_empty(),
        "a firewall-only deploy reopened it"
    );

    // A later deploy that does change the app's compose file reopens it.
    st.stacks.get_mut("media").unwrap().component_digests = digests("b");
    assert_eq!(
        evaluate_manual(&st, 400).len(),
        1,
        "a real change must still reopen it"
    );
}

/// fix-195 round 2: an old-rule answer that was ALREADY stale before the
/// deploy is never re-stamped as current.
///
/// covers: fix-195
/// fail-first: a guard on the new pinning, not on old behaviour; passes with
/// the stub too, which is its point (it must never pin a stale answer).
#[test]
fn fix_195_a_stale_old_rule_answer_is_not_pinned() {
    use homelab_core::ops::manualchecks::pin_pre_digest_answers;
    let mut st = state_with_stack(100);
    st.stacks.get_mut("media").unwrap().applied_hash = "h0".into();
    register(&mut st, "media", &[q("jellyfin", "sound in sync")], 100);
    let id = id_for("media", "jellyfin", "sound in sync");
    assert!(answer(&mut st, &id, true, "", 200));
    {
        let s = st.stacks.get_mut("media").unwrap();
        s.applied_hash = "h1".into();
        s.component_digests = ComponentDigests {
            manifest: "m1".into(),
            ..Default::default()
        };
    }
    pin_pre_digest_answers(&mut st, "media");
    assert!(st.manual_checks[&id].answered_digests.is_none());
}
