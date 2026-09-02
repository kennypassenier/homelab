//! J1-J3: the before/after pair, and why there are no thresholds in it.

use homelab_core::checks::*;

fn r(name: &str, before: &str, after: &str, expect: Expect) -> Reading {
    Reading {
        name: name.into(),
        before: before.into(),
        after: after.into(),
        expect,
        blind_spot: None,
    }
}

/// Kenny's objection, as a test. He asked what happens when he deletes half
/// his films himself, and a fixed floor of 900 would have alarmed about his
/// own housekeeping. A pair taken minutes apart cannot: 896 before and 898
/// after is a rise, whatever the absolute number is.
#[test]
fn j1_a_count_may_rise_and_must_not_fall() {
    assert_eq!(
        judge(&r("films", "896", "898", Expect::NeverDecreases)),
        Verdict::Ok
    );
    assert_eq!(
        judge(&r("films", "896", "896", Expect::NeverDecreases)),
        Verdict::Ok
    );
    match judge(&r("films", "896", "12", Expect::NeverDecreases)) {
        Verdict::Regressed(why) => assert!(
            why.contains("896") && why.contains("12"),
            "the message must carry both readings: {}",
            why
        ),
        other => panic!("a collapse must be a regression, got {:?}", other),
    }
}

/// The media rebuild is the case: the downloader was importing throughout, so
/// "equal" would have failed a healthy rebuild and "at least N" would have
/// been a guess about a library that changes hourly.
#[test]
fn j1_the_pair_needs_no_tolerance_because_it_is_taken_minutes_apart() {
    // Exactly what was measured on CT 106, before and after.
    for (name, before, after) in [
        ("films", "896", "898"),
        ("series", "203", "203"),
        ("afleveringen", "5462", "5462"),
        ("collections", "58", "58"),
    ] {
        assert_eq!(
            judge(&r(name, before, after, Expect::NeverDecreases)),
            Verdict::Ok,
            "{} {}→{} is a healthy rebuild",
            name,
            before,
            after
        );
    }
}

/// Configuration is not content: a library path or a transcoding device that
/// changed is a fault even though nothing got smaller.
#[test]
fn must_match_catches_a_changed_setting() {
    assert_eq!(
        judge(&r("hardware", "vaapi", "vaapi", Expect::MustMatch)),
        Verdict::Ok
    );
    match judge(&r("hardware", "vaapi", "none", Expect::MustMatch)) {
        Verdict::Regressed(why) => assert!(why.contains("vaapi") && why.contains("none")),
        other => panic!("a changed setting must regress, got {:?}", other),
    }
}

/// A command that will not run says nothing about the data. Reporting that as
/// a regression is how a check earns a reputation for crying wolf — and a
/// check nobody believes is a check nobody keeps.
#[test]
fn an_unreadable_measurement_is_not_a_regression() {
    match judge(&r("films", "896", "", Expect::NeverDecreases)) {
        Verdict::Unreadable(why) => assert!(why.contains("films")),
        other => panic!("empty after must be unreadable, got {:?}", other),
    }
    assert!(
        regressions(&[judge(&r("films", "896", "", Expect::NeverDecreases))]).is_empty(),
        "unreadable must not block"
    );
}

/// A fresh container has no before. That is not a failure — there is simply
/// nothing to compare against yet.
#[test]
fn a_first_deploy_has_nothing_to_compare_and_passes() {
    assert_eq!(
        judge(&r("films", "", "898", Expect::NeverDecreases)),
        Verdict::Ok
    );
}

/// The blind spots come back even when everything passed, because that is
/// exactly the moment nobody asks what was not checked. The restore drill's
/// most useful line was "this does not prove Jellyfin's application layer
/// comes up clean", and it was written next to a green result.
#[test]
fn blind_spots_are_reported_with_a_green_result() {
    let readings = vec![Reading {
        name: "films".into(),
        before: "896".into(),
        after: "898".into(),
        expect: Expect::NeverDecreases,
        blind_spot: Some("says nothing about whether playback looks right".into()),
    }];
    let (verdicts, blind) = judge_all(&readings);
    assert!(regressions(&verdicts).is_empty(), "this reading is healthy");
    assert_eq!(
        blind.len(),
        1,
        "and its blind spot must still be reported: {:?}",
        blind
    );
}

/// Kenny's challenge, as a rule rather than an intention.
///
/// He asked why "does this measure the right thing" had to be discipline
/// instead of automation. Two thirds of it did not: once a check declares
/// which layer it reaches, code can refuse the shapes that actually caused
/// today's faults.
#[test]
fn a_service_whose_deepest_check_is_a_port_is_refused() {
    let sc = ServiceChecks {
        checks: vec![Check {
            name: "poort antwoordt".into(),
            command: "curl -s -o /dev/null -w %{http_code} http://127.0.0.1:8096/".into(),
            expect: Expect::MustBePresent,
            layer: Layer::Network,
            blind_spot: Some("says nothing about the library".into()),
        }],
        manual: vec![],
    };
    let missing = shortcomings(&sc);
    assert!(
        missing.iter().any(|m| m.contains("application")),
        "the registry cache answered a port in 0.7ms and could not serve a \
         byte — a service measured only at that depth is not measured: {:?}",
        missing
    );
}

