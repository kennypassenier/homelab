//! key-move: what the host says when its clone holds a commit the remote
//! lacks (the host stopped between `commit` and `push`). Kenny decided
//! "melding, geen herstel": the host keeps refusing, never resets or pushes
//! by itself, and leaves ONE quiet Inbox notice naming the commit and the
//! two commands that settle it.

use crate::*;
use homelab_core::notify::{HostNotice, page};

/// The label the one open notice is found by.
pub(crate) const LABEL: &str = "stack-commit-unpushed";

/// A commit in the clone that is not on the remote branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unpushed {
    pub(crate) hash: String,
    pub(crate) subject: String,
    pub(crate) dir: String,
    pub(crate) branch: String,
}

impl Unpushed {
    /// The two ways out, each runnable as root on the host.
    pub(crate) fn commands(&self) -> [String; 2] {
        [
            format!(
                "cd {} && git push origin HEAD:refs/heads/{}",
                self.dir, self.branch
            ),
            format!("cd {} && git reset --hard origin/{}", self.dir, self.branch),
        ]
    }

    /// The refusal a request gets: `why :: what to do`.
    pub(crate) fn refusal(&self, count: &str) -> String {
        let [push, drop] = self.commands();
        format!(
            "the host's clone holds {count} commit(s) the remote lacks (latest {}: {}) :: \
             as root on the host, either push it with `{push}` or drop it with `{drop}`; \
             the host does neither by itself",
            &self.hash[..12.min(self.hash.len())],
            self.subject
        )
    }

    fn notice(&self, seq: u64, at: u64) -> HostNotice {
        let [push, drop] = self.commands();
        HostNotice {
            seq,
            at,
            since: at,
            op: "stack-commit".into(),
            label: LABEL.into(),
            ok: false,
            deferred: false,
            stack: None,
            title: "A stack commit was never pushed".into(),
            what: format!(
                "the host's clone {} holds commit {} \"{}\" that is not on origin/{}",
                self.dir,
                &self.hash[..12.min(self.hash.len())],
                self.subject,
                self.branch
            ),
            consequence: "The host refuses every dashboard edit until you choose; it never \
                          pushes or resets by itself."
                .into(),
            remedy: format!("As root on the host, either push it: `{push}`, or drop it: `{drop}`."),
            page: page::HOST.into(),
            urgent: false,
            routed: "not pushed: a refusal that waits for a choice".into(),
            push: "centre only".into(),
            incident: None,
            req: None,
            by: None,
            findings: Vec::new(),
        }
    }
}

/// Record the notice unless the same commit already has an open one; a
/// different commit replaces the old notice. Best effort, logged.
pub(crate) fn record(state: &AppState, found: &Unpushed) {
    let dir = &state.config.state_dir;
    let open = match hostdb::for_dir(dir).and_then(|db| hostdb::notices::all(&db)) {
        Ok(all) => all.into_iter().rev().find(|n| n.label == LABEL),
        Err(e) => return tracing::warn!("stack commits: could not read the open notices :: {e}"),
    };
    let fresh = found.notice(notices::next_notice_seq(state), unix_now());
    if open.as_ref().is_some_and(|n| n.what == fresh.what) {
        return;
    }
    match open {
        None => notices::record_notice(state, &fresh),
        Some(prev) => {
            let swapped =
                hostdb::for_dir(dir).and_then(|db| hostdb::notices::replace(&db, prev.seq, &fresh));
            if let Err(e) = swapped {
                tracing::warn!("stack commits: could not replace the notice :: {e}");
            }
        }
    }
}
