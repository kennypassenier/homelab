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

// ── F290: the drill rotated over the wrong list ────────────────────────────

use homelab_core::manifest::MountSpec;
use homelab_core::ops::restoredrill::drill_repos;

fn mount(host_path: &str, app: Option<&str>) -> MountSpec {
    MountSpec {
        host_path: host_path.into(),
        mount_point: host_path.into(),
        no_data: false,
        no_backup: None,
        host_owner_uid: None,
        app: app.map(|s| s.to_string()),
    }
}

/// A native stack has an EMPTY `apps` list by design — its services are in
/// `natives`. The old list came from `apps`, so the four services on CT 109
/// and CT 112 were never rehearsed: exactly the backups that had silently
/// been broken until two days before this was found (F179).
#[test]
fn f290_the_native_services_are_in_the_rotation() {
    let stacks = vec![
        (
            vec![],
            "kyu".to_string(),
            vec![
                "kyu".to_string(),
                "kyu-runner".into(),
                "http-switchboard".into(),
            ],
        ),
        (vec![], "almanac".to_string(), vec!["almanac".to_string()]),
    ];
    let repos = drill_repos(&stacks);
    for want in ["kyu", "kyu-runner", "http-switchboard", "almanac"] {
        assert!(
            repos.contains(&want.to_string()),
            "{} is missing: {:?}",
            want,
            repos
        );
    }
}

/// The other direction: an app that keeps nothing has no repository, and a
/// drill night spent on one proves nothing while looking like a failure.
#[test]
fn f290_an_app_without_a_repository_is_not_in_the_rotation() {
    let mut nothing = mount("/appdata/media/flaresolverr-config", Some("flaresolverr"));
    nothing.no_data = true;
    let mut declared_reproducible = mount("/appdata/registry/registry-config", Some("registry"));
    declared_reproducible.no_backup = Some("a pull-through cache".into());
    let stacks = vec![(
        vec![
            mount("/appdata/media/jellyfin-config", Some("jellyfin")),
            nothing,
            declared_reproducible,
        ],
        "media".to_string(),
        vec![],
    )];
    assert_eq!(drill_repos(&stacks), vec!["jellyfin".to_string()]);
}

/// The owner is what names the repository, not the stack — and an owner two
/// mounts share is one repository, not two.
#[test]
fn f290_the_list_is_owners_deduplicated_not_mounts() {
    let stacks = vec![(
        vec![
            mount("/appdata/kyu/kyu-config", Some("kyu")),
            mount("/appdata/kyu/kyu-extra", Some("kyu")),
            mount("/appdata/home/homepage-config", None),
        ],
        "home".to_string(),
        vec![],
    )];
    assert_eq!(drill_repos(&stacks), vec!["home".to_string(), "kyu".into()]);
}
