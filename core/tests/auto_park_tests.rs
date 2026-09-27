//! gap-22: the automatic-park notice says what the automatic park does.

use homelab_core::ops::enable::AUTO_PARK_NOTICE;

#[test]
fn gap_22_the_auto_park_notice_does_not_claim_onboot_was_cleared() {
    assert!(
        !AUTO_PARK_NOTICE.contains("no onboot"),
        "{AUTO_PARK_NOTICE}"
    );
    assert!(AUTO_PARK_NOTICE.contains("onboot and the running containers are left"));
    // And the host sends this text, not a copy of its own.
    let host = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../host/src/main.rs"),
    )
    .unwrap();
    assert!(!host.contains("no onboot until re-enabled"));
    // fix-59: both nightly branches now park through one helper, so the
    // notice is named once.
    assert!(host.matches("ops::enable::AUTO_PARK_NOTICE").count() >= 1);
}

fn one_stack() -> homelab_core::state::HostState {
    let mut st = homelab_core::state::HostState::default();
    st.stacks.insert(
        "media".into(),
        homelab_core::state::StackState {
            applied_source: None,
            vmid: 106,
            hostname: "106-app-media".into(),
            apps: Vec::new(),
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: None,
            enabled: true,
            native: None,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
        },
    );
    st
}

/// fix-59 (failed-update-parks-backups, 2026-09-27): one bad upstream image,
/// or one Drive 5xx at the backup hour, set `enabled = false`, and a disabled
/// stack gets no nightly backup until somebody types `homelab enable`. During
/// a week away that is a week without backups of the media configuration.
#[test]
fn fix_59_a_failed_night_never_stops_the_stacks_backups() {
    use homelab_core::ops::enable::after_night;

    // A failed update: updates are parked, the backups keep running.
    let mut st = one_stack();
    assert!(after_night(&mut st, "media", false, 1_000));
    assert!(
        st.stacks["media"].enabled,
        "a failed update must not take the stack out of the nightly backups"
    );
    assert_eq!(st.updates_parked.get("media"), Some(&1_000));
    // One notice, not one a night: the second failure changes nothing.
    assert!(!after_night(&mut st, "media", false, 2_000));
    assert_eq!(st.updates_parked.get("media"), Some(&1_000));

    // A failed backup with a good update: nothing is parked, the backup is
    // simply tried again tomorrow.
    let mut st = one_stack();
    assert!(!after_night(&mut st, "media", true, 1_000));
    assert!(
        st.stacks["media"].enabled,
        "a failed backup must be retried the next night, not parked"
    );
    assert!(st.updates_parked.is_empty());

    // The fleet check says so, since nothing else would.
    let mut st = one_stack();
    after_night(&mut st, "media", false, 1_000);
    let findings = homelab_core::ops::fleetcheck::evaluate(
        &st,
        &homelab_core::ops::fleetcheck::LiveFacts::default(),
        2_000,
        homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S,
        homelab_core::ops::fleetcheck::GrowthLimits::default(),
    );
    assert!(
        findings
            .iter()
            .any(|f| f.subject == "media" && f.what.contains("automatic updates parked")),
        "{:?}",
        findings
    );
}
