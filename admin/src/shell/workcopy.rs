//! arch-edit-txn: the dashboard's working copy of the homelab repository
//! (arch-state: `…/admin-config/repo`), and the one transaction every edit
//! goes through:
//!
//! fetch + fast-forward (refused otherwise) → validate with homelab-core →
//! write → commit (only paths under `stacks/<that stack>/`, checked on the
//! staged set) → push; when the push reports an error, `git ls-remote`
//! decides whether it landed before anything local is reset.
//!
//! One edit at a time: the transaction holds a lock that the action queue's
//! reads take too, so a deploy never reads a half-written stack.
//!
//! arch-push-credential: the remote is ssh with the deploy key
//! (`GIT_SSH_COMMAND`), so no token ever sits in a remote URL or in an error
//! body. The key is provisioned from latch (`HOMELAB_ADMIN_DEPLOY_KEY_B64`,
//! written to its file once at start by [`provision_credentials`]); the
//! transaction only hands its path to ssh.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command as Proc;
use std::sync::Mutex;

use serde::Serialize;

use crate::core::actions::Refusal;
use crate::core::actions_config::GitConfig;
use crate::core::stackedit::{outside_stack, FileChange, StackTexts, RAW_MAX};

/// What the status panel shows.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RepoStatus {
    pub present: bool,
    pub remote: String,
    pub branch: String,
    pub head: Option<CommitRef>,
    /// Commits here that the remote does not have.
    pub unpushed: Vec<CommitRef>,
    /// Commits on the remote this copy has not taken yet (after the last
    /// fetch).
    pub behind: usize,
    /// Files that differ from HEAD (should be none: the dashboard only
    /// writes inside a transaction).
    pub dirty: Vec<String>,
    /// Whether the deploy key file is there (ssh remotes only).
    pub key_present: Option<bool>,
    /// Unix seconds of the last successful fetch.
    pub fetched_at: Option<i64>,
    /// Why the last fetch or clone failed.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitRef {
    pub commit: String,
    pub subject: String,
    pub at: i64,
}

/// What a transaction ends in.
#[derive(Debug, Clone, Serialize)]
pub struct Committed {
    pub commit: String,
    pub subject: String,
    /// The remote has the commit.
    pub pushed: bool,
    /// The push reported an error and `ls-remote` showed it landed anyway.
    pub landed_despite_error: bool,
}

/// What the unpushed-commit choice may be (arch-edit-txn).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unpushed {
    Push,
    Rebase,
    Drop,
}

pub struct WorkingCopy {
    pub repo: PathBuf,
    pub git: GitConfig,
    scratch: PathBuf,
    lock: Mutex<()>,
    status: Mutex<RepoStatus>,
    clock: std::sync::Arc<dyn Fn() -> i64 + Send + Sync>,
}

fn refusal(what: &str, why: impl Into<String>, fix: impl Into<String>) -> Refusal {
    Refusal::new(what, why, fix)
}

