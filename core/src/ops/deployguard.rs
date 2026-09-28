//! arch-deploy-guard (homelab-admin, 2026-09-28): a deploy may not undo a
//! deploy made from somewhere else.
//!
//! With the dashboard there are two writers: the CLI on a workstation and
//! CT 120. The host records which commit each deploy came from
//! (`applied_source`, e.g. "a1b2c3d4e5f6 + 1 uncommitted file(s)") but never
//! compared it. A deploy from a tree that has not pulled the other writer's
//! commit silently reverted it. Now the deploying side refuses unless the
//! host's commit is in its own history, and `--force` says "I know".

/// What the repository says about the host's commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ancestry {
    /// The host's commit is HEAD or behind it: deploying adds to it.
    Contained,
    /// Both exist, but the host's commit is not behind HEAD.
    Diverged,
    /// This repository does not have the host's commit at all.
    Unknown,
}

/// The commit in an `applied_source` summary: its first word.
pub fn applied_commit(summary: &str) -> Option<&str> {
    let c = summary.split_whitespace().next()?;
    (c.len() >= 7 && c.bytes().all(|b| b.is_ascii_hexdigit())).then_some(c)
}

/// Ok to deploy, or why not, in words the operator can act on.
pub fn decide(
    stack: &str,
    applied: Option<&str>,
    ancestry: Ancestry,
    force: bool,
) -> Result<(), String> {
    let Some(commit) = applied.and_then(applied_commit) else {
        return Ok(()); // never deployed with a source: nothing to protect
    };
    if force || ancestry == Ancestry::Contained {
        return Ok(());
    }
    let why = match ancestry {
        Ancestry::Unknown => format!(
            "this repository does not have commit {commit}, which the host runs for {stack}"
        ),
        _ => format!(
            "the host runs {stack} from commit {commit}, which is not in this branch's history"
        ),
    };
    Err(format!(
        "refused: {why}. A deploy from here would undo it. Run `git pull` first, \
         or deploy with --force if undoing it is what you want."
    ))
}
