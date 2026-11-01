//! key-move: the working copy's commit, handed to the host.
//!
//! The transaction (`WorkingCopy::transact` and its twins) has synced and
//! validated; instead of writing, committing and pushing here, it sends the
//! files and the message to the host as `Command::CommitStackFiles`, with
//! this copy's HEAD as the base the plan was made on. The host commits and
//! pushes with the key only it holds. This copy then fetches and
//! fast-forwards to the host's commit, so the next plan starts from it.
//! Nothing is written to this copy's files before the host answers, so a
//! refusal or a lost answer leaves it exactly as it was.

use std::sync::Arc;
use std::time::Duration;

use homelab_proto::{Command, RpcResponse, StackCommitDone, StackFileChange};

use super::{Committed, WorkingCopy, refusal};
use crate::core::actions::Refusal;
use crate::core::stackedit::FileChange;
use crate::shell::actions::HostPort;

/// One command to the host and its answer. Called from the transaction,
/// which runs off the async threads (`spawn_blocking`).
pub trait HostCommit: Send + Sync {
    fn send(&self, command: Command) -> Result<RpcResponse, String>;
}

/// [`HostCommit`] over the dashboard's host link.
pub struct ViaHostLink {
    host: Arc<dyn HostPort>,
    runtime: tokio::runtime::Handle,
    timeout: Duration,
}

impl ViaHostLink {
    /// `runtime` is the dashboard's own: the blocking transaction waits on
    /// the link's future there.
    pub fn new(
        host: Arc<dyn HostPort>,
        runtime: tokio::runtime::Handle,
        timeout: Duration,
    ) -> Self {
        ViaHostLink {
            host,
            runtime,
            timeout,
        }
    }
}

impl HostCommit for ViaHostLink {
    fn send(&self, command: Command) -> Result<RpcResponse, String> {
        self.runtime
            .block_on(self.host.ask_traced(command, self.timeout, None))
    }
}

/// The host's refusal, `why :: what to do`, as a dashboard refusal.
fn host_refusal(what: &str, message: &str) -> Refusal {
    match message.split_once(" :: ") {
        Some((why, fix)) => refusal(what, format!("the host refused the commit: {why}"), fix),
        None => refusal(
            what,
            format!("the host refused the commit: {message}"),
            "nothing was written; make the plan again",
        ),
    }
}

impl WorkingCopy {
    /// The host commits `changes` (module doc). The working copy's lock is
    /// held by the caller, and this copy is in step with the remote (the
    /// transaction's sync refused otherwise).
    pub(super) fn commit_via_host(
        &self,
        host: &dyn HostCommit,
        what: &str,
        changes: &[FileChange],
        message: &str,
    ) -> Result<Committed, Refusal> {
        let base = self
            .git(&["rev-parse", "HEAD"])
            .map_err(|e| refusal(what, e, "look at the working copy by hand"))?
            .trim()
            .to_string();
        let files = changes
            .iter()
            .map(|c| StackFileChange {
                path: c.path.clone(),
                content: c.new.clone(),
            })
            .collect();
        let command = Command::CommitStackFiles {
            message: message.to_string(),
            base,
            files,
            push: true,
        };
        let answer = host.send(command).map_err(|e| {
            refusal(
                what,
                format!("the host did not answer, so whether it committed is unknown: {e}"),
                "nothing was written here; the next plan fetches the remote and shows whether the commit arrived",
            )
        })?;
        if !answer.ok {
            return Err(host_refusal(what, &answer.message));
        }
        let done: StackCommitDone = serde_json::from_str(&answer.message).map_err(|e| {
            refusal(
                what,
                format!("the host's answer could not be read: {e}"),
                "the next plan fetches the remote and shows whether the commit arrived",
            )
        })?;
        self.take_host_commit(&done);
        Ok(Committed {
            commit: done.commit,
            subject: done.subject,
            pushed: done.pushed,
            landed_despite_error: done.landed_despite_error,
        })
    }

    /// Fetch and fast-forward to what the host pushed. A failure here does
    /// not undo the commit (it is on the remote): it is recorded on the
    /// status panel, and the next sync tries again.
    fn take_host_commit(&self, done: &StackCommitDone) {
        if let Err(r) = self.sync_locked() {
            self.set_error(Some(format!(
                "the host pushed {} but this copy could not take it yet: {}",
                short(&done.commit),
                r.why
            )));
            return;
        }
        // Someone may have pushed after the host: then HEAD is a descendant.
        let taken = self
            .git(&["merge-base", "--is-ancestor", &done.commit, "HEAD"])
            .is_ok();
        let error = (!taken).then(|| {
            format!(
                "the host pushed {} but this copy does not hold it after the fetch",
                short(&done.commit)
            )
        });
        self.set_error(error);
    }
}

fn short(c: &str) -> &str {
    &c[..12.min(c.len())]
}
