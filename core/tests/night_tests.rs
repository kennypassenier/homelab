//! The nightly round's per-stack decisions (expert panel 2026-09-27,
//! host-monolith-untested). They lived inline in the host's 360-line
//! `scheduler_loop`, which no test reached; every backup and update fault
//! the panel found depends on them.

use homelab_core::manifest::StackManifest;
use homelab_core::native::NativeServiceManifest;
use homelab_core::ops::backup::NightBackup;
use homelab_core::ops::night::{BackupWork, UpdateWork, backup_work, stack_night};
use homelab_core::sink::Level;
use homelab_core::state::StackState;

fn compose_manifest() -> StackManifest {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks/drill/lxc-compose.yml"),
    )
    .unwrap();
    serde_yaml::from_str(&text).unwrap()
}

fn service(unit: &str, policy: &str) -> NativeServiceManifest {
    serde_json::from_value(serde_json::json!({
        "stack_name": "kyu", "vmid": 109, "hostname": "109-app-kyu", "unit": unit,
        "binary": format!("/opt/kyu/bin/{unit}"), "env_file": null,
        "data_dirs": ["/appdata/kyu/kyu-config"], "update_cmd": null,
        "update_policy": policy
    }))
    .unwrap()
}

fn stack(manifest: Option<StackManifest>, natives: Vec<NativeServiceManifest>) -> StackState {
    StackState {
        pushed_file_hashes: std::collections::BTreeMap::new(),
        component_digests: Default::default(),
        applied_source: None,
        extra_route_files: Vec::new(),
        vmid: 119,
        hostname: "119-app-drill".into(),
        apps: Vec::new(),
        applied_at: 7,
        last_backup: 0,
        applied_hash: String::new(),
        manifest,
        enabled: true,
        route_file: None,
        natives,
        incomplete_step: None,
    }
}

fn says(lines: &[(Level, String)], level: Level, needle: &str) -> bool {
    lines.iter().any(|(l, m)| *l == level && m.contains(needle))
}

/// A stack that is not due tonight is left alone; a disabled one says why.
#[test]
fn a_stack_not_due_tonight_gets_nothing() {
    let st = stack(Some(compose_manifest()), Vec::new());
    let n = stack_night("drill", &st, false, false, &NightBackup::Done);
    assert!(!n.record_last_backup);
    assert!(matches!(n.updates, UpdateWork::None));
    assert!(!n.settle_park);
    assert!(n.lines.is_empty());

    let mut off = st.clone();
    off.enabled = false;
    let n = stack_night("drill", &off, false, false, &NightBackup::Done);
    assert!(says(&n.lines, Level::Info, "disabled"), "{:?}", n.lines);
}

/// The ordinary night of a compose stack: the backup ran, so its time is
/// recorded and the update runs, and the update's outcome settles the park.
#[test]
fn a_compose_stack_backed_up_tonight_is_recorded_and_updated() {
    let st = stack(Some(compose_manifest()), Vec::new());
    let n = stack_night("drill", &st, true, false, &NightBackup::Done);
    assert!(n.record_last_backup);
    assert!(matches!(n.updates, UpdateWork::Compose(ref m) if m.stack_name == "drill"));
    assert!(n.settle_park);
}

/// fix-60: no update without a backup of tonight to go back to, and no
/// timestamp for a backup that did not happen.
#[test]
fn a_failed_or_deferred_backup_holds_the_update_back() {
    let st = stack(Some(compose_manifest()), Vec::new());
    for backup in [NightBackup::Failed, NightBackup::Deferred("a film".into())] {
        let n = stack_night("drill", &st, true, false, &backup);
        assert!(!n.record_last_backup);
        assert!(matches!(n.updates, UpdateWork::None));
        assert!(
            !n.settle_park,
            "a held-back update neither parks nor unparks"
        );
        assert!(
            says(&n.lines, Level::Info, "skipped tonight"),
            "{:?}",
            n.lines
        );
    }
}

/// fix-59: parked updates are skipped, the backup is still recorded.
#[test]
fn a_parked_compose_stack_is_backed_up_but_not_updated() {
    let st = stack(Some(compose_manifest()), Vec::new());
    let n = stack_night("drill", &st, true, true, &NightBackup::Done);
    assert!(n.record_last_backup);
    assert!(matches!(n.updates, UpdateWork::None));
    assert!(!n.settle_park);
    assert!(says(&n.lines, Level::Info, "parked"), "{:?}", n.lines);
}

/// A compose stack whose manifest was never stored is skipped with a
/// warning, before anything is recorded.
#[test]
fn a_compose_stack_without_a_stored_manifest_is_skipped_loudly() {
    let st = stack(None, Vec::new());
    let n = stack_night("drill", &st, true, false, &NightBackup::Done);
    assert!(!n.record_last_backup);
    assert!(matches!(n.updates, UpdateWork::None));
    assert!(
        says(&n.lines, Level::Warn, "no stored manifest"),
        "{:?}",
        n.lines
    );
    assert!(backup_work(&st).is_none());
}

/// fix-58: each native service's policy picks its update path; the stack's
/// night settles the park even when no service had anything to update.
/// fix-148: `auto` is the signed release update only, `self` the unit's own
/// verb only, `manual` neither.
#[test]
fn native_services_are_updated_along_their_own_policy() {
    let st = stack(
        None,
        vec![
            service("kyu", "auto"),
            service("almanac", "self"),
            service("inbox", "manual"),
        ],
    );
    let n = stack_night("kyu", &st, true, false, &NightBackup::Done);
    assert!(n.record_last_backup);
    let UpdateWork::Native { release, own_cmd } = &n.updates else {
        panic!("{:?}", n.updates);
    };
    let units = |v: &[NativeServiceManifest]| v.iter().map(|s| s.unit.clone()).collect::<Vec<_>>();
    assert_eq!(units(release), vec!["kyu"]);
    assert_eq!(units(own_cmd), vec!["almanac"]);
    assert!(n.settle_park);

    // Parked: nothing is updated, and the night still settles (as before).
    let n = stack_night("kyu", &st, true, true, &NightBackup::Done);
    assert!(
        matches!(&n.updates, UpdateWork::Native { release, own_cmd } if release.is_empty() && own_cmd.is_empty())
    );
    assert!(n.settle_park);
    assert!(n.record_last_backup);
}

/// What the backup batch takes for a due stack.
#[test]
fn the_backup_batch_takes_the_services_or_the_manifest() {
    let native = stack(None, vec![service("kyu", "auto")]);
    assert!(matches!(backup_work(&native), Some(BackupWork::Native(ref s)) if s.len() == 1));
    let compose = stack(Some(compose_manifest()), Vec::new());
    assert!(matches!(
        backup_work(&compose),
        Some(BackupWork::Compose(_))
    ));
}
