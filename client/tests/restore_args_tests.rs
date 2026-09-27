//! fix-112 (restore-stale-files-mixed-nights, 2026-09-27): `homelab restore`
//! takes `--app <name>` to restore one app of a stack, wherever it stands.

use homelab_client::restore_args;

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_app_flag_takes_its_value_and_leaves_dir_and_snapshot_alone() {
    let a = restore_args(&args(&[
        "stacks/media",
        "--app",
        "sonarr",
        "3f2a1b0c",
        "--yes",
    ]))
    .unwrap();
    assert_eq!(a.dir, "stacks/media");
    assert_eq!(a.snapshot, "3f2a1b0c");
    assert_eq!(a.app.as_deref(), Some("sonarr"));
    assert!(a.yes);
    assert!(!a.skip_safety_copy);
}

#[test]
fn without_flags_it_is_the_whole_stack_from_latest() {
    let a = restore_args(&args(&["stacks/media"])).unwrap();
    assert_eq!(a.snapshot, "latest");
    assert_eq!(a.app, None);
}

#[test]
fn an_app_flag_without_a_name_is_refused() {
    assert!(restore_args(&args(&["stacks/media", "--app"])).is_err());
    assert!(restore_args(&args(&["stacks/media", "--app", "--yes"])).is_err());
    assert!(restore_args(&args(&[])).is_err());
}
