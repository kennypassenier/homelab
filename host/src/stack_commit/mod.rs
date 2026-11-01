//! key-move: the host's side of a dashboard edit — one transaction in the
//! host's own clone of the stacks repository (`[stack_commits]`):
//!
//! clone when missing → refuse a dirty tree or local commits → fetch and
//! fast-forward → the branch must be exactly at the request's `base` →
//! write the files → `git add` only those paths (the staged set is checked
//! against the request) → commit → push `HEAD:refs/heads/<branch>`, never
//! forced; when the push reports an error, `ls-remote` decides whether it
//! landed. Anything that does not end on the remote is undone here, so the
//! clone always matches the remote and the next request starts clean.
//!
//! One at a time per clone: a second request while one runs is refused,
//! not queued (the dashboard already sends one edit at a time).

mod files;
mod git;

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use homelab_core::ops::stackcommit::guard::staged_outside;
use homelab_core::ops::stackcommit::{StackCommitConfig, StackCommitDone, StackFileChange};

use git::Git;

#[cfg(test)]
pub(crate) use files::credentials_ready;

/// One `CommitStackFiles`, after the core guard passed it.
pub(crate) struct Request {
    pub(crate) message: String,
    pub(crate) base: String,
    pub(crate) files: Vec<StackFileChange>,
    pub(crate) push: bool,
}

/// The clones a transaction is running in. The host has one clone; the
/// lock is per clone so that only what shares a clone excludes each other.
static BUSY: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// One running transaction's hold on its clone, given back when dropped.
pub(crate) struct Claim(String);

impl Drop for Claim {
    fn drop(&mut self) {
        BUSY.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.0);
    }
}

/// Hold `dir` for one transaction, or refuse while another holds it.
pub(crate) fn claim(dir: &str) -> Result<Claim, String> {
    let mut busy = BUSY.lock().unwrap_or_else(PoisonError::into_inner);
    if !busy.insert(dir.to_string()) {
        return Err("another stack commit is running on the host :: \
                    send the edit again once it has finished"
            .into());
    }
    Ok(Claim(dir.to_string()))
}

/// The transaction (module doc). Blocking: run it on a blocking thread.
/// The error is `why :: what to do`, the host's refusal style.
pub(crate) fn commit(cfg: &StackCommitConfig, req: &Request) -> Result<StackCommitDone, String> {
    let _held = claim(&cfg.dir)?;
    files::credentials_ready(cfg)?;
    let git = Git::new(cfg);
    prepare(&git, cfg, &req.base)?;
    let head = match apply(&git, req) {
        Ok(h) => h,
        Err(e) => {
            undo(&git, &req.base, &req.files);
            return Err(e);
        }
    };
    finish(&git, cfg, req, head)
}

/// Clone when missing, then bring the clone to exactly `base`.
fn prepare(git: &Git, cfg: &StackCommitConfig, base: &str) -> Result<(), String> {
    ensure_clone(git, cfg)?;
    let dirty = git.run(&["status", "--porcelain", "--untracked-files=all"])?;
    if !dirty.trim().is_empty() {
        return Err(format!(
            "the host's clone ({}) has files that differ from its last commit :: \
             look at it by hand; the host never commits what it did not write",
            cfg.dir
        ));
    }
    git.run(&["fetch", "--quiet", "--prune", "origin", &cfg.branch])
        .map_err(|e| format!("{e} :: check that the remote answers; nothing was changed"))?;
    let origin = format!("origin/{}", cfg.branch);
    let ahead = git.run(&["rev-list", "--count", &format!("{origin}..HEAD")])?;
    if ahead.trim() != "0" {
        return Err(format!(
            "the host's clone holds {} commit(s) the remote lacks :: look at {} by hand",
            ahead.trim(),
            cfg.dir
        ));
    }
    git.run(&["merge", "--quiet", "--ff-only", &origin])
        .map_err(|e| format!("{e} :: look at {} by hand", cfg.dir))?;
    let head = git.rev("HEAD")?;
    if head != base {
        return Err(format!(
            "the remote branch {} is at {} but the edit was planned on {} :: \
             the dashboard fetches the newest files; make the plan again",
            cfg.branch,
            short(&head),
            short(base)
        ));
    }
    Ok(())
}

