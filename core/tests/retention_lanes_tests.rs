//! fix-238 (2026-10-02, Jellyfin 12 upgrade): a manual `homelab backup` at
//! 22:05 made that night's nightly snapshot disappear from every stack backed
//! up by hand, because retention kept one snapshot per day and the manual one
//! was newer. A snapshot taken on demand must never push a scheduled one out.

use homelab_core::ops::backup::retention_doomed;
use homelab_core::retention::default_tiers;

fn listing(rows: &[(&str, &str, &[&str])]) -> String {
    let v: Vec<serde_json::Value> = rows
        .iter()
        .map(|(id, time, tags)| serde_json::json!({"short_id": id, "time": time, "tags": tags}))
        .collect();
    serde_json::to_string(&v).unwrap()
}

/// 2026-10-02 22:30 Europe/Brussels.
const NOW: u64 = 1_790_973_000;

#[test]
fn fix_238_a_manual_backup_never_displaces_the_same_days_nightly() {
    let raw = listing(&[
        (
            "80200a7f",
            "2026-10-01T04:07:00+02:00",
            &["run-1790820418", "trigger:nightly"],
        ),
        (
            "78e2cae5",
            "2026-10-02T04:06:00+02:00",
            &["run-1790906718", "trigger:nightly"],
        ),
        (
            "af364ed7",
            "2026-10-02T22:05:00+02:00",
            &["run-1790971494", "trigger:manual"],
        ),
    ]);
    let doomed = retention_doomed(&raw, &default_tiers(), NOW);
    assert!(
        !doomed.contains(&"78e2cae5".to_string()),
        "the nightly of 2026-10-02 was forgotten because a manual snapshot followed it: {doomed:?}"
    );
    assert!(doomed.is_empty(), "nothing is due yet: {doomed:?}");
}

#[test]
fn fix_238_a_snapshot_without_a_trigger_tag_counts_as_scheduled() {
    // Snapshots from before fix-223 carry no trigger tag: they were nightly
    // runs, and must keep competing with each other exactly as before.
    let raw = listing(&[
        ("aaaa0001", "2026-09-30T02:00:00+02:00", &["run-1"]),
        ("aaaa0002", "2026-09-30T03:00:00+02:00", &["run-2"]),
    ]);
    let doomed = retention_doomed(&raw, &default_tiers(), NOW);
    assert_eq!(doomed, vec!["aaaa0001".to_string()]);
}

#[test]
fn fix_238_manual_snapshots_are_thinned_among_themselves() {
    // Two manual backups on one day: one survives, and the nightly stays.
    let raw = listing(&[
        (
            "bbbb0001",
            "2026-10-02T04:00:00+02:00",
            &["trigger:nightly"],
        ),
        ("bbbb0002", "2026-10-02T19:00:00+02:00", &["trigger:manual"]),
        ("bbbb0003", "2026-10-02T21:00:00+02:00", &["trigger:manual"]),
    ]);
    let doomed = retention_doomed(&raw, &default_tiers(), NOW);
    assert_eq!(doomed, vec!["bbbb0002".to_string()]);
}
