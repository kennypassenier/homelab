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
