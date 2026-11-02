//! key-move: the host commits and pushes the dashboard's edits — against a
//! throwaway bare repository in a temp directory (never GitHub, no network)
//! and a fake deploy key.

use super::support::*;
use crate::stack_commit::{Request, commit};
use crate::*;
use homelab_core::ops::stackcommit::{StackCommitConfig, StackCommitSettings, StackFileChange};
use std::path::{Path, PathBuf};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "seed")
        .env("GIT_AUTHOR_EMAIL", "seed@example.invalid")
        .env("GIT_COMMITTER_NAME", "seed")
        .env("GIT_COMMITTER_EMAIL", "seed@example.invalid")
        .env_remove("GIT_DIR")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A bare "GitHub" seeded with one stack file (and, when asked, a symlink
/// pointing out of the tree), and the host's settings for it.
struct World {
    root: PathBuf,
    bare: PathBuf,
    cfg: StackCommitConfig,
}

impl World {
    fn new(tag: &str, symlink_to: Option<&Path>) -> World {
        let root =
            std::env::temp_dir().join(format!("homelab-keymove-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let bare = root.join("origin.git");
        git(
            &root,
            &[
                "init",
                "--quiet",
                "--bare",
                "-b",
                "main",
                bare.to_str().unwrap(),
            ],
        );
        let seed = root.join("seed");
        git(
            &root,
            &[
                "clone",
                "--quiet",
                bare.to_str().unwrap(),
                seed.to_str().unwrap(),
            ],
        );
        git(&seed, &["checkout", "--quiet", "-b", "main"]);
        std::fs::create_dir_all(seed.join("stacks/media")).unwrap();
        std::fs::write(seed.join("stacks/media/lxc-compose.yml"), "old: 1\n").unwrap();
        if let Some(target) = symlink_to {
            std::os::unix::fs::symlink(target, seed.join("stacks/escape")).unwrap();
        }
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "--quiet", "-m", "seed"]);
        git(&seed, &["push", "--quiet", "origin", "main"]);
        let settings = StackCommitSettings {
            remote: Some(bare.display().to_string()),
            key: Some(root.join("deploy_key").display().to_string()),
            ..Default::default()
        };
        let cfg = settings
            .resolve(root.join("state").to_str().unwrap())
            .unwrap();
        World { root, bare, cfg }
    }

    fn remote_head(&self) -> String {
        git(&self.bare, &["rev-parse", "main"])
    }

    fn request(&self, files: Vec<(&str, Option<&str>)>, push: bool) -> Request {
        Request {
            message: "edit media [key-move]\n".into(),
            base: self.remote_head(),
            files: files
                .into_iter()
                .map(|(p, c)| StackFileChange {
                    path: p.into(),
                    content: c.map(str::to_string),
                })
                .collect(),
            push,
        }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
pub(crate) fn key_move_the_host_commits_and_pushes_the_edit_as_itself() {
    let w = World::new("push", None);
    let req = w.request(
        vec![
            ("stacks/media/lxc-compose.yml", Some("new: 2\n")),
            ("stacks/media/extra.yml", Some("x: 1\n")),
        ],
        true,
    );
    let done = commit(&w.cfg, &req).unwrap();
    assert!(done.pushed && !done.landed_despite_error);
    assert_eq!(w.remote_head(), done.commit);
    assert_eq!(done.subject, "edit media [key-move]");
    assert_eq!(
        git(&w.bare, &["show", "main:stacks/media/lxc-compose.yml"]),
        "new: 2"
    );
    assert_eq!(
        git(&w.bare, &["log", "-1", "--format=%an <%ae>", "main"]),
        homelab_core::ops::stackcommit::DEFAULT_AUTHOR
    );
    // A delete in a second request starts from the first one's commit.
    let del = w.request(vec![("stacks/media/extra.yml", None)], true);
    commit(&w.cfg, &del).unwrap();
    let tree = git(&w.bare, &["ls-tree", "-r", "--name-only", "main"]);
    assert!(!tree.contains("extra.yml"), "{tree}");
}

#[test]
pub(crate) fn key_move_a_stale_base_is_refused_and_the_remote_is_untouched() {
    let w = World::new("stale", None);
    let before = w.remote_head();
    let mut req = w.request(vec![("stacks/media/lxc-compose.yml", Some("new\n"))], true);
    req.base = "1".repeat(40);
    let err = commit(&w.cfg, &req).unwrap_err();
    assert!(err.contains("planned on"), "{err}");
    assert_eq!(w.remote_head(), before);
}

#[test]
pub(crate) fn key_move_a_dry_run_commits_nothing_anywhere() {
    let w = World::new("dry", None);
    let before = w.remote_head();
    let done = commit(
        &w.cfg,
        &w.request(vec![("stacks/media/lxc-compose.yml", Some("dry\n"))], false),
    )
    .unwrap();
    assert!(!done.pushed);
    assert_eq!(w.remote_head(), before);
    let clone = Path::new(&w.cfg.dir);
    assert_eq!(git(clone, &["rev-parse", "HEAD"]), before);
    assert_eq!(git(clone, &["status", "--porcelain"]), "");
}

#[test]
pub(crate) fn key_move_only_one_commit_runs_at_a_time() {
    let w = World::new("lock", None);
    let held = crate::stack_commit::claim(&w.cfg.dir).unwrap();
    let req = w.request(vec![("stacks/media/lxc-compose.yml", Some("x\n"))], true);
    let err = commit(&w.cfg, &req).unwrap_err();
    assert!(err.contains("another stack commit is running"), "{err}");
    drop(held);
    commit(&w.cfg, &req).unwrap();
}

#[test]
pub(crate) fn key_move_a_dirty_clone_is_refused() {
    let w = World::new("dirty", None);
    commit(
        &w.cfg,
        &w.request(vec![("stacks/media/lxc-compose.yml", Some("a\n"))], true),
    )
    .unwrap();
    std::fs::write(Path::new(&w.cfg.dir).join("stacks/media/stray.yml"), "x").unwrap();
    let err = commit(
        &w.cfg,
        &w.request(vec![("stacks/media/lxc-compose.yml", Some("b\n"))], true),
    )
    .unwrap_err();
    assert!(err.contains("differ from its last commit"), "{err}");
}

#[test]
pub(crate) fn key_move_a_path_through_a_symlink_is_refused_and_nothing_lands_outside() {
    let outside =
        std::env::temp_dir().join(format!("homelab-keymove-outside-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).unwrap();
    let w = World::new("symlink", Some(&outside));
    let before = w.remote_head();
    let err = commit(
        &w.cfg,
        &w.request(vec![("stacks/escape/owned.yml", Some("x\n"))], true),
    )
    .unwrap_err();
    assert!(err.contains("symlink"), "{err}");
    assert!(!outside.join("owned.yml").exists());
    assert_eq!(w.remote_head(), before);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
pub(crate) fn key_move_the_fake_key_must_be_private_and_github_host_keys_are_written() {
    use std::os::unix::fs::PermissionsExt as _;
    let w = World::new("key", None);
    let mut cfg = w.cfg.clone();
    cfg.remote = homelab_core::ops::stackcommit::DEFAULT_REMOTE.into();
    let err = crate::stack_commit::credentials_ready(&cfg).unwrap_err();
    assert!(err.contains("no deploy key"), "{err}");
    std::fs::write(
        &cfg.key,
        "-----BEGIN OPENSSH PRIVATE KEY-----\nfake\n-----END OPENSSH PRIVATE KEY-----\n",
    )
    .unwrap();
    std::fs::set_permissions(&cfg.key, std::fs::Permissions::from_mode(0o644)).unwrap();
    let err = crate::stack_commit::credentials_ready(&cfg).unwrap_err();
    assert!(err.contains("chmod 600"), "{err}");
    std::fs::set_permissions(&cfg.key, std::fs::Permissions::from_mode(0o600)).unwrap();
    crate::stack_commit::credentials_ready(&cfg).unwrap();
    let hosts = std::fs::read_to_string(&cfg.known_hosts).unwrap();
    assert!(hosts.starts_with("github.com ssh-ed25519 "), "{hosts}");
}

#[tokio::test]
pub(crate) async fn key_move_the_rpc_refuses_when_off_and_guards_paths_with_an_audit_line() {
    let off = test_state(config_from_text(TOKEN));
    let r = crate::rpc::handle_rpc(&off, commit_rpc("stacks/media/a.yml")).await;
    assert!(
        !r.ok && r.message.contains("stack commits are off"),
        "{}",
        r.message
    );
    let w = World::new("rpc", None);
    let state_dir = w.root.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let raw = format!(
        "{TOKEN}state_dir = {:?}\n[stack_commits]\nremote = {:?}\nkey = \"/nonexistent\"\n",
        state_dir.display().to_string(),
        w.bare.display().to_string()
    );
    let on = test_state(config_from_text(&raw));
    for bad in [
        "stacks/../etc/passwd",
        "stacks/media/.env",
        "docs/x.md",
        "stacks/.git/config",
    ] {
        let r = crate::rpc::handle_rpc(&on, commit_rpc(bad)).await;
        assert!(!r.ok, "{bad} passed");
        assert!(
            r.message.contains("nothing was written"),
            "{bad}: {}",
            r.message
        );
    }
    let audit = std::fs::read_to_string(state_dir.join("audit.log")).unwrap();
    assert_eq!(
        audit
            .lines()
            .filter(|l| l.contains("commit-stack-files"))
            .count(),
        4,
        "{audit}"
    );
}

const TOKEN: &str = "token = \"0123456789abcdef0123\"\n";

fn commit_rpc(path: &str) -> RpcRequest {
    RpcRequest {
        id: 9,
        command: Rpc::CommitStackFiles {
            message: "m [key-move]".into(),
            base: "a".repeat(40),
            files: vec![StackFileChange {
                path: path.into(),
                content: Some("x".into()),
            }],
            push: true,
        },
    }
}

#[test]
pub(crate) fn key_move_a_path_with_pattern_characters_is_a_name_not_a_glob() {
    let w = World::new("glob", None);
    let req = w.request(vec![("stacks/media/a[b]*.yml", Some("x: 1\n"))], true);
    let done = commit(&w.cfg, &req).expect("a literal name commits");
    assert!(done.pushed);
    let files = git(&w.bare, &["ls-tree", "-r", "--name-only", "main"]);
    assert!(files.contains("stacks/media/a[b]*.yml"), "{files}");
}
