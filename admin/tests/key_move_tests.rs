//! key-move: with `HOMELAB_ADMIN_COMMIT_VIA=host` the working copy hands
//! its commit to the host and only reads the remote. Against a throwaway
//! bare repository (never GitHub); the "host" is a stand-in that commits
//! the request the way the host's own clone does.

use std::path::{Path, PathBuf};
use std::process::Command as Proc;
use std::sync::{Arc, Mutex};

use homelab_admin::core::actions_config::{GitConfig, from_env};
use homelab_admin::core::commit_via::{CommitVia, HOST_MODE_REMOTE};
use homelab_admin::core::stackedit::FileChange;
use homelab_admin::shell::workcopy::{HostCommit, Unpushed, WorkingCopy};
use homelab_proto::{Command, RpcResponse, StackCommitDone};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Proc::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "homelab-host")
        .env("GIT_AUTHOR_EMAIL", "host@example.invalid")
        .env("GIT_COMMITTER_NAME", "homelab-host")
        .env("GIT_COMMITTER_EMAIL", "host@example.invalid")
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

/// The host's part, played from a clone of its own: the request is
/// recorded, then committed and pushed (or refused, when told to).
struct FakeHost {
    root: PathBuf,
    bare: PathBuf,
    refuse: Option<String>,
    seen: Mutex<Vec<Command>>,
}

impl HostCommit for FakeHost {
    fn send(&self, command: Command) -> Result<RpcResponse, String> {
        self.seen.lock().unwrap().push(command.clone());
        let answer = |ok, message| {
            Ok(RpcResponse {
                id: 1,
                ok,
                message,
                deferred: None,
            })
        };
        if let Some(why) = &self.refuse {
            return answer(false, why.clone());
        }
        let Command::CommitStackFiles {
            message,
            base,
            files,
            ..
        } = command
        else {
            return Err("not a commit".into());
        };
        let clone = self.root.join("host-clone");
        let _ = std::fs::remove_dir_all(&clone);
        git(
            &self.root,
            &[
                "clone",
                "--quiet",
                self.bare.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        assert_eq!(
            git(&clone, &["rev-parse", "HEAD"]),
            base,
            "planned on the remote's head"
        );
        for f in &files {
            let p = clone.join(&f.path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, f.content.as_deref().unwrap()).unwrap();
        }
        git(&clone, &["add", "--all"]);
        git(&clone, &["commit", "--quiet", "-m", &message]);
        git(
            &clone,
            &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
        );
        let done = StackCommitDone {
            commit: git(&clone, &["rev-parse", "HEAD"]),
            subject: git(&clone, &["log", "-1", "--format=%s"]),
            pushed: true,
            landed_despite_error: false,
        };
        answer(true, serde_json::to_string(&done).unwrap())
    }
}

struct World {
    root: PathBuf,
    bare: PathBuf,
    host: Arc<FakeHost>,
    wc: WorkingCopy,
}

fn world(tag: &str, refuse: Option<&str>) -> World {
    let root = std::env::temp_dir().join(format!(
        "homelab-keymove-admin-{}-{tag}",
        std::process::id()
    ));
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
    std::fs::write(seed.join("stacks/media/notes.txt"), "old\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "--quiet", "-m", "seed"]);
    git(&seed, &["push", "--quiet", "origin", "main"]);
    let host = Arc::new(FakeHost {
        root: root.clone(),
        bare: bare.clone(),
        refuse: refuse.map(str::to_string),
        seen: Mutex::new(Vec::new()),
    });
    let wc = WorkingCopy::new(
        root.join("data/repo"),
        GitConfig {
            remote: bare.display().to_string(),
            branch: "main".into(),
            // No key exists: the host mode never needs one.
            key: root.join("data/deploy_key"),
            known_hosts: root.join("data/known_hosts"),
            author_name: "homelab-admin".into(),
            author_email: "homelab-admin@example.invalid".into(),
        },
        root.join("data/tmp"),
        Arc::new(|| 1_800_000_000),
    )
    .with_host_commit(host.clone());
    wc.sync().unwrap();
    World {
        root,
        bare,
        host,
        wc,
    }
}

fn edit() -> Vec<FileChange> {
    vec![FileChange {
        path: "stacks/media/notes.txt".into(),
        old: Some("old\n".into()),
        new: Some("new\n".into()),
    }]
}

#[test]
fn key_move_the_host_commits_and_the_copy_takes_its_commit() {
    let w = world("ok", None);
    let before = git(&w.wc.repo, &["rev-parse", "HEAD"]);
    let done =
        w.wc.transact("media", &edit(), "edit media [key-move]\n", |_| Vec::new())
            .unwrap();
    assert!(done.pushed);
    assert_eq!(git(&w.bare, &["rev-parse", "main"]), done.commit);
    // The copy fast-forwarded to the host's commit and wrote nothing itself.
    assert_eq!(git(&w.wc.repo, &["rev-parse", "HEAD"]), done.commit);
    assert_eq!(
        git(&w.wc.repo, &["log", "-1", "--format=%an"]),
        "homelab-host"
    );
    let seen = w.host.seen.lock().unwrap();
    let Command::CommitStackFiles {
        base, files, push, ..
    } = &seen[0]
    else {
        panic!()
    };
    assert_eq!(base, &before);
    assert!(*push);
    assert_eq!(files[0].content.as_deref(), Some("new\n"));
    assert_eq!(w.wc.status().commits_via, "host");
    let _ = std::fs::remove_dir_all(&w.root);
}

#[test]
fn key_move_a_refusal_leaves_the_copy_as_it_was() {
    let w = world("refused", Some("the remote moved :: make the plan again"));
    let before = git(&w.wc.repo, &["rev-parse", "HEAD"]);
    let r =
        w.wc.transact("media", &edit(), "edit [key-move]\n", |_| Vec::new())
            .unwrap_err();
    assert!(r.why.contains("the remote moved"), "{}", r.why);
    assert_eq!(r.fix, "make the plan again");
    assert_eq!(git(&w.wc.repo, &["rev-parse", "HEAD"]), before);
    assert_eq!(git(&w.wc.repo, &["status", "--porcelain"]), "");
    // The panel's push cannot run here: the host is the only one with a key.
    assert!(w.wc.resolve(Unpushed::Push).is_err());
    let _ = std::fs::remove_dir_all(&w.root);
}

#[test]
fn key_move_commit_via_host_reads_over_https_and_never_with_a_token() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    };
    let c = from_env(&env(&[])).unwrap();
    assert_eq!(c.commit_via, CommitVia::Dashboard);
    let c = from_env(&env(&[("HOMELAB_ADMIN_COMMIT_VIA", "host")])).unwrap();
    assert_eq!(
        (c.commit_via, c.git.remote.as_str()),
        (CommitVia::Host, HOST_MODE_REMOTE)
    );
    let token = from_env(&env(&[
        ("HOMELAB_ADMIN_COMMIT_VIA", "host"),
        (
            "HOMELAB_ADMIN_GIT_REMOTE",
            "https://x:ghp_secret@github.com/a/b.git",
        ),
    ]))
    .unwrap_err();
    assert!(token.contains("must not carry a user or token"), "{token}");
    let dashboard_https = from_env(&env(&[(
        "HOMELAB_ADMIN_GIT_REMOTE",
        "https://github.com/a/b.git",
    )]))
    .unwrap_err();
    assert!(dashboard_https.contains("ssh remote"), "{dashboard_https}");
    let typo = from_env(&env(&[("HOMELAB_ADMIN_COMMIT_VIA", "hots")])).unwrap_err();
    assert!(typo.contains("`dashboard` or `host`"), "{typo}");
}
