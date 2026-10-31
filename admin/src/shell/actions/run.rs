//! The worker's turn: run one job, record how it ended and tell the person
//! (the notice) and the page (the batch).

use homelab_proto::RpcResponse;

use super::{Actions, Job, JobState, JobView, Origin, Stop};
use crate::core::actions::{self, ActionKind, ActionRequest};
use crate::core::notify::{Detail, Draft, Kind, Level};

impl Actions {
    pub(super) async fn run(&self, job: Job) {
        // A batch stopped while this job waited in the queue: the stored
        // view already says so, and the job never starts.
        if self
            .job(job.view.job)
            .is_some_and(|v| v.state != JobState::Queued)
        {
            return;
        }
        // one-process-batch-1: a batch's step plan exists before its first
        // job starts.
        if let Origin::Batch { batch } = job.view.origin {
            self.ensure_batch_plan(batch).await;
        }
        let mut view = job.view;
        self.mark_running(&mut view);
        let outcome = self.execute(&request_of(&view), &mut view).await;
        let stopped_by = self
            .inner
            .cancelling
            .lock()
            .ok()
            .and_then(|mut c| c.remove(&view.job));
        let (state, message) = job_end(outcome, stopped_by.as_deref());
        // arch-secrets-read: what the host printed (an exec's output above
        // all) is masked by shape before it goes over the live channel.
        let message = homelab_core::executor::mask_secrets(&message);
        self.finish(&mut view, state, message.clone());
        // Stopped on purpose by the person who sees the job: nothing failed,
        // so no notice goes out.
        if stopped_by.is_none() || state != JobState::Refused {
            self.report_settled(&view, state);
            let notice = job_notice(&view, state, message);
            self.inner.notify.notify(notice).await;
        }
        self.publish_batch_of(&view);
    }

    fn mark_running(&self, view: &mut JobView) {
        view.state = JobState::Running;
        view.started_at = Some(self.now());
        self.store(view);
    }

    fn publish_batch_of(&self, view: &JobView) {
        if let Origin::Batch { batch } = view.origin {
            self.publish_batch(batch);
        }
    }

    /// fix-stale-refresh-1: the stack now runs what its files name.
    fn report_settled(&self, view: &JobView, state: JobState) {
        if state == JobState::Done
            && matches!(
                view.action,
                ActionKind::Deploy
                    | ActionKind::DeployCommit
                    | ActionKind::Update
                    | ActionKind::UpdateApps
                    | ActionKind::Enable
            )
            && let Some(f) = self.inner.settled.get()
        {
            f(&view.stack);
        }
    }
}

fn request_of(view: &JobView) -> ActionRequest {
    ActionRequest {
        stack: view.stack.clone(),
        action: view.action,
        args: view.args.clone(),
    }
}

/// How a job ended, from what `execute` answered.
fn job_end(outcome: Result<RpcResponse, Stop>, stopped_by: Option<&str>) -> (JobState, String) {
    match outcome {
        // A viewer stopped the running command: the host's reply starts
        // `cancelled`; the job ends the way a queued cancel does.
        Ok(r) if !r.ok && stopped_by.is_some() && r.message.starts_with("cancelled") => (
            JobState::Refused,
            format!(
                "cancelled by {}; the command was stopped\n{}",
                stopped_by.unwrap_or("a viewer"),
                r.message.trim_start_matches("cancelled").trim_start()
            ),
        ),
        Ok(r) if r.ok => (JobState::Done, r.message),
        Ok(r) if r.deferred.is_some() => (JobState::Deferred, r.message),
        Ok(r) => (JobState::Failed, r.message),
        Err(Stop::Refused(r)) => (
            JobState::Refused,
            format!("{}: {} ({})", r.what, r.why, r.fix),
        ),
        Err(Stop::Link(e)) => (
            JobState::Unknown,
            format!("{e}; the activity page shows what the host did"),
        ),
    }
}

/// The notice a finished job raises.
fn job_notice(view: &JobView, state: JobState, body: String) -> Draft {
    let kind = match state {
        JobState::Done => Kind::ActionDone,
        JobState::Deferred => Kind::ActionDeferred,
        _ => Kind::ActionFailed,
    };
    let on_stack = view.stack != actions::HOST_TARGET;
    let place = if on_stack {
        format!(" {}", view.stack)
    } else {
        String::new()
    };
    let ran_s = view
        .started_at
        .zip(view.finished_at)
        .map(|(s, f)| (f - s).max(0) as u64);
    Draft {
        kind,
        op: format!("{}-{}", view.action.slug(), view.stack),
        stack: on_stack.then(|| view.stack.clone()),
        title: format!("{}{}: {}", view.action.label(), place, state_word(state)),
        body,
        job: Some(view.job),
        ran_s,
        detail: job_detail(view, state, on_stack),
    }
}

fn state_word(state: JobState) -> &'static str {
    match state {
        JobState::Done => "done",
        JobState::Deferred => "stood aside",
        JobState::Refused => "refused",
        JobState::Unknown => "outcome unknown",
        _ => "failed",
    }
}

/// Decision notify-detail: the host's own notice of this job fills in
/// since when, the consequence and the remedy when it arrives
/// (`NotifyFile::import_host`); what the dashboard knows now is here.
fn job_detail(view: &JobView, state: JobState, on_stack: bool) -> Detail {
    let failed = !matches!(state, JobState::Done | JobState::Deferred);
    Detail {
        level: match state {
            JobState::Done => Level::Ok,
            JobState::Deferred => Level::Info,
            _ => Level::Warning,
        },
        since: view.started_at,
        link: Some(if on_stack {
            homelab_core::notify::page::stack(&view.stack)
        } else {
            format!("{}?job={}", homelab_core::notify::page::JOBS, view.job)
        }),
        label: Some(view.action.slug().to_string()),
        fixes: if failed {
            crate::core::notify::fix_for(&crate::core::notify::FixSource::Retry {
                action: view.action.slug(),
                stack: &view.stack,
            })
            .into_iter()
            .collect()
        } else {
            Vec::new()
        },
        ..Default::default()
    }
}