/// And a shallow check must say what it does not prove, because that is
/// exactly the one that gets mistaken for proof. Nobody reads "films: 935" as
/// evidence the network is fine; plenty of people read "port answered" as
/// evidence the service is.
#[test]
fn a_shallow_check_must_declare_its_blind_spot() {
    let mut sc = ServiceChecks {
        checks: vec![
            Check {
                name: "poort antwoordt".into(),
                command: "true".into(),
                expect: Expect::MustBePresent,
                layer: Layer::Network,
                blind_spot: None,
            },
            Check {
                name: "films".into(),
                command: "true".into(),
                expect: Expect::NeverDecreases,
                layer: Layer::Application,
                blind_spot: None,
            },
        ],
        manual: vec![],
    };
    let missing = shortcomings(&sc);
    assert_eq!(
        missing.len(),
        1,
        "only the shallow one owes an explanation: {:?}",
        missing
    );
    assert!(missing[0].contains("poort antwoordt"));

    sc.checks[0].blind_spot = Some("only that something is listening".into());
    assert!(
        shortcomings(&sc).is_empty(),
        "with the blind spot declared, both rules are satisfied"
    );
}

/// A service with no checks at all is not judged. Saying nothing is a
/// different thing from saying something shallow, and treating them the same
/// would push people to write one meaningless check to silence the rule.
#[test]
fn a_service_without_checks_is_not_nagged() {
    assert!(shortcomings(&ServiceChecks::default()).is_empty());
}

// ── T69: a step can stop and ask ───────────────────────────────────────────

/// The answer that matters most is the one nobody gives.
///
/// The same operations run unattended: the nightly round at 04:00 has no
/// client. A question asked into an empty room must not hang the night, and
/// must not quietly pass either — so `Unattended` is its own answer, and
/// only a person saying yes lets anything continue.
#[tokio::test]
async fn a_question_nobody_answers_never_reads_as_permission() {
    use homelab_core::ask::{Answer, Asker, Question, NOBODY};

    let q = Question {
        op: "deploy-gateway".into(),
        step: "service checks".into(),
        what: "routes went 29 → 28".into(),
        if_allowed: "the deploy continues and records the new count".into(),
        if_stopped: "the deploy fails and bundles an incident".into(),
    };
    let a = NOBODY.ask(&q).await;

    assert!(!a.may_continue(), "silence is never permission");
    // And it is not the same as somebody saying stop. A transcript that
    // cannot tell "Kenny stopped this" from "this ran at 04:00 and nobody
    // was there" has lost the only fact worth reading afterwards.
    assert_ne!(a, Answer::Stop);
    match a {
        Answer::Unattended(why) => assert!(
            !why.is_empty(),
            "an unattended answer carries its reason, or the transcript says \
             nothing about why the operation went the way it did"
        ),
        other => panic!("expected Unattended, got {:?}", other),
    }

    // Only an explicit yes continues.
    assert!(Answer::Allow.may_continue());
    assert!(!Answer::Stop.may_continue());
    assert!(!Answer::Unattended("x".into()).may_continue());
}

/// T69, the half that matters: the mechanism is actually wired to the case
/// Kenny named.
///
/// **What this test does and does not do** (corrected 2026-09-02, F209). It
/// checks the three answers in isolation: yes continues, no does not, and
/// silence is not a yes. That last one is the one that matters at 04:00.
///
/// It does NOT drive a deploy, and its comment used to claim it did — which
/// meant the `Answer::Allow` branch of the real deploy was executed by no
/// test at all, while a reader of this file would have concluded otherwise.
/// The deploy-driving twins live in `m4_ops_tests.rs`:
/// `t69_a_regressed_check_fails_the_deploy_when_nobody_is_watching` and
/// `t69_an_operator_who_says_yes_lets_the_deploy_finish`.
///
/// covers: F156
#[tokio::test]
async fn a_deliberate_drop_can_be_allowed_but_never_by_silence() {
    use homelab_core::ask::{Answer, Asker, Question};

    struct Yes;
    #[async_trait::async_trait]
    impl Asker for Yes {
        async fn ask(&self, _q: &Question) -> Answer {
            Answer::Allow
        }
    }
    struct No;
    #[async_trait::async_trait]
    impl Asker for No {
        async fn ask(&self, _q: &Question) -> Answer {
            Answer::Stop
        }
    }

    // The three answers, on the same question.
    let q = Question {
        op: "deploy-gateway".into(),
        step: "service checks".into(),
        what: "'routes' fell from 29 to 28".into(),
        if_allowed: "continue".into(),
        if_stopped: "fail".into(),
    };
    assert!(
        Yes.ask(&q).await.may_continue(),
        "an explicit yes continues"
    );
    assert!(!No.ask(&q).await.may_continue(), "an explicit no does not");
    assert!(
        !homelab_core::ask::NOBODY.ask(&q).await.may_continue(),
        "and silence is not a yes — this is the one that runs at 04:00"
    );

    // The question a step asks has to be answerable without the transcript:
    // both consequences spelled out, not just two labels (D82).
    assert!(!q.if_allowed.is_empty() && !q.if_stopped.is_empty());
}
