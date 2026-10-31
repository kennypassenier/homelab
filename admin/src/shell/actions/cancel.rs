//! feat-batch-1: cancel one job, queued or (a command) running.

use std::time::Duration;

use homelab_proto::Command;

use super::{Actions, CANCEL_EXEC_SINCE, JobState, JobView, Origin};
use crate::core::actions::ActionKind;

impl Actions {
    /// feat-batch-1: cancel ONE queued job (Kenny, 2026-10-09). A job that
    /// runs or has finished is left alone. It leaves the queue as refused
    /// ("cancelled by …"); the `action` event goes out through `store`, and
    /// the batch it belongs to is published again. `Err` says why not.
    pub fn cancel_job(&self, id: u64, who: &str) -> Result<JobView, String> {
        let Some(mut v) = self.job(id) else {
            return Err(format!("this dashboard knows no job {id}"));
        };
        if v.state != JobState::Queued {
            return Err(format!(
                "job {id} is not queued any more, so it cannot be cancelled"
            ));
        }
        v.state = JobState::Refused;
        v.finished_at = Some(self.now());
        v.message = Some(format!("cancelled by {who}; this stack was not touched"));
        self.store(&v);
        if let Origin::Batch { batch } = v.origin {
            self.publish_batch(batch);
        }
        Ok(v)
    }

    /// Stop a RUNNING command (Run a command) cleanly: the host signals it
    /// inside the container and the job ends as `refused` with "cancelled by
    /// <who>", the same shape a queued cancel has. Other running jobs run to
    /// their end. `Err` says why not, in a sentence for the viewer.
    pub async fn cancel_running(&self, id: u64, who: &str) -> Result<JobView, String> {
        let v = self.running_exec(id)?;
        let Some(&request) = v.reqs.last() else {
            return Err("the command has not reached the host yet; try again in a moment".into());
        };
        self.check_cancel_support().await?;
        if let Ok(mut c) = self.inner.cancelling.lock() {
            c.insert(id, who.to_string());
        }
        let reply = self
            .inner
            .host
            .ask_traced(
                Command::CancelExec { request },
                Duration::from_secs(20),
                None,
            )
            .await;
        match reply {
            Ok(r) if r.ok => Ok(self.job(id).unwrap_or(v)),
            other => {
                if let Ok(mut c) = self.inner.cancelling.lock() {
                    c.remove(&id);
                }
                Err(match other {
                    Ok(r) => r.message,
                    Err(e) => e,
                })
            }
        }
    }

    /// The job, when it is a running command; why not otherwise.
    fn running_exec(&self, id: u64) -> Result<JobView, String> {
        let Some(v) = self.job(id) else {
            return Err(format!("this dashboard knows no job {id}"));
        };
        if v.state != JobState::Running {
            return Err(format!("job {id} is not running, so it cannot be stopped"));
        }
        if v.action != ActionKind::Exec {
            return Err(format!(
                "only a running command can be stopped; {} runs to its end",
                v.action.label()
            ));
        }
        Ok(v)
    }

    /// An older host drops `CancelExec` unanswered; say so instead of waiting.
    async fn check_cancel_support(&self) -> Result<(), String> {
        let host_version = self.inner.shared.read().await.host_version.clone();
        if let Some(ver) = host_version
            && homelab_client::version::older(&ver, CANCEL_EXEC_SINCE)
        {
            return Err(format!(
                "the host runs v{ver}, which cannot stop a running command yet (v{CANCEL_EXEC_SINCE} can); it ends by itself within 120 s"
            ));
        }
        Ok(())
    }
}
