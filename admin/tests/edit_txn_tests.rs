//! arch-edit-txn, arch-push-credential: the working copy's transaction
//! against a local bare repository in a temp directory (never GitHub):
//! clone, fetch + fast-forward (refused otherwise), validate, write, commit
//! only under `stacks/<stack>/`, push, and `ls-remote` deciding a push that
//! reported an error.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use homelab_admin::core::actions_config::GitConfig;
use homelab_admin::core::stackedit::{changes, FileChange, SettingsEdit, StackEdit};
use homelab_admin::shell::workcopy::{read_texts, scrub, Unpushed, WorkingCopy};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn real_stack_file(stack: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../stacks")
            .join(stack)
            .join("lxc-compose.yml"),
    )
    .unwrap()
}

struct World {
    root: PathBuf,
    bare: PathBuf,
    seed: PathBuf,
    wc: WorkingCopy,
}

impl World {
    fn new(tag: &str) -> World {
        let root =
            std::env::temp_dir().join(format!("homelab-edit-txn-{}-{tag}", std::process::id()));
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
        for stack in ["kp-soft", "admin"] {
            let d = seed.join("stacks").join(stack);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("lxc-compose.yml"), real_stack_file(stack)).unwrap();
        }
        std::fs::write(seed.join("README.md"), "homelab\n").unwrap();
        git(&seed, &["add", "."]);
        git(&seed, &["commit", "--quiet", "-m", "seed [meta]"]);
        git(&seed, &["push", "--quiet", "origin", "main"]);
        let wc = WorkingCopy::new(
            root.join("data/repo"),
            GitConfig {
                remote: bare.display().to_string(),
                branch: "main".into(),
                key: root.join("data/deploy_key"),
                known_hosts: root.join("data/known_hosts"),
                author_name: "homelab-admin".into(),
                author_email: "homelab-admin@example.invalid".into(),
            },
            root.join("data/tmp"),
            Arc::new(|| 1_800_000_000),
        );
        World {
            root,
            bare,
            seed,
            wc,
        }
    }

    fn remote_head(&self) -> String {
        git(&self.bare, &["rev-parse", "main"]).trim().to_string()
    }

    fn remote_subject(&self) -> String {
        git(&self.bare, &["log", "-1", "--format=%s", "main"])
            .trim()
            .to_string()
    }

    fn remote_paths(&self) -> Vec<String> {
        git(&self.bare, &["show", "--name-only", "--format=", "main"])
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn memory_edit(&self, mb: u32) -> Vec<FileChange> {
        let texts = read_texts(&self.wc.repo.join("stacks/kp-soft"));
        changes(
            "kp-soft",
            &texts,
            &StackEdit::Settings(SettingsEdit {
                memory_mb: Some(mb),
                ..Default::default()
            }),
            None,
        )
        .unwrap()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn arch_edit_txn_a_change_is_cloned_validated_committed_and_pushed() {
    let w = World::new("happy");
    assert!(!w.wc.present());
    w.wc.sync().expect("the first sync clones");
    assert!(w.wc.present());
    let ch = w.memory_edit(3072);
    assert_eq!(ch.len(), 1);
    assert_eq!(ch[0].path, "stacks/kp-soft/lxc-compose.yml");
    let done =
        w.wc.transact(
            "kp-soft",
            &ch,
            "stacks/kp-soft: memory 3 GB [feat-stacks-2]\n\nbody\n",
            |_| Vec::new(),
        )
        .expect("committed");
    assert!(done.pushed && !done.landed_despite_error);
    assert_eq!(w.remote_head(), done.commit);
    assert_eq!(
        w.remote_subject(),
        "stacks/kp-soft: memory 3 GB [feat-stacks-2]"
    );
    assert_eq!(w.remote_paths(), vec!["stacks/kp-soft/lxc-compose.yml"]);
    let author = git(&w.bare, &["log", "-1", "--format=%an <%ae>", "main"]);
    assert_eq!(
        author.trim(),
        "homelab-admin <homelab-admin@example.invalid>"
    );
    let status = w.wc.status();
    assert!(
        status.unpushed.is_empty() && status.dirty.is_empty(),
        "{status:?}"
    );
}

/// The dashboard commits only under the stack it edits: a change outside
/// it is refused before anything is written, and nothing lands.
#[test]
fn arch_push_credential_nothing_outside_the_stack_is_committed() {
    let w = World::new("outside");
    w.wc.sync().unwrap();
    let before = w.remote_head();
    for path in [
        "stacks/admin/lxc-compose.yml",
        "README.md",
        "stacks/kp-soft/../admin/x",
        "stacks/kp-soft",
    ] {
        let ch = vec![FileChange {
            path: path.into(),
            old: std::fs::read_to_string(w.wc.repo.join(path)).ok(),
            new: Some("x: 1\n".into()),
        }];
        let e =
            w.wc.transact("kp-soft", &ch, "x [meta]", |_| Vec::new())
                .unwrap_err();
        assert!(
            e.why.contains("outside stacks/kp-soft/") || e.why.contains("plain path"),
            "{path}: {}",
            e.why
        );
    }
    assert_eq!(w.remote_head(), before);
    assert!(w.wc.status().dirty.is_empty());
}

/// Behind the remote: fast-forwarded first, then committed on top. A local
/// commit the remote lacks, or a file changed by hand: refused.
#[test]
fn arch_edit_txn_fast_forward_only() {
    let w = World::new("ff");
    w.wc.sync().unwrap();
    // Someone pushes from a workstation meanwhile.
    std::fs::write(w.seed.join("README.md"), "homelab v2\n").unwrap();
    git(&w.seed, &["commit", "--quiet", "-am", "readme [meta]"]);
    git(&w.seed, &["push", "--quiet", "origin", "main"]);
    let ch = w.memory_edit(4096);
    let done =
        w.wc.transact("kp-soft", &ch, "m [feat-stacks-2]", |_| Vec::new())
            .unwrap();
    let parent = git(
        &w.bare,
        &["log", "-1", "--format=%s", &format!("{}^", done.commit)],
    );
    assert_eq!(parent.trim(), "readme [meta]");

    // A file changed by hand in the working copy.
    std::fs::write(w.wc.repo.join("README.md"), "hand\n").unwrap();
    let e =
        w.wc.transact("kp-soft", &w.memory_edit(5120), "m [feat-stacks-2]", |_| {
            Vec::new()
        })
        .unwrap_err();
    assert!(e.why.contains("differ from the last commit"), "{}", e.why);
    git(&w.wc.repo, &["checkout", "--", "README.md"]);

    // Both moved: a local commit and a remote one.
    std::fs::write(w.wc.repo.join("stacks/admin/extra.yml"), "a: 1\n").unwrap();
    git(&w.wc.repo, &["add", "."]);
    git(&w.wc.repo, &["commit", "--quiet", "-m", "local [meta]"]);
    std::fs::write(w.seed.join("README.md"), "homelab v3\n").unwrap();
    git(&w.seed, &["pull", "--quiet", "--ff-only"]);
    std::fs::write(w.seed.join("README.md"), "homelab v3\n").unwrap();
    git(&w.seed, &["commit", "--quiet", "-am", "again [meta]"]);
    git(&w.seed, &["push", "--quiet", "origin", "main"]);
    let e =
        w.wc.transact("kp-soft", &w.memory_edit(6144), "m [feat-stacks-2]", |_| {
            Vec::new()
        })
        .unwrap_err();
    assert!(e.why.contains("both moved"), "{}", e.why);
    assert_eq!(w.wc.status().unpushed.len(), 1);
    // The panel's choice: rebase, which also pushes.
    w.wc.resolve(Unpushed::Rebase).unwrap();
    assert_eq!(w.remote_subject(), "local [meta]");
    assert!(w.wc.status().unpushed.is_empty());
}

#[test]
fn arch_edit_txn_an_unpushed_commit_is_pushed_or_dropped_on_request() {
    let w = World::new("unpushed");
    w.wc.sync().unwrap();
    std::fs::write(w.wc.repo.join("stacks/admin/extra.yml"), "a: 1\n").unwrap();
    git(&w.wc.repo, &["add", "."]);
    git(&w.wc.repo, &["commit", "--quiet", "-m", "local one [meta]"]);
    let e =
        w.wc.transact("kp-soft", &w.memory_edit(3072), "m [feat-stacks-2]", |_| {
            Vec::new()
        })
        .unwrap_err();
    assert!(e.why.contains("not on the remote yet"), "{}", e.why);
    w.wc.resolve(Unpushed::Drop).unwrap();
    assert!(!w.wc.repo.join("stacks/admin/extra.yml").exists());
    std::fs::write(w.wc.repo.join("stacks/admin/extra.yml"), "a: 2\n").unwrap();
    git(&w.wc.repo, &["add", "."]);
    git(&w.wc.repo, &["commit", "--quiet", "-m", "local two [meta]"]);
    w.wc.resolve(Unpushed::Push).unwrap();
    assert_eq!(w.remote_subject(), "local two [meta]");
}

/// Validation problems and a plan made on files that moved since are
/// refused with nothing written.
#[test]
fn arch_edit_txn_refusals_write_nothing() {
    let w = World::new("refuse");
    w.wc.sync().unwrap();
    let before = w.remote_head();
    let ch = w.memory_edit(3072);
    let e =
        w.wc.transact("kp-soft", &ch, "m [feat-stacks-2]", |dir| {
            assert!(dir.join("lxc-compose.yml").exists(), "the staged copy");
            vec!["memory is wrong".into()]
        })
        .unwrap_err();
    assert!(e.why.contains("memory is wrong"));
    let stale = vec![FileChange {
        old: Some("not what is there\n".into()),
        ..ch[0].clone()
    }];
    let e =
        w.wc.transact("kp-soft", &stale, "m [feat-stacks-2]", |_| Vec::new())
            .unwrap_err();
    assert!(e.why.contains("changed since the plan"), "{}", e.why);
    assert_eq!(w.remote_head(), before);
    assert!(w.wc.status().dirty.is_empty());
}

fn receive_pack_wrapper(w: &World, body: &str) {
    let script = w.root.join("receive-pack.sh");
    std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(
        &w.wc.repo,
        &[
            "config",
            "remote.origin.receivepack",
            script.to_str().unwrap(),
        ],
    );
}

/// A push the remote refused: `ls-remote` shows it is not there, and the
/// working copy goes back to where it was.
#[test]
fn arch_edit_txn_a_refused_push_is_undone_locally() {
    let w = World::new("refused-push");
    w.wc.sync().unwrap();
    let before = w.remote_head();
    let hook = w.bare.join("hooks/pre-receive");
    std::fs::write(&hook, "#!/bin/sh\necho 'protected branch' >&2\nexit 1\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let e =
        w.wc.transact("kp-soft", &w.memory_edit(3072), "m [feat-stacks-2]", |_| {
            Vec::new()
        })
        .unwrap_err();
    assert!(e.why.contains("not on the remote"), "{}", e.why);
    assert!(e.why.contains("protected branch"), "{}", e.why);
    assert_eq!(w.remote_head(), before);
    let status = w.wc.status();
    assert_eq!(status.head.unwrap().commit, before);
    assert!(status.unpushed.is_empty() && status.dirty.is_empty());
}

/// A push that reported an error after the remote took it: `ls-remote`
/// finds the commit there, so it counts as pushed and nothing is reset.
#[test]
fn arch_edit_txn_a_push_error_that_landed_counts_as_pushed() {
    let w = World::new("landed");
    w.wc.sync().unwrap();
    receive_pack_wrapper(&w, "git-receive-pack \"$@\"\nexit 1");
    let done =
        w.wc.transact("kp-soft", &w.memory_edit(3072), "m [feat-stacks-2]", |_| {
            Vec::new()
        })
        .unwrap();
    assert!(done.pushed && done.landed_despite_error);
    assert_eq!(w.remote_head(), done.commit);
}

/// When neither the push nor `ls-remote` answers, nobody can tell: the
/// commit stays here as unpushed, for the panel's push, rebase or drop.
#[test]
fn arch_edit_txn_an_unknown_push_keeps_the_commit_unpushed() {
    let w = World::new("unknown");
    w.wc.sync().unwrap();
    let before = w.remote_head();
    // The push fails without touching the remote, and from then on the
    // remote does not answer ls-remote either.
    let repo = w.wc.repo.display().to_string();
    receive_pack_wrapper(
        &w,
        &format!("GIT_CONFIG_GLOBAL=/dev/null git -C '{repo}' config remote.origin.uploadpack /bin/false\nexit 1"),
    );
    let e =
        w.wc.transact("kp-soft", &w.memory_edit(3072), "m [feat-stacks-2]", |_| {
            Vec::new()
        })
        .unwrap_err();
    assert!(e.why.contains("unknown"), "{}", e.why);
    assert_eq!(w.remote_head(), before);
    git(
        &w.wc.repo,
        &["config", "--unset", "remote.origin.uploadpack"],
    );
    git(
        &w.wc.repo,
        &["config", "--unset", "remote.origin.receivepack"],
    );
    assert_eq!(w.wc.status().unpushed.len(), 1);
    w.wc.resolve(Unpushed::Push).unwrap();
    assert_ne!(w.remote_head(), before);
}

/// No credential in a URL reaches an error body.
#[test]
fn arch_edit_txn_errors_never_carry_a_credential() {
    let e = scrub(
        "fatal: unable to access 'https://x-access-token:ghs_SECRET@github.com/k/h.git/': 403",
    );
    assert!(!e.contains("ghs_SECRET"), "{e}");
    assert!(e.contains("https://***@github.com/k/h.git"), "{e}");
    assert_eq!(scrub("git@github.com:k/h.git"), "git@github.com:k/h.git");
}

/// With an ssh remote and no deploy key, nothing is cloned and the reason
/// says where the key belongs.
#[test]
fn arch_edit_txn_without_the_deploy_key_nothing_is_tried() {
    let root = std::env::temp_dir().join(format!("homelab-edit-txn-{}-nokey", std::process::id()));
    let wc = WorkingCopy::new(
        root.join("repo"),
        GitConfig {
            remote: "git@github.com:kennypassenier/homelab.git".into(),
            branch: "main".into(),
            key: root.join("deploy_key"),
            known_hosts: root.join("known_hosts"),
            author_name: "a".into(),
            author_email: "a@b.c".into(),
        },
        root.join("tmp"),
        Arc::new(|| 0),
    );
    let e = wc.sync().unwrap_err();
    assert!(e.why.contains("no deploy key"), "{}", e.why);
    assert!(!root.join("repo").exists());
    assert_eq!(wc.status().key_present, Some(false));
}
