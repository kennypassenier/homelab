//! fix-150 (expert panel 2026-09-27, the patch-state half of
//! check-blind-to-repo-drift; Kenny 2026-09-28, form "Vragen bij terugkomst",
//! patch-threshold: "7 dagen"): a container whose updates stand still for a
//! week is a finding. Measured on 2026-09-27 21:15: CT 116 had 40 upgradable
//! packages and CT 113 64, no reboot pending, unattended-upgrades run that
//! morning — nothing said so anywhere.
//!
//! Each test here was written before the code and failed on it first.

use homelab_core::ops::facts::parse_patch_probe;
use homelab_core::ops::fleetcheck::{evaluate_patch_state, PatchFact, Severity, PATCH_THRESHOLD_S};

const NOW: u64 = 1_790_600_000;
const DAY: u64 = 86_400;

fn fact(vmid: u16, upgradable: u32, reboot_age: Option<u64>, stamp_age: Option<u64>) -> PatchFact {
    PatchFact {
        vmid,
        hostname: format!("{}-app-x", vmid),
        upgradable: Some(upgradable),
        reboot_required_age_s: reboot_age,
        unattended_stamp_age_s: stamp_age,
    }
}

/// The threshold is Kenny's: a week.
#[test]
fn fix_150_the_threshold_is_seven_days() {
    assert_eq!(PATCH_THRESHOLD_S, 7 * DAY);
}

/// A reboot that has been required for eight days is drift, named with the
/// container and the days; three days is not.
#[test]
fn fix_150_a_reboot_pending_longer_than_the_threshold_is_drift() {
    let f = evaluate_patch_state(&[fact(116, 40, Some(8 * DAY), Some(DAY / 2))]);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(f[0].subject.contains("116"), "{:?}", f[0]);
    assert!(
        f[0].what.contains("reboot") && f[0].what.contains("8 days"),
        "{:?}",
        f[0]
    );
    assert!(f[0].remedy.contains("reboot"), "{:?}", f[0]);

    let quiet = evaluate_patch_state(&[fact(116, 40, Some(3 * DAY), Some(DAY / 2))]);
    assert!(quiet.is_empty(), "{:?}", quiet);
}

/// Security updates that have not run for nine days (the unattended-upgrades
/// stamp is that old) is drift; upgradable packages alone, with the stamp
/// fresh, are not — the nightly `homelab patch` takes those.
#[test]
fn fix_150_a_stalled_unattended_upgrade_is_drift_but_upgradable_packages_alone_are_not() {
    let f = evaluate_patch_state(&[fact(113, 64, None, Some(9 * DAY))]);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(
        f[0].what.contains("9 days") && f[0].what.contains("64"),
        "{:?}",
        f[0]
    );

    let quiet = evaluate_patch_state(&[fact(113, 64, None, Some(DAY))]);
    assert!(quiet.is_empty(), "{:?}", quiet);
}

/// A container that never ran unattended-upgrades (no stamp at all) is drift
/// too: the daily security patching is not happening there.
#[test]
fn fix_150_a_container_without_any_unattended_run_is_drift() {
    let f = evaluate_patch_state(&[fact(118, 3, None, None)]);
    assert_eq!(f.len(), 1, "{:?}", f);
    assert!(f[0].what.contains("never"), "{:?}", f[0]);
}

/// The probe prints three lines: upgradable count, reboot-required mtime or
/// `-`, upgrade-stamp mtime or `-`. Ages are taken against `now`.
#[test]
fn fix_150_the_probe_output_parses_into_ages() {
    let p = parse_patch_probe(116, "116-app-kp-soft", "40\n-\n1790500000\n", NOW);
    assert_eq!(p.upgradable, Some(40));
    assert_eq!(p.reboot_required_age_s, None);
    assert_eq!(p.unattended_stamp_age_s, Some(100_000));

    let p = parse_patch_probe(113, "113-app-metrics", "64\n1790000000\n-\n", NOW);
    assert_eq!(p.reboot_required_age_s, Some(600_000));
    assert_eq!(p.unattended_stamp_age_s, None);

    // Garbage from a broken container is unknown, never a finding.
    let p = parse_patch_probe(113, "113-app-metrics", "lxc-attach: failed\n", NOW);
    assert_eq!(p.upgradable, None);
    assert!(evaluate_patch_state(&[p]).is_empty());
}
