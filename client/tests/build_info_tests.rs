//! fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): a
//! binary and a deploy say which commit they came from.
//!
//! Both binaries reported only the Cargo version, so `make install` and
//! `make host-binary` builds from a working tree claimed the same "v3.59.3"
//! as the release; and nothing recorded which commit a stack was deployed
//! from, or that it was deployed from files in no commit at all.

use std::path::{Path, PathBuf};
use std::process::Command;

// The build scripts' own logic, run here against throwaway repositories.
include!("../../build-support/git_describe.rs");

fn scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "homelab-build-info-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// `git` in `dir` with a fixed identity, the environment of a hook removed
/// (a commit hook exports GIT_DIR, which would point these commands at the
/// real repository).
fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .args([
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?}");
}

fn repo_with_tag(name: &str) -> PathBuf {
    let dir = scratch(name);
    git(&dir, &["init", "-q"]);
    std::fs::create_dir_all(dir.join("stacks/app")).unwrap();
    std::fs::write(dir.join("stacks/app/lxc-compose.yml"), "a: 1\n").unwrap();
    std::fs::write(dir.join("README"), "x\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "one"]);
    git(&dir, &["tag", "v1.2.3"]);
    dir
}

#[test]
fn fix_141_describe_names_the_tag_and_marks_uncommitted_changes() {
    let dir = repo_with_tag("describe");
    assert_eq!(git_describe(&dir), "v1.2.3");
    std::fs::write(dir.join("README"), "changed\n").unwrap();
    assert_eq!(git_describe(&dir), "v1.2.3-dirty");
    git(&dir, &["commit", "-q", "-am", "two"]);
    let d = git_describe(&dir);
    assert!(
        d.starts_with("v1.2.3-1-g") && !d.ends_with("-dirty"),
        "one commit past the tag, clean: {d}"
    );
    // Cargo reruns the build script when the commit moves.
    let paths = rerun_paths(&dir);
    assert!(
        paths.iter().any(|p| p.ends_with("HEAD")),
        "HEAD is watched: {paths:?}"
    );
    assert!(
        paths.iter().all(|p| p.exists()),
        "a path that does not exist makes cargo rerun every build: {paths:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fix_141_describe_outside_a_repository_is_unknown() {
    let dir = scratch("no-repo");
    assert_eq!(git_describe(&dir), "unknown");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fix_141_the_client_carries_its_build_and_labels_the_host_s() {
    assert!(
        !homelab_client::BUILD.is_empty(),
        "the build script sets HOMELAB_BUILD"
    );
    assert_eq!(
        homelab_client::link::version_label("3.60.0", Some("v3.60.0-2-gabc1234-dirty")),
        "v3.60.0 (v3.60.0-2-gabc1234-dirty)"
    );
    // A host from before this change sends no build.
    assert_eq!(
        homelab_client::link::version_label("3.59.4", None),
        "v3.59.4 (build not reported)"
    );
}

#[test]
fn fix_141_a_stack_read_from_a_clean_tree_names_its_commit() {
    let dir = repo_with_tag("clean");
    let src = homelab_client::spec::stack_source(&dir.join("stacks/app")).expect("a git tree");
    assert_eq!(src.commit.len(), 40, "the full commit: {src:?}");
    assert!(src.uncommitted.is_empty(), "{src:?}");
    assert_eq!(src.client, homelab_client::BUILD);
    assert!(homelab_client::spec::uncommitted_warning("app", &src).is_none());
    // Outside a repository there is nothing to name.
    let bare = scratch("bare");
    assert!(homelab_client::spec::stack_source(&bare).is_none());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
}

#[test]
fn fix_141_uncommitted_files_in_the_stack_are_named_and_warned_about() {
    let dir = repo_with_tag("dirty");
    let stack = dir.join("stacks/app");
    std::fs::write(stack.join("lxc-compose.yml"), "a: 2\n").unwrap();
    std::fs::write(stack.join("new.yml"), "b: 1\n").unwrap();
    // A change elsewhere in the repository is not this stack's.
    std::fs::write(dir.join("README"), "elsewhere\n").unwrap();
    let src = homelab_client::spec::stack_source(&stack).unwrap();
    assert_eq!(
        src.uncommitted,
        vec![
            "stacks/app/lxc-compose.yml".to_string(),
            "stacks/app/new.yml".to_string()
        ]
    );
    assert_eq!(
        src.summary(),
        format!("{} + 2 uncommitted file(s)", &src.commit[..12])
    );
    let w = homelab_client::spec::uncommitted_warning("app", &src).expect("a warning");
    assert!(
        w.contains("app") && w.contains("2 uncommitted") && w.contains("stacks/app/new.yml"),
        "{w}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
