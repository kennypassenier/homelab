//! The kept jobs and their logs: store, read, prune, publish.

use super::{Actions, JOB_LOG_MAX, JobState, JobView};

impl Actions {
    pub(super) fn store(&self, view: &JobView) {
        if let Ok(mut jobs) = self.inner.jobs.lock() {
            if let Some(slot) = jobs.iter_mut().find(|j| j.job == view.job) {
                *slot = view.clone();
            } else {
                jobs.push_back(view.clone());
                let keep_jobs = self.keep_jobs();
                while jobs.len() > keep_jobs {
                    jobs.pop_front();
                }
                // feat-logs-1: a job's kept log goes with the job.
                if let Ok(mut l) = self.inner.job_logs.lock() {
                    l.retain(|id, _| jobs.iter().any(|j| j.job == *id));
                }
                // feat-batch-1: a batch goes with its jobs; one whose jobs
                // are all gone is no longer reopenable, so it is not kept.
                if let Ok(mut b) = self.inner.batches.lock() {
                    b.retain(|_, ids| ids.iter().any(|id| jobs.iter().any(|j| j.job == *id)));
                }
            }
        }
        self.note_job(view);
        self.inner
            .publish
            .publish("action", serde_json::to_value(view).unwrap_or_default());
    }

    /// feat-logs-1: one `action_log` line, kept for the job (so the whole
    /// log can be fetched later, not only what a page happened to see live)
    /// and pushed to the open pages. Per job at most [`JOB_LOG_MAX`] lines
    /// are kept (the newest); a job's log goes when the job is pruned.
    pub(super) fn log_event(&self, line: serde_json::Value) {
        if let Some(job) = line["job"].as_u64()
            && let Ok(mut logs) = self.inner.job_logs.lock()
        {
            let l = logs.entry(job).or_default();
            l.push_back(line.clone());
            while l.len() > JOB_LOG_MAX {
                l.pop_front();
            }
        }
        self.inner.publish.publish("action_log", line);
    }

    /// feat-logs-1: every line kept of one job, oldest first; None for a job
    /// this dashboard does not keep.
    pub fn job_log_lines(&self, job: u64) -> Option<Vec<serde_json::Value>> {
        self.job(job)?;
        Some(
            self.inner
                .job_logs
                .lock()
                .ok()
                .and_then(|l| l.get(&job).map(|v| v.iter().cloned().collect()))
                .unwrap_or_default(),
        )
    }

    /// Every job kept, newest first.
    pub fn jobs(&self) -> Vec<JobView> {
        self.inner
            .jobs
            .lock()
            .map(|j| j.iter().rev().cloned().collect())
            .unwrap_or_default()
    }

    /// Decision notify-routing: the job that sent this request to the host,
    /// so the host's notice of it fills that job's notice. The dashboard's
    /// request ids start past any a CLI or TUI uses (arch-host-link).
    pub fn job_for_req(&self, req: u64) -> Option<u64> {
        self.inner.jobs.lock().ok().and_then(|j| {
            j.iter()
                .rev()
                .find(|v| v.reqs.contains(&req))
                .map(|v| v.job)
        })
    }

    pub fn job(&self, id: u64) -> Option<JobView> {
        self.inner
            .jobs
            .lock()
            .ok()
            .and_then(|j| j.iter().find(|v| v.job == id).cloned())
    }

    pub(super) fn finish(&self, view: &mut JobView, state: JobState, message: String) {
        view.state = state;
        view.message = Some(message);
        view.finished_at = Some(self.now());
        self.store(view);
    }
}