fn ensure_clone(git: &Git, cfg: &StackCommitConfig) -> Result<(), String> {
    if git.dir.join(".git").exists() {
        let url = git
            .run(&["remote", "get-url", "origin"])
            .unwrap_or_default();
        if url.trim() != cfg.remote {
            git.run(&["remote", "set-url", "origin", &cfg.remote])?;
        }
        return Ok(());
    }
    let occupied = std::fs::read_dir(&git.dir).is_ok_and(|mut d| d.next().is_some());
    if occupied {
        return Err(format!(
            "{} exists and is not a clone :: move it away; the host clones there itself",
            cfg.dir
        ));
    }
    if let Some(parent) = git.dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    git.run_in(
        None,
        &[
            "clone",
            "--quiet",
            "--branch",
            &cfg.branch,
            "--",
            &cfg.remote,
            &cfg.dir,
        ],
    )
    .map(|_| ())
    .map_err(|e| format!("{e} :: check the deploy key, the known hosts and the remote"))
}

/// Write, stage only the request's paths, check the staged set, commit.
/// Answers the new HEAD.
fn apply(git: &Git, req: &Request) -> Result<String, String> {
    files::write_all(&git.dir, &req.files)?;
    let mut add = vec!["add", "--all", "--"];
    add.extend(req.files.iter().map(|f| f.path.as_str()));
    git.run(&add)?;
    let staged: Vec<String> = git
        .run(&["diff", "--cached", "--name-only", "-z"])?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if staged.is_empty() {
        return Err(
            "nothing changes: the files already hold this text :: nothing was committed".into(),
        );
    }
    let outside = staged_outside(&staged, &req.files);
    if !outside.is_empty() {
        return Err(format!(
            "the staged set holds {} the request did not name :: nothing was committed",
            outside.join(", ")
        ));
    }
    let msg_file = format!("{}.commit-msg", git.dir.display());
    std::fs::write(&msg_file, &req.message).map_err(|e| format!("{msg_file}: {e}"))?;
    let committed = git.run(&["commit", "--quiet", "--no-verify", "-F", &msg_file]);
    let _ = std::fs::remove_file(&msg_file);
    committed?;
    git.rev("HEAD")
}

/// Push (or, for a dry run, undo) and say how it ended.
fn finish(
    git: &Git,
    cfg: &StackCommitConfig,
    req: &Request,
    head: String,
) -> Result<StackCommitDone, String> {
    let subject = git
        .run(&["log", "-1", "--format=%s"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let done = |pushed, landed_despite_error| StackCommitDone {
        commit: head.clone(),
        subject: subject.clone(),
        pushed,
        landed_despite_error,
    };
    if !req.push {
        undo(git, &req.base, &req.files);
        return Ok(done(false, false));
    }
    let refspec = format!("HEAD:refs/heads/{}", cfg.branch);
    let Err(err) = git.run(&["push", "--quiet", "origin", &refspec]) else {
        return Ok(done(true, false));
    };
    let branch_ref = format!("refs/heads/{}", cfg.branch);
    let remote = git.run(&["ls-remote", "origin", &branch_ref]);
    if let Ok(out) = &remote
        && out.split_whitespace().next() == Some(head.as_str())
    {
        return Ok(done(true, true));
    }
    undo(git, &req.base, &req.files);
    Err(match remote {
        Ok(_) => format!(
            "the push was refused and the commit is not on the remote: {err} :: \
             nothing changed; make the plan again"
        ),
        Err(e) => format!(
            "the push failed and whether it landed is unknown: {err}; ls-remote: {e} :: \
             the dashboard's next fetch shows whether commit {} arrived",
            short(&head)
        ),
    })
}

/// Back to `base`, and the request's untracked files gone.
fn undo(git: &Git, base: &str, files: &[StackFileChange]) {
    if let Err(e) = git.run(&["reset", "--quiet", "--hard", base]) {
        tracing::warn!(
            "stack commits: the clone could not be reset to {}: {}",
            short(base),
            e
        );
    }
    let mut clean = vec!["clean", "-fdq", "--"];
    clean.extend(files.iter().map(|f| f.path.as_str()));
    if let Err(e) = git.run(&clean) {
        tracing::warn!("stack commits: untracked files could not be removed: {}", e);
    }
}

fn short(c: &str) -> &str {
    &c[..12.min(c.len())]
}
