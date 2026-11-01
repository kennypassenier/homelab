//! key-move: the pure checks on a `CommitStackFiles` request and the
//! `[stack_commits]` settings.

use homelab_core::ops::stackcommit::guard::{
    path_problem, request_problems, staged_outside, under_roots,
};
use homelab_core::ops::stackcommit::{
    DEFAULT_REMOTE, DEFAULT_ROOTS, StackCommitSettings, StackFileChange,
};

fn roots() -> Vec<String> {
    DEFAULT_ROOTS.iter().map(|s| s.to_string()).collect()
}

fn file(path: &str, text: &str) -> StackFileChange {
    StackFileChange {
        path: path.into(),
        content: Some(text.into()),
    }
}

#[test]
fn key_move_paths_that_climb_or_reach_into_git_are_refused() {
    for bad in [
        "",
        "/etc/passwd",
        "stacks/../config/x",
        "stacks//a",
        "stacks/./a",
        "stacks/a/.GIT/config",
        "stacks\\a",
        "stacks/a\nb",
    ] {
        assert!(path_problem(bad).is_some(), "{bad:?} passed");
    }
    assert!(path_problem("stacks/media/lxc-compose.yml").is_none());
}

#[test]
fn key_move_only_the_configured_roots_are_reachable() {
    let r = roots();
    assert!(under_roots("stacks/media/a.yml", &r));
    assert!(under_roots("presets/x/preset.yml", &r));
    assert!(under_roots("config/host.toml", &r));
    assert!(!under_roots("config/host.toml.bak", &r));
    assert!(!under_roots("stacks/", &r));
    assert!(!under_roots("docs/x.md", &r));
    assert!(!under_roots("Makefile", &r));
}

#[test]
fn key_move_a_request_is_checked_whole() {
    let base = "a".repeat(40);
    let ok = vec![file("stacks/media/a.yml", "x")];
    assert!(request_problems("m", &base, &ok, &roots(), 100).is_empty());
    // A secret's file, twice the same path, a short base, an empty
    // message and too many bytes: each is named.
    let bad = vec![
        file("stacks/media/.env", "SECRET=1"),
        file("stacks/media/a.yml", "x"),
        file("stacks/media/a.yml", "y"),
    ];
    let p = request_problems(" ", "abc", &bad, &roots(), 4).join("\n");
    for want in [
        "secret",
        "named twice",
        "full commit hash",
        "message is empty",
        "bytes",
    ] {
        assert!(p.contains(want), "{want} missing in:\n{p}");
    }
    assert!(!request_problems("m", &base, &[], &roots(), 100).is_empty());
}

#[test]
fn key_move_the_staged_set_may_hold_only_named_paths() {
    let files = vec![file("stacks/media/a.yml", "x")];
    let staged = vec![
        "stacks/media/a.yml".to_string(),
        "stacks/other/b.yml".to_string(),
    ];
    assert_eq!(staged_outside(&staged, &files), vec!["stacks/other/b.yml"]);
}

#[test]
fn key_move_the_settings_need_a_key_and_fill_the_rest() {
    let err = StackCommitSettings::default()
        .resolve("/var/lib/homelab")
        .unwrap_err();
    assert!(err.contains("stack_commits.key"), "{err}");
    let cfg = StackCommitSettings {
        key: Some("/etc/homelab/deploy_key".into()),
        ..Default::default()
    }
    .resolve("/var/lib/homelab")
    .unwrap();
    assert_eq!(cfg.remote, DEFAULT_REMOTE);
    assert_eq!(cfg.branch, "main");
    // Never the intent repository at <state_dir>/repo.
    assert_eq!(cfg.dir, "/var/lib/homelab/stack-commits/repo");
    assert_eq!(
        cfg.known_hosts,
        "/var/lib/homelab/stack-commits/known_hosts"
    );
    let https = StackCommitSettings {
        key: Some("/k".into()),
        remote: Some("https://github.com/x/y.git".into()),
        branch: Some("+main".into()),
        ..Default::default()
    }
    .resolve("/s")
    .unwrap_err();
    assert!(
        https.contains("ssh remote") && https.contains("branch"),
        "{https}"
    );
}
