//! fix-218 round 2 (live on host 3.70.7, 2026-10-03): `homelab doctor` said
//! "stack inbox backup — last backup 22h ago" while `homelab snapshots
//! stacks/inbox` said "no repositories". Both read the same snapshot cache,
//! but chose the stack's repositories differently: `GetBackups` took the
//! manifest's `owner_groups` whenever a manifest was recorded and fell back to
//! the native units only without one; doctor took the native units whenever
//! the stack had any. inbox carries both since fix-145 (an
//! `lxc-compose.yml` with `storage: []` plus `natives: [inbox]`), so
//! `GetBackups` found zero repositories while the nightly native backup had
//! been writing the `inbox` repository all along.

use homelab_core::manifest::StackManifest;
use homelab_core::native::NativeServiceManifest;
use homelab_core::ops::backup::stack_repo_owners;
use homelab_core::state::StackState;

fn recorded(stack: &str, with_manifest: bool) -> StackState {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../stacks/");
    let manifest: StackManifest = serde_yaml::from_str(
        &std::fs::read_to_string(format!("{root}{stack}/lxc-compose.yml")).unwrap(),
    )
    .unwrap();
    let native: NativeServiceManifest = serde_yaml::from_str(
        &std::fs::read_to_string(format!("{root}{stack}/service.yml")).unwrap(),
    )
    .unwrap();
    serde_json::from_value(serde_json::json!({
        "vmid": manifest.vmid,
        "hostname": manifest.hostname,
        "apps": [],
        "applied_at": 0,
        "manifest": if with_manifest { Some(&manifest) } else { None },
        "natives": [native],
    }))
    .unwrap()
}

/// covers: fix-218
/// fail-first: `stack_repo_owners` is new; written first with
/// `Rpc::GetBackups`'s old choice (manifest owners whenever a manifest was
/// recorded) as its body, this failed with `left: []` (2026-10-03).
#[test]
fn fix_218_native_stack_with_an_empty_storage_manifest_still_owns_its_unit_repository() {
    // The real stack files, recorded the way a deploy records them.
    let st = recorded("inbox", true);
    assert!(st.manifest.is_some() && st.is_native());
    assert_eq!(stack_repo_owners(&st), vec!["inbox".to_string()]);
}

/// covers: fix-218
/// fail-first: a guard that a native stack with a real mount keeps exactly
/// its unit's repository; passed on the old choice too (almanac worked).
#[test]
fn fix_218_native_stack_with_a_mount_lists_each_repository_once() {
    // almanac: a bind mount owned by `almanac` and the native unit `almanac`.
    let st = recorded("almanac", true);
    assert_eq!(stack_repo_owners(&st), vec!["almanac".to_string()]);
}

/// covers: fix-218
/// fail-first: a guard for the adopt-only shape (no manifest recorded);
/// passed on the old choice too.
#[test]
fn fix_218_native_stack_recorded_without_a_manifest_reads_its_units() {
    let st = recorded("inbox", false);
    assert_eq!(stack_repo_owners(&st), vec!["inbox".to_string()]);
}
