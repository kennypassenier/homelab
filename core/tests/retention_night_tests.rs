//! fix-42: the tiered retention applied night after night.
//!
//! The expert panel (backup reviewer, 2026-09-27) found that the tiers only
//! ever kept about eight days: buckets were aligned to "now", so every night
//! the snapshot that had just turned eight days old became the newest in the
//! first 14-day bucket and pushed the older one out. Measured live the same
//! day: no repository held a snapshot older than 16 days (paperless-config:
//! 09-17, 09-22, 09-27). Every earlier test applied the policy once.

use homelab_core::retention::{default_tiers, forget_list};

const DAY: u64 = 86_400;

/// One backup every night for `nights` nights, retention run after each.
fn simulate(nights: u64) -> Vec<u64> {
    let start = 1_780_000_000u64;
    let mut snaps: Vec<(String, u64)> = Vec::new();
    for n in 0..nights {
        let now = start + n * DAY;
        snaps.push((format!("s{n}"), now));
        let forget = forget_list(&snaps, &default_tiers(), now);
        snaps.retain(|(id, _)| !forget.contains(id));
    }
    let now = start + (nights - 1) * DAY;
    let mut ages: Vec<u64> = snaps.iter().map(|(_, t)| (now - t) / DAY).collect();
    ages.sort();
    ages
}

#[test]
fn fix_42_a_year_of_nightly_retention_keeps_the_older_tiers() {
    let ages = simulate(400);
    // Daily for a week: ages 0..=6 all present.
    for d in 0..7 {
        assert!(ages.contains(&d), "daily {d} missing: {ages:?}");
    }
    // Something between two weeks and two months old (the 14-day tier).
    assert!(
        ages.iter().any(|a| (14..67).contains(a)),
        "no fortnightly snapshot: {ages:?}"
    );
    // And the 60-day tier reaches back most of the year.
    assert!(
        ages.iter().any(|a| *a >= 300),
        "nothing older than 300 days after 400 nights: {ages:?}"
    );
    assert!(ages.len() <= 20, "too many kept: {ages:?}");
}

#[test]
fn fix_42_the_kept_older_snapshots_stay_put_from_night_to_night() {
    // What survives into the 14-day tier must not be replaced every night.
    let a = simulate(120);
    let b = simulate(121);
    let old_a: Vec<u64> = a.iter().copied().filter(|x| *x >= 20).collect();
    let old_b: Vec<u64> = b
        .iter()
        .copied()
        .filter(|x| *x >= 21)
        .map(|x| x - 1)
        .collect();
    let common = old_a.iter().filter(|x| old_b.contains(x)).count();
    assert!(
        common + 1 >= old_a.len(),
        "older snapshots churned overnight: {a:?} -> {b:?}"
    );
}

#[test]
fn fix_42_a_backup_a_few_minutes_earlier_than_yesterday_keeps_every_daily() {
    // Live on 2026-09-27: the nightly ran at 04:14-04:16 local, so one night's
    // snapshot was often less than 24 hours old when the next run looked at
    // it. Counted in whole days from `now` it was "0 days old", shared bucket
    // 0 with the new one and was forgotten: HDD2TB/paperless_media held
    // 09-22, 09-26 and 09-27 but not 09-23 to 09-25.
    let start = 1_780_000_000u64;
    let mut snaps: Vec<(String, u64)> = Vec::new();
    let at = |n: u64| start + n * DAY + if n.is_multiple_of(2) { 0 } else { 180 };
    for n in 0..60u64 {
        // Alternate a few minutes earlier and later than the day before.
        let now = at(n);
        snaps.push((format!("s{n}"), now));
        let forget = forget_list(&snaps, &default_tiers(), now);
        snaps.retain(|(id, _)| !forget.contains(id));
    }
    let now = at(59);
    let mut ages: Vec<u64> = snaps.iter().map(|(_, t)| (now - t) / DAY).collect();
    ages.sort();
    for d in 0..7 {
        assert!(ages.contains(&d), "daily {d} missing: {ages:?}");
    }
}
