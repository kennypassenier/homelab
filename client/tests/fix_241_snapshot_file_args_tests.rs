//! fix-241: `homelab snapshot-file stacks/<name> <snapshot|latest> --app
//! <app> <path>` — one file from a snapshot, read-only.

use homelab_client::snapshots::{render_snapshot_file, snapshot_file_args};
use homelab_core::ops::backup::SnapshotFile;

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// covers: fix-241
#[test]
fn fix_241_flags_stand_anywhere_and_the_words_are_stack_snapshot_path() {
    let a = snapshot_file_args(&args(&[
        "--app",
        "jellyfin",
        "stacks/media",
        "af364ed7",
        "config/encoding.xml",
        "--json",
    ]))
    .unwrap();
    assert_eq!(a.stack, "stacks/media");
    assert_eq!(a.snapshot, "af364ed7");
    assert_eq!(a.app, "jellyfin");
    assert_eq!(a.path, "config/encoding.xml");
    assert!(a.json);
}

/// covers: fix-241
#[test]
fn fix_241_without_app_or_with_a_word_missing_it_is_refused() {
    assert!(snapshot_file_args(&args(&["stacks/media", "latest", "config/x.xml"])).is_err());
    assert!(snapshot_file_args(&args(&["stacks/media", "latest", "--app", "jellyfin"])).is_err());
    assert!(snapshot_file_args(&args(&["stacks/media", "--app"])).is_err());
    assert!(
        snapshot_file_args(&args(&[
            "stacks/media",
            "latest",
            "--app",
            "jellyfin",
            "x",
            "--yes"
        ]))
        .is_err(),
        "an unknown flag is refused, not ignored"
    );
}

/// covers: fix-241
#[test]
fn fix_241_a_cut_or_binary_file_says_so_and_prints_only_text() {
    let mut f = SnapshotFile {
        owner: "jellyfin".into(),
        snapshot: "latest".into(),
        path: "/appdata/media/jellyfin-config/config/encoding.xml".into(),
        shown_bytes: 1048576,
        truncated: true,
        cap_bytes: 1048576,
        binary: false,
        text: Some("abc".into()),
    };
    let (out, note) = render_snapshot_file(&f);
    assert_eq!(out, "abc");
    assert!(
        note.contains("CUT") && note.contains("1024 KiB"),
        "{}",
        note
    );
    assert!(note.contains("nothing restored"), "{}", note);
    f.truncated = false;
    f.binary = true;
    f.text = None;
    let (out, note) = render_snapshot_file(&f);
    assert_eq!(out, "");
    assert!(note.contains("not text"), "{}", note);
}
