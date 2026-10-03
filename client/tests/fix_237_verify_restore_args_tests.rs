//! fix-237: `homelab verify-restore stacks/<name> [<snapshot>|latest]
//! [--app <app>]` — restore one snapshot into the drill's scratch
//! directory, never the live data.

use homelab_client::snapshots::{VerifyRestoreArgs, verify_restore_args};

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// covers: fix-237
#[test]
fn fix_237_the_words_are_stack_then_snapshot_and_app_stands_anywhere() {
    assert_eq!(
        verify_restore_args(&args(&["--app", "jellyfin", "stacks/media", "af364ed7"])).unwrap(),
        VerifyRestoreArgs {
            stack: "stacks/media".into(),
            snapshot: "af364ed7".into(),
            app: Some("jellyfin".into()),
        }
    );
    // The snapshot defaults to latest; no --app means every repository.
    assert_eq!(
        verify_restore_args(&args(&["media"])).unwrap(),
        VerifyRestoreArgs {
            stack: "media".into(),
            snapshot: "latest".into(),
            app: None,
        }
    );
}

/// covers: fix-237
#[test]
fn fix_237_refuses_what_it_cannot_read() {
    assert!(verify_restore_args(&args(&[])).is_err());
    assert!(verify_restore_args(&args(&["media", "latest", "extra"])).is_err());
    assert!(verify_restore_args(&args(&["media", "--app"])).is_err());
    assert!(verify_restore_args(&args(&["media", "--app", "--yes"])).is_err());
    assert!(verify_restore_args(&args(&["media", "--force"])).is_err());
}
