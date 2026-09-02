//! G14 · the recurring restore drill, and the rule that a drill which can be
//! satisfied by empty files rehearses nothing.

use homelab_core::ops::fleetcheck::Severity;
use homelab_core::ops::restoredrill::{
    due, evaluate_drill, next_repo, verdict, Outcome, DEFAULT_DRILL_INTERVAL_S,
};
use homelab_core::state::HostState;

const DAY: u64 = 86400;

#[test]
fn a_drill_that_has_never_run_is_always_due() {
    assert!(due(0, 0, DEFAULT_DRILL_INTERVAL_S));
    assert!(due(0, 10 * DAY, DEFAULT_DRILL_INTERVAL_S));
}

#[test]
fn a_passed_drill_counts_for_the_configured_window_and_not_a_day_longer() {
    let last = 100 * DAY;
    assert!(!due(last, last + 89 * DAY, DEFAULT_DRILL_INTERVAL_S));
    assert!(due(last, last + 90 * DAY, DEFAULT_DRILL_INTERVAL_S));
    // The window is Kenny's, not the author's.
    assert!(due(last, last + 8 * DAY, 7 * DAY));
}

#[test]
fn the_turn_goes_round_so_a_year_covers_every_repository() {
    let repos: Vec<String> = ["jellyfin", "actual", "grafana"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Sorted, so the order does not depend on how the state map happened to
    // iterate on this host.
    let (a, next) = next_repo(&repos, 0).unwrap();
    assert_eq!(a, "actual");
    let (b, next) = next_repo(&repos, next).unwrap();
    assert_eq!(b, "grafana");
    let (c, next) = next_repo(&repos, next).unwrap();
    assert_eq!(c, "jellyfin");
    assert_eq!(next, 0, "and then it starts again");
}

#[test]
fn a_repeated_repository_is_drilled_once_not_twice() {
    let repos: Vec<String> = ["promtail", "promtail", "grafana"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let (a, next) = next_repo(&repos, 0).unwrap();
    let (b, next2) = next_repo(&repos, next).unwrap();
    assert_eq!((a.as_str(), b.as_str()), ("grafana", "promtail"));
    assert_eq!(next2, 0, "two entries, not three");
}

#[test]
fn a_host_with_nothing_to_drill_is_not_an_error() {
    assert!(next_repo(&[], 0).is_none());
}

/// The whole reason this module judges rather than trusts an exit code.
#[test]
fn a_restore_of_only_empty_files_fails_the_drill() {
    assert_eq!(
        verdict(1, 0),
        Outcome::Failed(
            "1 file(s) came back and every one of them is empty — a restore of zero-byte \
             files proves nothing about whether the data is recoverable"
                .into()
        )
    );
    assert!(matches!(verdict(14, 0), Outcome::Failed(_)));
    assert!(matches!(verdict(0, 0), Outcome::Failed(_)));
}

#[test]
fn a_restore_with_content_in_it_passes_and_says_how_much() {
    assert_eq!(
        verdict(14, 220_684),
        Outcome::Passed {
            files: 14,
            largest_bytes: 220_684
        }
    );
}

#[test]
fn a_failed_drill_stays_a_finding_until_one_succeeds() {
    let st = HostState {
        last_restore_drill: 100 * DAY,
        last_restore_drill_repo: "jellyfin".into(),
        last_restore_drill_error: Some("every one of them is empty".into()),
        ..Default::default()
    };
    let f = evaluate_drill(&st, 100 * DAY + 3600, DEFAULT_DRILL_INTERVAL_S);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Broken);
    assert!(f[0].subject.contains("jellyfin"), "{}", f[0].subject);
    assert!(f[0].what.contains("every one of them is empty"));
}

#[test]
fn a_drill_that_never_ran_says_never_rather_than_a_misleading_zero() {
    let st = HostState::default();
    let f = evaluate_drill(&st, 500 * DAY, DEFAULT_DRILL_INTERVAL_S);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(f[0].what.contains("never"), "{}", f[0].what);
}

#[test]
fn a_recent_passing_drill_is_not_a_finding() {
    let st = HostState {
        last_restore_drill: 100 * DAY,
        ..Default::default()
    };
    assert!(evaluate_drill(&st, 101 * DAY, DEFAULT_DRILL_INTERVAL_S).is_empty());
}