/// A git error with anything that looks like a credential in a URL masked
/// (`scheme://user:secret@host` → `scheme://***@host`), so no error body
/// can carry one.
pub fn scrub(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(a) => {
                out.push_str("***");
                out.push_str(&tail[a..end]);
            }
            None => out.push_str(&tail[..end]),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out.lines()
        .take(12)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

impl WorkingCopy {
    pub fn new(
        repo: PathBuf,
        git: GitConfig,
        scratch: PathBuf,
        clock: std::sync::Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        let status = RepoStatus {
            remote: display_remote(&git.remote),
            branch: git.branch.clone(),
            ..Default::default()
        };
        WorkingCopy {
            repo,
            git,
            scratch,
            lock: Mutex::new(()),
            status: Mutex::new(status),
            clock,
        }
    }

    /// Whether the remote is reached over ssh (and so needs the key).
    pub fn is_ssh(&self) -> bool {
        is_ssh(&self.git.remote)
    }

    /// Hold the working copy still (reads by the action queue).
    pub fn hold(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn cmd(&self, dir: Option<&Path>) -> Proc {
        let mut c = Proc::new("git");
        if let Some(d) = dir {
            c.arg("-C").arg(d);
        }
        c.env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("GIT_AUTHOR_NAME", &self.git.author_name)
            .env("GIT_AUTHOR_EMAIL", &self.git.author_email)
            .env("GIT_COMMITTER_NAME", &self.git.author_name)
            .env("GIT_COMMITTER_EMAIL", &self.git.author_email)
            // No workstation setting may change what the dashboard writes.
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if self.is_ssh() {
            c.env(
                "GIT_SSH_COMMAND",
                format!(
                    "ssh -i {} -o IdentitiesOnly=yes -o BatchMode=yes -o UserKnownHostsFile={} -o StrictHostKeyChecking=yes -o ConnectTimeout=20",
                    shell_quote(&self.git.key.display().to_string()),
                    shell_quote(&self.git.known_hosts.display().to_string()),
                ),
            );
        }
        c
    }

    fn run(&self, dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
        let out = self
            .cmd(dir)
            .args(args)
            .output()
            .map_err(|e| format!("git did not run: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            let msg = if err.trim().is_empty() {
                String::from_utf8_lossy(&out.stdout).into_owned()
            } else {
                err.into_owned()
            };
            Err(scrub(&msg))
        }
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        self.run(Some(&self.repo), args)
    }

    pub fn present(&self) -> bool {
        self.repo.join(".git").exists() && self.repo.join("stacks").is_dir()
    }

    fn origin(&self) -> String {
        format!("origin/{}", self.git.branch)
    }

    fn set_error(&self, e: Option<String>) {
        if let Ok(mut s) = self.status.lock() {
            s.error = e;
        }
    }

    /// Clone when there is no working copy yet. Called at start and before
    /// every transaction.
    fn ensure(&self) -> Result<(), Refusal> {
        const WHAT: &str = "the working copy";
        if self.is_ssh() && !self.git.key.is_file() {
            return Err(refusal(
                WHAT,
                format!("there is no deploy key at {}", self.git.key.display()),
                "set HOMELAB_ADMIN_DEPLOY_KEY_B64 (the key file, base64) in admin.env through latch and restart the dashboard: it writes the key there (mode 0600) and GitHub's host keys to HOMELAB_ADMIN_GIT_KNOWN_HOSTS",
            ));
        }
        if self.present() {
            let url = self
                .git(&["remote", "get-url", "origin"])
                .unwrap_or_default();
            if url.trim() != self.git.remote {
                self.git(&["remote", "set-url", "origin", &self.git.remote])
                    .map_err(|e| {
                        refusal(
                            WHAT,
                            format!("the remote could not be set: {e}"),
                            "check HOMELAB_ADMIN_GIT_REMOTE",
                        )
                    })?;
            }
            return Ok(());
        }
        if self.repo.exists()
            && std::fs::read_dir(&self.repo)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        {
            return Err(refusal(
                WHAT,
                format!(
                    "{} exists and is not a clone of the homelab repository",
                    self.repo.display()
                ),
                "move it away; the dashboard clones the repository there itself",
            ));
        }
        if let Some(parent) = self.repo.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let dest = self.repo.display().to_string();
        self.run(
            None,
            &[
                "clone",
                "--quiet",
                "--branch",
                &self.git.branch,
                "--",
                &self.git.remote,
                &dest,
            ],
        )
        .map_err(|e| {
            refusal(
                WHAT,
                format!(
                    "the clone of {} failed: {e}",
                    display_remote(&self.git.remote)
                ),
                "check the deploy key, the known hosts and that the remote answers",
            )
        })?;
        Ok(())
    }

    /// Fetch, and fast-forward when this copy is only behind. Refused when
    /// this copy has commits the remote lacks, when both moved, or when a
    /// file differs from HEAD.
    fn sync_locked(&self) -> Result<(), Refusal> {
        const WHAT: &str = "the working copy";
        self.ensure()?;
        self.git(&["fetch", "--quiet", "--prune", "origin", &self.git.branch])
            .map_err(|e| {
                refusal(
                    WHAT,
                    format!("the fetch failed: {e}"),
                    "check that the remote answers; nothing was changed",
                )
            })?;
        if let Ok(mut s) = self.status.lock() {
            s.fetched_at = Some((self.clock)());
        }
        let dirty = self.dirty();
        if !dirty.is_empty() {
            return Err(refusal(
                WHAT,
                format!("files differ from the last commit: {}", dirty.join(", ")),
                "the dashboard only writes inside a transaction; look at the working copy by hand",
            ));
        }
        let (ahead, behind) = self.counts();
        match (ahead, behind) {
            (0, 0) => Ok(()),
            (0, _) => self
                .git(&["merge", "--quiet", "--ff-only", &self.origin()])
                .map(|_| ())
                .map_err(|e| {
                    refusal(
                        WHAT,
                        format!("the fast-forward failed: {e}"),
                        "look at the working copy by hand",
                    )
                }),
            (_, 0) => Err(refusal(
                WHAT,
                format!("{ahead} commit(s) here are not on the remote yet"),
                "push, rebase or drop them on the working copy panel first",
            )),
            _ => Err(refusal(
                WHAT,
                format!("this copy and the remote both moved ({ahead} here, {behind} there)"),
                "rebase or drop the local commits on the working copy panel",
            )),
        }
    }

    /// (commits here the remote lacks, commits there this copy lacks).
    fn counts(&self) -> (usize, usize) {
        let range = format!("HEAD...{}", self.origin());
        self.git(&["rev-list", "--left-right", "--count", &range])
            .ok()
            .and_then(|s| {
                let mut p = s.split_whitespace();
                Some((p.next()?.parse().ok()?, p.next()?.parse().ok()?))
            })
            .unwrap_or((0, 0))
    }

    fn dirty(&self) -> Vec<String> {
        self.git(&["status", "--porcelain", "--untracked-files=all"])
            .map(|s| {
                s.lines()
                    .filter(|l| l.len() > 3)
                    .map(|l| l[3..].to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn head(&self) -> Option<CommitRef> {
        self.git(&["log", "-1", "--format=%H%x09%ct%x09%s"])
            .ok()
            .and_then(|l| parse_commit_line(l.trim()))
    }

    /// Fetch and fast-forward now (the status panel's button, the start,
    /// and before a plan).
    pub fn sync(&self) -> Result<(), Refusal> {
        let _g = self.hold();
        let r = self.sync_locked();
        self.set_error(r.as_ref().err().map(|e| e.why.clone()));
        r
    }

    /// The status, read now (without fetching).
    pub fn status(&self) -> RepoStatus {
        let _g = self.hold();
        let mut s = self.status.lock().map(|s| s.clone()).unwrap_or_default();
        s.present = self.present();
        s.key_present = self.is_ssh().then(|| self.git.key.is_file());
        if s.present {
            s.head = self.head();
            let range = format!("{}..HEAD", self.origin());
            s.unpushed = self
                .git(&["log", "--format=%H%x09%ct%x09%s", &range])
                .map(|t| t.lines().filter_map(parse_commit_line).collect())
                .unwrap_or_default();
            s.behind = self.counts().1;
            s.dirty = self.dirty();
        }
        s
    }

    /// Every text file of a stack, by path inside its directory; secrets
    /// (`.env`) and files too large for the editor are left out.
    pub fn stack_texts(&self, stack: &str) -> Result<StackTexts, Refusal> {
        let _g = self.hold();
        let dir = self.repo.join("stacks").join(stack);
        if !dir.is_dir() {
            return Err(refusal(
                &format!("the files of {stack}"),
                format!("the working copy has no stacks/{stack}"),
                "the fleet page lists the stacks the repository holds",
            ));
        }
        Ok(read_texts(&dir))
    }

    /// The names of every stack directory.
    pub fn stack_names(&self) -> Vec<String> {
        let _g = self.hold();
        let mut out: Vec<String> = std::fs::read_dir(self.repo.join("stacks"))
            .map(|d| {
                d.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    /// Where a scratch tree is made for a validation, and removed again.
    pub fn scratch(&self) -> &Path {
        &self.scratch
    }

    /// A copy of `stacks/<stack>` with the changes laid over it, in a fresh
    /// scratch directory; `f` looks at it, then it is removed.
    pub fn with_staged<T>(
        &self,
        stack: &str,
        changes: &[FileChange],
        f: impl FnOnce(&Path) -> T,
    ) -> Result<T, Refusal> {
        let _g = self.hold();
        self.with_staged_locked(stack, changes, f)
    }

    fn with_staged_locked<T>(
        &self,
        stack: &str,
        changes: &[FileChange],
        f: impl FnOnce(&Path) -> T,
    ) -> Result<T, Refusal> {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = self.scratch.join(format!(
            "stage-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let fail = |why: String| {
            refusal(
                "the validation",
                why,
                "report this with the dashboard's log",
            )
        };
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("stacks").join(stack);
        std::fs::create_dir_all(&dir)
            .map_err(|e| fail(format!("scratch {}: {e}", dir.display())))?;
        let src = self.repo.join("stacks").join(stack);
        if src.is_dir() {
            copy_tree(&src, &dir).map_err(|e| fail(format!("copy of stacks/{stack}: {e}")))?;
        }
        write_changes(&base, changes).map_err(fail)?;
        let out = f(&dir);
        let _ = std::fs::remove_dir_all(&base);
        Ok(out)
    }

    /// The transaction (module doc). `validate` sees the staged stack
    /// directory and answers the problems that stop the commit.
    pub fn transact(
        &self,
        stack: &str,
        changes: &[FileChange],
        message: &str,
        validate: impl FnOnce(&Path) -> Vec<String>,
    ) -> Result<Committed, Refusal> {
        let what = format!("the commit to stacks/{stack}");
        let _g = self.hold();
        let synced = self.sync_locked();
        self.set_error(synced.as_ref().err().map(|e| e.why.clone()));
        synced?;
        if changes.is_empty() {
            return Err(refusal(&what, "nothing changes", "change something first"));
        }
        // arch-push-credential: every path under stacks/<stack>/.
        let outside = outside_stack(stack, changes.iter().map(|c| c.path.as_str()));
        if !outside.is_empty() {
            return Err(refusal(
                &what,
                format!(
                    "the edit touches {} outside stacks/{stack}/",
                    outside.join(", ")
                ),
                "the dashboard commits only under the stack it edits",
            ));
        }
        // The files must still be what the plan was made on.
        for c in changes {
            let now = std::fs::read_to_string(self.repo.join(&c.path)).ok();
            if now != c.old {
                return Err(refusal(
                    &what,
                    format!("{} changed since the plan was made", c.path),
                    "make the plan again; it will start from the newest files",
                ));
            }
        }
        let problems = self.with_staged_locked(stack, changes, validate)?;
        if !problems.is_empty() {
            return Err(refusal(
                &what,
                problems.join("; "),
                "correct the edit; nothing was written",
            ));
        }
        let before = self
            .git(&["rev-parse", "HEAD"])
            .map_err(|e| refusal(&what, e, "look at the working copy by hand"))?
            .trim()
            .to_string();
        let undo = |me: &Self| {
            let _ = me.git(&["reset", "--quiet", "--hard", &before]);
            let _ = me.git(&["clean", "-fdq", "--", &format!("stacks/{stack}")]);
        };
        if let Err(e) = write_changes(&self.repo, changes) {
            undo(self);
            return Err(refusal(
                &what,
                format!("writing failed: {e}"),
                "nothing was committed",
            ));
        }
        let dir = format!("stacks/{stack}");
        if let Err(e) = self.git(&["add", "--all", "--", &dir]) {
            undo(self);
            return Err(refusal(
                &what,
                format!("git add failed: {e}"),
                "nothing was committed",
            ));
        }
        let staged: Vec<String> = self
            .git(&["diff", "--cached", "--name-only", "-z"])
            .unwrap_or_default()
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        let outside = outside_stack(stack, staged.iter().map(String::as_str));
        if staged.is_empty() || !outside.is_empty() {
            undo(self);
            return Err(refusal(
                &what,
                if staged.is_empty() {
                    "nothing was staged".to_string()
                } else {
                    format!(
                        "the staged set holds {} outside stacks/{stack}/",
                        outside.join(", ")
                    )
                },
                "nothing was committed",
            ));
        }
        let _ = std::fs::create_dir_all(&self.scratch);
        let msg_file = self
            .scratch
            .join(format!("commit-msg-{}", std::process::id()));
        if let Err(e) = std::fs::write(&msg_file, message) {
            undo(self);
            return Err(refusal(
                &what,
                format!("the message file: {e}"),
                "nothing was committed",
            ));
        }
        let committed = self.git(&["commit", "--quiet", "-F", &msg_file.display().to_string()]);
        let _ = std::fs::remove_file(&msg_file);
        if let Err(e) = committed {
            undo(self);
            return Err(refusal(
                &what,
                format!("git commit failed: {e}"),
                "nothing was committed",
            ));
        }
        let head = self.head().ok_or_else(|| {
            refusal(
                &what,
                "the new commit could not be read",
                "look at the working copy by hand",
            )
        })?;
        match self.push_locked() {
            Ok(landed_despite_error) => Ok(Committed {
                commit: head.commit,
                subject: head.subject,
                pushed: true,
                landed_despite_error,
            }),
            Err(PushFail::Refused(e)) => {
                // ls-remote said the remote does not have it: this copy
                // goes back to where it was.
                undo(self);
                Err(refusal(
                    &what,
                    format!("the push was refused and the commit is not on the remote: {e}"),
                    "the working copy is back where it was; make the plan again",
                ))
            }
            Err(PushFail::Unknown(e)) => Err(refusal(
                &what,
                format!("the push failed and whether it landed is unknown: {e}"),
                format!(
                    "commit {} stays here, unpushed; the working copy panel offers push, rebase or drop",
                    &head.commit[..12.min(head.commit.len())]
                ),
            )),
        }
    }

    /// fix-110: `transact`'s single-file twin — `config/host.toml`'s
    /// declarative commit. `transact` guards every staged path under
    /// `stacks/<stack>/`; here there is exactly one path, named by the
    /// caller, so the same push/undo machinery applies without that guard.
    /// `validate` sees the new text and answers the problems that stop the
    /// commit (shape only — the host's own apply still runs its own
    /// cross-field `startup_problems` when it writes the file, since only
    /// the host knows every rule).
    pub fn transact_file(
        &self,
        change: &FileChange,
        message: &str,
        validate: impl FnOnce(&str) -> Vec<String>,
    ) -> Result<Committed, Refusal> {
        let what = format!("the commit to {}", change.path);
        let _g = self.hold();
        let synced = self.sync_locked();
        self.set_error(synced.as_ref().err().map(|e| e.why.clone()));
        synced?;
        let Some(new_text) = &change.new else {
            return Err(refusal(&what, "nothing changes", "change something first"));
        };
        let now = std::fs::read_to_string(self.repo.join(&change.path)).ok();
        if now != change.old {
            return Err(refusal(
                &what,
                format!("{} changed since the plan was made", change.path),
                "make the plan again; it will start from the newest file",
            ));
        }
        let problems = validate(new_text);
        if !problems.is_empty() {
            return Err(refusal(
                &what,
                problems.join("; "),
                "correct the edit; nothing was written",
            ));
        }
        let before = self
            .git(&["rev-parse", "HEAD"])
            .map_err(|e| refusal(&what, e, "look at the working copy by hand"))?
            .trim()
            .to_string();
        let undo = |me: &Self| {
            let _ = me.git(&["reset", "--quiet", "--hard", &before]);
            let _ = me.git(&["clean", "-fdq", "--", &change.path]);
        };
        let changes = std::slice::from_ref(change);
        if let Err(e) = write_changes(&self.repo, changes) {
            undo(self);
            return Err(refusal(
                &what,
                format!("writing failed: {e}"),
                "nothing was committed",
            ));
        }
        if let Err(e) = self.git(&["add", "--", &change.path]) {
            undo(self);
            return Err(refusal(
                &what,
                format!("git add failed: {e}"),
                "nothing was committed",
            ));
        }
        let staged: Vec<String> = self
            .git(&["diff", "--cached", "--name-only", "-z"])
            .unwrap_or_default()
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        if staged != [change.path.clone()] {
            undo(self);
            return Err(refusal(
                &what,
                if staged.is_empty() {
                    "nothing was staged".to_string()
                } else {
                    format!("the staged set holds {} instead", staged.join(", "))
                },
                "nothing was committed",
            ));
        }
        let _ = std::fs::create_dir_all(&self.scratch);
        let msg_file = self
            .scratch
            .join(format!("commit-msg-{}", std::process::id()));
        if let Err(e) = std::fs::write(&msg_file, message) {
            undo(self);
            return Err(refusal(
                &what,
                format!("the message file: {e}"),
                "nothing was committed",
            ));
        }
        let committed = self.git(&["commit", "--quiet", "-F", &msg_file.display().to_string()]);
        let _ = std::fs::remove_file(&msg_file);
        if let Err(e) = committed {
            undo(self);
            return Err(refusal(
                &what,
                format!("git commit failed: {e}"),
                "nothing was committed",
            ));
        }
        let head = self.head().ok_or_else(|| {
            refusal(
                &what,
                "the new commit could not be read",
                "look at the working copy by hand",
            )
        })?;
        match self.push_locked() {
            Ok(landed_despite_error) => Ok(Committed {
                commit: head.commit,
                subject: head.subject,
                pushed: true,
                landed_despite_error,
            }),
            Err(PushFail::Refused(e)) => {
                undo(self);
                Err(refusal(
                    &what,
                    format!("the push was refused and the commit is not on the remote: {e}"),
                    "the working copy is back where it was; make the plan again",
                ))
            }
            Err(PushFail::Unknown(e)) => Err(refusal(
                &what,
                format!("the push failed and whether it landed is unknown: {e}"),
                format!(
                    "commit {} stays here, unpushed; the working copy panel offers push, rebase or drop",
                    &head.commit[..12.min(head.commit.len())]
                ),
            )),
        }
    }

    /// Every text file of `presets/`, keyed `<preset>/<rel>` —
    /// `stack_texts`'s twin for the presets editor (feat-preset-1).
    pub fn preset_texts(&self) -> crate::core::presetedit::PresetTexts {
        let _g = self.hold();
        read_texts(&self.repo.join("presets"))
    }

    /// A copy of `presets/` with `changes` laid over it, in a fresh scratch
    /// directory; `f` looks at it, then it is removed. `with_staged`'s
    /// twin, scoped to the whole presets tree rather than one stack.
    pub fn with_staged_presets<T>(
        &self,
        changes: &[FileChange],
        f: impl FnOnce(&Path) -> T,
    ) -> Result<T, Refusal> {
        let _g = self.hold();
        self.with_staged_presets_locked(changes, f)
    }

    /// `with_staged_presets` without taking the lock itself — for
    /// `transact_presets`, which already holds it (`with_staged`/
    /// `with_staged_locked`'s split, mirrored here: calling the public,
    /// lock-taking `with_staged_presets` from inside `transact_presets`
    /// would deadlock on `self.hold()`, a plain `std::sync::Mutex` that is
    /// not reentrant).
    fn with_staged_presets_locked<T>(
        &self,
        changes: &[FileChange],
        f: impl FnOnce(&Path) -> T,
    ) -> Result<T, Refusal> {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = self.scratch.join(format!(
            "stage-presets-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let fail = |why: String| {
            refusal(
                "the validation",
                why,
                "report this with the dashboard's log",
            )
        };
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("presets");
        std::fs::create_dir_all(&dir)
            .map_err(|e| fail(format!("scratch {}: {e}", dir.display())))?;
        let src = self.repo.join("presets");
        if src.is_dir() {
            copy_tree(&src, &dir).map_err(|e| fail(format!("copy of presets: {e}")))?;
        }
        write_changes(&base, changes).map_err(fail)?;
        let out = f(&dir);
        let _ = std::fs::remove_dir_all(&base);
        Ok(out)
    }

    /// The transaction for `presets/` — `transact`'s twin, committing under
    /// `presets/` as a whole instead of one stack's directory.
    pub fn transact_presets(
        &self,
        changes: &[FileChange],
        message: &str,
        validate: impl FnOnce(&Path) -> Vec<String>,
    ) -> Result<Committed, Refusal> {
        let what = "the commit to presets/".to_string();
        let _g = self.hold();
        let synced = self.sync_locked();
        self.set_error(synced.as_ref().err().map(|e| e.why.clone()));
        synced?;
        if changes.is_empty() {
            return Err(refusal(&what, "nothing changes", "change something first"));
        }
        let outside =
            crate::core::presetedit::outside_presets(changes.iter().map(|c| c.path.as_str()));
        if !outside.is_empty() {
            return Err(refusal(
                &what,
                format!("the edit touches {} outside presets/", outside.join(", ")),
                "the dashboard commits only under presets/ here",
            ));
        }
        for c in changes {
            let now = std::fs::read_to_string(self.repo.join(&c.path)).ok();
            if now != c.old {
                return Err(refusal(
                    &what,
                    format!("{} changed since the plan was made", c.path),
                    "make the plan again; it will start from the newest files",
                ));
            }
        }
        let problems = self.with_staged_presets_locked(changes, validate)?;
        if !problems.is_empty() {
            return Err(refusal(
                &what,
                problems.join("; "),
                "correct the edit; nothing was written",
            ));
        }
        let before = self
            .git(&["rev-parse", "HEAD"])
            .map_err(|e| refusal(&what, e, "look at the working copy by hand"))?
            .trim()
            .to_string();
        let undo = |me: &Self| {
            let _ = me.git(&["reset", "--quiet", "--hard", &before]);
            let _ = me.git(&["clean", "-fdq", "--", "presets"]);
        };
        if let Err(e) = write_changes(&self.repo, changes) {
            undo(self);
            return Err(refusal(
                &what,
                format!("writing failed: {e}"),
                "nothing was committed",
            ));
        }
        if let Err(e) = self.git(&["add", "--all", "--", "presets"]) {
            undo(self);
            return Err(refusal(
                &what,
                format!("git add failed: {e}"),
                "nothing was committed",
            ));
        }
        let staged: Vec<String> = self
            .git(&["diff", "--cached", "--name-only", "-z"])
            .unwrap_or_default()
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        let outside = crate::core::presetedit::outside_presets(staged.iter().map(String::as_str));
        if staged.is_empty() || !outside.is_empty() {
            undo(self);
            return Err(refusal(
                &what,
                if staged.is_empty() {
                    "nothing was staged".to_string()
                } else {
                    format!(
                        "the staged set holds {} outside presets/",
                        outside.join(", ")
                    )
                },
                "nothing was committed",
            ));
        }
        let _ = std::fs::create_dir_all(&self.scratch);
        let msg_file = self
            .scratch
            .join(format!("commit-msg-presets-{}", std::process::id()));
        if let Err(e) = std::fs::write(&msg_file, message) {
            undo(self);
            return Err(refusal(
                &what,
                format!("the message file: {e}"),
                "nothing was committed",
            ));
        }
        let committed = self.git(&["commit", "--quiet", "-F", &msg_file.display().to_string()]);
        let _ = std::fs::remove_file(&msg_file);
        if let Err(e) = committed {
            undo(self);
            return Err(refusal(
                &what,
                format!("git commit failed: {e}"),
                "nothing was committed",
            ));
        }
        let head = self.head().ok_or_else(|| {
            refusal(
                &what,
                "the new commit could not be read",
                "look at the working copy by hand",
            )
        })?;
        match self.push_locked() {
            Ok(landed_despite_error) => Ok(Committed {
                commit: head.commit,
                subject: head.subject,
                pushed: true,
                landed_despite_error,
            }),
            Err(PushFail::Refused(e)) => {
                undo(self);
                Err(refusal(
                    &what,
                    format!("the push was refused and the commit is not on the remote: {e}"),
                    "the working copy is back where it was; make the plan again",
                ))
            }
            Err(PushFail::Unknown(e)) => Err(refusal(
                &what,
                format!("the push failed and whether it landed is unknown: {e}"),
                format!(
                    "commit {} stays here, unpushed; the working copy panel offers push, rebase or drop",
                    &head.commit[..12.min(head.commit.len())]
                ),
            )),
        }
    }

    /// Push HEAD; on an error ask the remote whether it has HEAD anyway.
    fn push_locked(&self) -> Result<bool, PushFail> {
        let refspec = format!("HEAD:refs/heads/{}", self.git.branch);
        let pushed = self.git(&["push", "--quiet", "origin", &refspec]);
        let Err(err) = pushed else {
            let _ = self.git(&["fetch", "--quiet", "origin", &self.git.branch]);
            return Ok(false);
        };
        let head = self.git(&["rev-parse", "HEAD"]).unwrap_or_default();
        let branch_ref = format!("refs/heads/{}", self.git.branch);
        match self.git(&["ls-remote", "origin", &branch_ref]) {
            Ok(out) => {
                let remote = out.split_whitespace().next().unwrap_or("");
                if !remote.is_empty() && remote == head.trim() {
                    let _ = self.git(&["fetch", "--quiet", "origin", &self.git.branch]);
                    Ok(true)
                } else {
                    Err(PushFail::Refused(err))
                }
            }
            Err(e) => Err(PushFail::Unknown(format!("{err}; ls-remote: {e}"))),
        }
    }

    /// The working copy panel's choice for commits the remote lacks.
    pub fn resolve(&self, choice: Unpushed) -> Result<(), Refusal> {
        const WHAT: &str = "the unpushed commits";
        let _g = self.hold();
        self.ensure()?;
        self.git(&["fetch", "--quiet", "origin", &self.git.branch])
            .map_err(|e| {
                refusal(
                    WHAT,
                    format!("the fetch failed: {e}"),
                    "check that the remote answers",
                )
            })?;
        let (ahead, _) = self.counts();
        if ahead == 0 {
            return Err(refusal(WHAT, "there are none", "nothing to do"));
        }
        match choice {
            Unpushed::Drop => self
                .git(&["reset", "--quiet", "--hard", &self.origin()])
                .map(|_| ())
                .map_err(|e| refusal(WHAT, e, "look at the working copy by hand")),
            Unpushed::Rebase => {
                if let Err(e) = self.git(&["rebase", "--quiet", &self.origin()]) {
                    let _ = self.git(&["rebase", "--abort"]);
                    return Err(refusal(
                        WHAT,
                        format!("the rebase stopped on a conflict and was undone: {e}"),
                        "drop the local commits, or resolve it in a workstation clone",
                    ));
                }
                self.push_resolved()
            }
            Unpushed::Push => self.push_resolved(),
        }
    }

    fn push_resolved(&self) -> Result<(), Refusal> {
        match self.push_locked() {
            Ok(_) => Ok(()),
            Err(PushFail::Refused(e)) => Err(refusal(
                "the unpushed commits",
                format!("the push was refused: {e}"),
                "rebase first, or drop them",
            )),
            Err(PushFail::Unknown(e)) => Err(refusal(
                "the unpushed commits",
                format!("the push failed and whether it landed is unknown: {e}"),
                "try again once the remote answers",
            )),
        }
    }

    /// feat-stacks-3: the files a scaffold writes, read back from a scratch
    /// tree (`make` writes `stacks/<name>` under the base it is given).
    pub fn scaffolded(
        &self,
        name: &str,
        make: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<Vec<FileChange>, Refusal> {
        let base = self
            .scratch
            .join(format!("new-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base)
            .map_err(|e| refusal("the new stack", format!("scratch: {e}"), "report this"))?;
        let made = make(&base);
        let out = made.map(|()| {
            read_texts(&base.join(name))
                .into_iter()
                .map(|(rel, text)| FileChange {
                    path: format!("stacks/{name}/{rel}"),
                    old: None,
                    new: Some(text),
                })
                .collect::<Vec<_>>()
        });
        let _ = std::fs::remove_dir_all(&base);
        out.map_err(|e| refusal("the new stack", e, "pick other values in the wizard"))
    }
}

enum PushFail {
    /// The remote does not have the commit.
    Refused(String),
    /// Nobody can tell.
    Unknown(String),
}

fn parse_commit_line(l: &str) -> Option<CommitRef> {
    let mut p = l.splitn(3, '\t');
    Some(CommitRef {
        commit: p.next()?.to_string(),
        at: p.next()?.parse().ok()?,
        subject: p.next().unwrap_or("").to_string(),
    })
}

pub fn is_ssh(remote: &str) -> bool {
    !(remote.starts_with('/')
        || remote.starts_with("file://")
        || remote.starts_with("./")
        || remote.starts_with("../"))
}

/// The remote as the page shows it: a local path stays a path, an ssh
/// remote shows host and repository.
pub fn display_remote(r: &str) -> String {
    scrub(r)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn is_secret_name(name: &str) -> bool {
    name == ".env" || name.ends_with(".env") || name.starts_with(".env")
}

/// Every text file under `dir` (relative paths), secrets and large or
/// binary files left out.
pub fn read_texts(dir: &Path) -> StackTexts {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name == ".git" {
                continue;
            }
            if p.is_dir() {
                walk(root, &p, out);
                continue;
            }
            if is_secret_name(&name) {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            if meta.len() as usize > RAW_MAX {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            if let Ok(rel) = p.strip_prefix(root) {
                out.insert(rel.to_string_lossy().into_owned(), text);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)?.flatten() {
        let p = e.path();
        let to = dst.join(e.file_name());
        if p.is_dir() {
            copy_tree(&p, &to)?;
        } else if p.is_file() {
            std::fs::copy(&p, &to)?;
        }
    }
    Ok(())
}

/// Write each change under `base` (`base/<change.path>`); a change to None
/// removes the file.
fn write_changes(base: &Path, changes: &[FileChange]) -> Result<(), String> {
    for c in changes {
        if c.path.split('/').any(|p| p == ".." || p.is_empty()) {
            return Err(format!("{} is not a plain path", c.path));
        }
        let p = base.join(&c.path);
        match &c.new {
            Some(text) => {
                if let Some(parent) = p.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", c.path))?;
                }
                std::fs::write(&p, text).map_err(|e| format!("{}: {e}", c.path))?;
            }
            None => {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    Ok(())
}

/// What [`provision_credentials`] did, for the log. Never a secret: paths
/// and verbs only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provisioned {
    /// The key file was written from the environment.
    pub key_written: bool,
    /// The known-hosts file was written with GitHub's pinned host keys.
    pub known_hosts_written: bool,
    /// What could not be done, and why (no secret in it).
    pub problems: Vec<String>,
}

/// arch-push-credential on CT 120: latch hands the deploy key over as
/// `HOMELAB_ADMIN_DEPLOY_KEY_B64`; ssh wants a file. At start, write the key
/// (mode 0600, created new, never over an existing file) and GitHub's pinned
/// host keys, each only when its file is missing. `key_b64` is the
/// variable's value, None when it is not set. Nothing here logs or returns
/// any part of the key.
pub fn provision_credentials(git: &GitConfig, key_b64: Option<&str>) -> Provisioned {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut out = Provisioned::default();
    if !is_ssh(&git.remote) {
        return out;
    }
    if !git.key.exists() {
        if let Some(b64) = key_b64.filter(|v| !v.trim().is_empty()) {
            match crate::core::credentials::decode_deploy_key(b64) {
                Ok(bytes) => {
                    if let Some(parent) = git.key.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let written = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&git.key)
                        .and_then(|mut f| {
                            f.write_all(&bytes)?;
                            f.sync_all()
                        });
                    match written {
                        Ok(()) => out.key_written = true,
                        Err(e) => {
                            let _ = std::fs::remove_file(&git.key);
                            out.problems.push(format!(
                                "the deploy key could not be written to {}: {e}",
                                git.key.display()
                            ));
                        }
                    }
                }
                Err(why) => out.problems.push(why),
            }
        }
    }
    if !git.known_hosts.exists() && crate::core::credentials::is_github_ssh(&git.remote) {
        if let Some(parent) = git.known_hosts.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&git.known_hosts)
            .and_then(|mut f| f.write_all(crate::core::credentials::GITHUB_KNOWN_HOSTS.as_bytes()));
        match written {
            Ok(()) => out.known_hosts_written = true,
            Err(e) => out.problems.push(format!(
                "GitHub's host keys could not be written to {}: {e}",
                git.known_hosts.display()
            )),
        }
    }
    out
}
