//! feat-stacks-5: a batch of jobs, queued together, stopped together and
//! shown as one process.

use std::sync::atomic::Ordering;

use axum::http::StatusCode;

use super::{Actions, JobState, JobView, Origin};
use crate::core::actions::{self, ActionRequest, Refusal};
use crate::core::actions_progress;

impl Actions {
    /// feat-stacks-5: a batch as the page sends it, checked whole, then
    /// queued. The route and a driven batch dialog (feat-platform-10) both
    /// come here, so a driven batch is the same batch a click makes.
    pub fn run_batch(
        &self,
        b: actions::BatchRequest,
    ) -> Result<serde_json::Value, (StatusCode, Refusal)> {
        let reqs = actions::validate_batch(b).map_err(|r| (StatusCode::BAD_REQUEST, r))?;
        // fix-batch-native-1: native stacks back up with backup-native.
        let native: std::collections::BTreeSet<String> = self
            .inner
            .shared
            .try_read()
            .ok()
            .and_then(|s| {
                s.fleet.as_ref().map(|f| {
                    f.stacks
                        .iter()
                        .filter(|x| x.native)
                        .map(|x| x.name.clone())
                        .collect()
                })
            })
            .unwrap_or_default();
        let reqs = actions::native_batch_backups(reqs, &|s| native.contains(s))
            .map_err(|r| (StatusCode::BAD_REQUEST, r))?;
        for r in &reqs {
            self.precheck(r).map_err(|r| (StatusCode::CONFLICT, r))?;
        }
        let (batch, jobs) = self.submit_batch(reqs);
        Ok(serde_json::json!({
            "batch": batch,
            "jobs": jobs.iter().map(|j| serde_json::json!({"job": j.job, "stack": j.stack})).collect::<Vec<_>>(),
        }))
    }

    /// feat-stacks-5: every stack of a validated batch, in the order given.
    pub fn submit_batch(&self, reqs: Vec<ActionRequest>) -> (u64, Vec<JobView>) {
        let batch = self.inner.next_job.fetch_add(1, Ordering::Relaxed);
        let jobs: Vec<JobView> = reqs
            .into_iter()
            .map(|r| self.submit(r, Origin::Batch { batch }))
            .collect();
        if let Ok(mut b) = self.inner.batches.lock() {
            b.insert(batch, jobs.iter().map(|j| j.job).collect());
        }
        self.publish_batch(batch);
        // one-process-batch-1: the batch's whole step plan, made now so the
        // page has the total before the first step (the first job to start
        // also waits for it, see `run`).
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let me = self.clone();
            rt.spawn(async move { me.ensure_batch_plan(batch).await });
        }
        (batch, jobs)
    }

    /// The ids of a batch's jobs; empty for an unknown batch.
    pub(super) fn batch_ids(&self, batch: u64) -> Vec<u64> {
        self.inner
            .batches
            .lock()
            .ok()
            .and_then(|b| b.get(&batch).cloned())
            .unwrap_or_default()
    }

    pub(super) fn publish_batch(&self, batch: u64) {
        if let Some(v) = self.batch_view(batch) {
            self.inner.publish.publish("action_batch", v);
        }
    }

    /// Stop a batch (Kenny, 2026-09-30: a running batch could not be
    /// stopped halfway). The job that runs now runs to its end; every job
    /// still queued is refused and never starts. The number refused.
    pub fn stop_batch(&self, batch: u64, who: &str) -> usize {
        let mut stopped = 0;
        for id in self.batch_ids(batch) {
            let Some(mut v) = self.job(id) else { continue };
            if v.state != JobState::Queued {
                continue;
            }
            v.state = JobState::Refused;
            v.finished_at = Some(self.now());
            v.message = Some(format!(
                "batch stopped by {who}; this stack was not touched"
            ));
            self.store(&v);
            stopped += 1;
        }
        self.publish_batch(batch);
        stopped
    }

    /// feat-batch-1: every still-queued job of a batch is cancelled, the
    /// running one is left (the same boundary as [`Self::stop_batch`]). The
    /// ids cancelled.
    pub fn cancel_batch(&self, batch: u64, who: &str) -> Vec<u64> {
        let cancelled: Vec<u64> = self
            .batch_ids(batch)
            .into_iter()
            .filter(|id| self.cancel_job(*id, who).is_ok())
            .collect();
        self.publish_batch(batch);
        cancelled
    }

    /// feat-batch-1: the batches still kept, newest first, as
    /// [`Self::batch_view`] shows each.
    pub fn batches(&self) -> Vec<serde_json::Value> {
        let ids: Vec<u64> = self
            .inner
            .batches
            .lock()
            .map(|b| b.keys().rev().copied().collect())
            .unwrap_or_default();
        ids.into_iter().filter_map(|b| self.batch_view(b)).collect()
    }

    /// The batch with the latest view of its jobs, as `action_batch` sends
    /// it and `homelab ui finish` follows it. None for an unknown batch.
    pub fn batch_view(&self, batch: u64) -> Option<serde_json::Value> {
        let ids = self.batch_ids(batch);
        if ids.is_empty() {
            return None;
        }
        let views: Vec<JobView> = ids.iter().filter_map(|id| self.job(*id)).collect();
        let count = |s: JobState| views.iter().filter(|j| j.state == s).count();
        // one-process-batch-1: the plan of the whole batch, one process.
        let plan = self
            .inner
            .batch_plans
            .lock()
            .ok()
            .and_then(|p| p.get(&batch).cloned());
        let (steps_total, steps_exact) = plan_totals(plan.as_deref());
        Some(serde_json::json!({
            "batch": batch,
            "plan": plan,
            "steps_total": steps_total,
            "steps_exact": steps_exact,
            "jobs": views.iter().map(batch_job).collect::<Vec<_>>(),
            "done": views.iter().all(|j| j.state.finished()),
            "ok": count(JobState::Done),
            "failed": count(JobState::Failed) + count(JobState::Refused) + count(JobState::Unknown),
            "deferred": count(JobState::Deferred),
        }))
    }
}

fn batch_job(j: &JobView) -> serde_json::Value {
    serde_json::json!({
        "job": j.job, "stack": j.stack, "state": j.state, "message": j.message,
    })
}

fn plan_totals(
    plan: Option<&[actions_progress::MemberPlan]>,
) -> (serde_json::Value, serde_json::Value) {
    plan.map(actions_progress::batch_total).map_or(
        (serde_json::Value::Null, serde_json::Value::Null),
        |(t, e)| (t.into(), e.into()),
    )
}
