//! G14 · the recurring restore drill, and the rule that a drill which can be
//! satisfied by empty files rehearses nothing.

use homelab_core::ops::fleetcheck::Severity;
use homelab_core::ops::restoredrill::{
    DEFAULT_DRILL_INTERVAL_S, Outcome, due, evaluate_drill, next_repo, verdict,
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
    // fix-62: the default is one night now; the rule itself is unchanged.
    let last = 100 * DAY;
    assert!(!due(last, last + 89 * DAY, 90 * DAY));
    assert!(due(last, last + 90 * DAY, 90 * DAY));
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
        postgres_check_image: None,
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

// ── fix-62: the drill covered almost nothing ────────────────────────────────

use homelab_core::ops::restoredrill::{
    all_drill_repos, pick, postgres_check, record, with_archives, with_postgres_check,
    with_sqlite_checks,
};

fn thirty_repos() -> Vec<String> {
    (0..30).map(|i| format!("repo{:02}", i)).collect()
}

fn passed() -> Outcome {
    Outcome::Passed {
        files: 3,
        largest_bytes: 1000,
    }
}

/// fix-62 (restore-drill-covers-almost-nothing, 2026-09-27): one repository
/// per 90 days over about thirty repositories is one full rotation every
/// 7.4 years. Nights run at the same hour, a few minutes apart; every
/// repository must have come up within a month of them.
#[test]
fn fix_62_every_repository_is_drilled_within_a_month_of_nights() {
    let repos = thirty_repos();
    let mut st = HostState::default();
    let start = 1_800_000_000u64;
    for night in 0..31u64 {
        // A few minutes earlier than the night before, as the scheduler's
        // 20-minute tick allows.
        let now = start + night * DAY - night * 180;
        if !due(st.last_restore_drill, now, DEFAULT_DRILL_INTERVAL_S) {
            continue;
        }
        let repo = pick(&st, &repos).unwrap();
        record(&mut st, &repos, &repo, &passed(), now);
    }
    let drilled: Vec<&String> = repos
        .iter()
        .filter(|r| st.restore_drills.get(*r).is_some_and(|d| d.last_pass > 0))
        .collect();
    assert_eq!(drilled.len(), 30, "drilled: {:?}", drilled);
}

/// fix-62: a failed repository was reported for one night, then the next
/// repository's pass cleared the error and it was not tried again for years.
#[test]
fn fix_62_a_failed_repository_stays_a_finding_after_another_one_passes() {
    let repos: Vec<String> = vec!["paperless".into(), "sonarr".into()];
    let mut st = HostState::default();
    record(
        &mut st,
        &repos,
        "paperless",
        &Outcome::Failed("the restore itself failed: wrong password".into()),
        100 * DAY,
    );
    record(&mut st, &repos, "sonarr", &passed(), 101 * DAY);
    let f = evaluate_drill(&st, 101 * DAY + 3600, DEFAULT_DRILL_INTERVAL_S);
    assert!(
        f.iter().any(|f| f.severity == Severity::Broken
            && f.subject.contains("paperless")
            && f.what.contains("wrong password")),
        "{:?}",
        f
    );
    // And it comes up again before the ones that passed.
    assert_eq!(pick(&st, &repos).as_deref(), Some("paperless"));
    // Until it passes: then the finding goes.
    record(&mut st, &repos, "paperless", &passed(), 102 * DAY);
    assert!(evaluate_drill(&st, 102 * DAY + 3600, DEFAULT_DRILL_INTERVAL_S).is_empty());
}

/// fix-62: the host's own repository (the vault holding the restic password,
/// state.json, TLS) and the device configurations were never drilled.
#[test]
fn fix_62_host_meta_and_device_repositories_are_in_the_rotation() {
    let stacks = vec![(
        vec![mount("/appdata/media/jellyfin-config", Some("jellyfin"))],
        "media".to_string(),
        vec![],
    )];
    let repos = all_drill_repos(&stacks, &["opnsense".to_string()]);
    for want in ["jellyfin", "host-meta", "opnsense"] {
        assert!(
            repos.contains(&want.to_string()),
            "{} missing: {:?}",
            want,
            repos
        );
    }
}

/// fix-62: a native unit's backup is one tar file; a torn one has content
/// and passed the size rule.
#[test]
fn fix_62_an_archive_tar_cannot_read_fails_the_drill() {
    let out = with_archives(passed(), &["/r/kyu-data.tar".to_string()]);
    match out {
        Outcome::Failed(why) => assert!(why.contains("/r/kyu-data.tar"), "{}", why),
        other => panic!("a torn archive passed: {:?}", other),
    }
    assert_eq!(with_archives(passed(), &[]), passed());
}

/// fix-62: a restored SQLite database that fails its own integrity check
/// proves the backup copied a corrupt file, however many files came back
/// and however large the largest one is.
#[test]
fn fix_62_a_restored_sqlite_database_that_fails_integrity_check_fails_the_drill() {
    let bad = [(
        "/r/jellyfin-config/data/jellyfin.db".to_string(),
        "*** in database main *** Page 12 is never used".to_string(),
    )];
    let out = with_sqlite_checks(passed(), &bad);
    match out {
        Outcome::Failed(why) => {
            assert!(why.contains("jellyfin.db"), "{}", why);
            assert!(why.contains("Page 12"), "{}", why);
        }
        other => panic!("a failed integrity check passed: {:?}", other),
    }
    assert_eq!(with_sqlite_checks(passed(), &[]), passed());
    // A restore that already failed stays failed — the sqlite check only
    // ever makes a Passed outcome worse, never a Failed one better.
    let already_failed = Outcome::Failed("the restore itself failed".into());
    assert_eq!(
        with_sqlite_checks(already_failed.clone(), &bad),
        already_failed
    );
}

/// fix-62: which repository needs a throwaway Postgres check, found
/// generically by content (what the mount says about itself), over every
/// stack — not by naming an app in code.
#[test]
fn fix_62_postgres_check_finds_the_declaring_mount_by_owner() {
    use homelab_core::ops::restoredrill::PostgresCheck;
    let mut pg_mount = mount(
        "/appdata/paperwork/paperless-db-config",
        Some("paperless-db"),
    );
    pg_mount.postgres_check_image = Some("postgres:17.11-alpine".into());
    let stacks = vec![
        (
            114u16,
            vec![
                mount("/appdata/paperwork/paperless-config", Some("paperless")),
                pg_mount,
            ],
            "paperwork".to_string(),
        ),
        (
            112,
            vec![mount("/appdata/almanac/almanac-config", None)],
            "almanac".to_string(),
        ),
    ];
    assert_eq!(
        postgres_check(&stacks, "paperless-db"),
        Some(PostgresCheck {
            vmid: 114,
            host_path: "/appdata/paperwork/paperless-db-config".into(),
            image: "postgres:17.11-alpine".into(),
        })
    );
    // An ordinary repository, and a repository that does not exist at all,
    // both answer None — nothing here guesses.
    assert_eq!(postgres_check(&stacks, "paperless"), None);
    assert_eq!(postgres_check(&stacks, "no-such-repo"), None);
}

/// fix-62: a throwaway Postgres container that never became ready proves
/// the restored data is not a usable database, however many files restic
/// restored without error.
#[test]
fn fix_62_a_postgres_container_that_never_becomes_ready_fails_the_drill() {
    let out = with_postgres_check(passed(), Some(false));
    match out {
        Outcome::Failed(why) => assert!(why.contains("ready to accept connections"), "{}", why),
        other => panic!(
            "a Postgres container that never came up passed: {:?}",
            other
        ),
    }
    // Ready, and "could not even run the check" (no postgres_check_image on
    // this repository, or docker was not reachable): both leave a passed
    // drill passed — the second is a gap in what this drill can prove, not
    // a failed drill.
    assert_eq!(with_postgres_check(passed(), Some(true)), passed());
    assert_eq!(with_postgres_check(passed(), None), passed());
    // A restore that already failed stays failed.
    let already_failed = Outcome::Failed("the restore itself failed".into());
    assert_eq!(
        with_postgres_check(already_failed.clone(), Some(false)),
        already_failed
    );
}
