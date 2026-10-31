//! A press: the checks it gets, the deploy guard, and the job it becomes.

use std::sync::atomic::Ordering;

use homelab_core::ops::deployguard::{self, Ancestry};

use super::{Actions, Job, JobState, JobView, Origin};
use crate::core::actions::{self, ActionKind, ActionRequest, Needs, Refusal};

impl Actions {
    /// Checks that need the fleet or the working copy, done at the press
    /// for a quick answer and again when the job runs.
    pub fn precheck(&self, req: &ActionRequest) -> Result<(), Refusal> {
        let what = format!("{} {}", req.action.slug(), req.stack);
        self.precheck_working_copy(req, &what)?;
        self.precheck_host_version(req, &what)?;
        self.precheck_not_queued(req, &what)
    }

    fn precheck_working_copy(&self, req: &ActionRequest, what: &str) -> Result<(), Refusal> {
        let needs_files = matches!(
            req.action.needs(),
            Needs::Manifest | Needs::Spec | Needs::NativeManifest
        );
        if needs_files && !self.inner.files.present() && req.action != ActionKind::Destroy {
            return Err(Refusal::new(
                what,
                "the dashboard has no working copy of the homelab repository",
                "the dashboard clones it at start once the deploy key is provisioned (the \
                 working copy panel on the settings page says what is missing); actions that \
                 need only the stack name work without it",
            ));
        }
        Ok(())
    }

    fn precheck_host_version(&self, req: &ActionRequest, what: &str) -> Result<(), Refusal> {
        // An older host drops a command it does not know without a word.
        if req.action != ActionKind::InstallNative {
            return Ok(());
        }
        let v = self
            .inner
            .shared
            .try_read()
            .ok()
            .and_then(|s| s.host_version.clone());
        crate::core::hostversion::at_least(
            what,
            v.as_deref(),
            crate::core::hostversion::NEXT_RELEASE,
            "update the host first (Update the host on the host page), or install from a \
             workstation: homelab install-native stacks/<stack> <tag>",
        )?;
        Ok(())
    }

    fn precheck_not_queued(&self, req: &ActionRequest, what: &str) -> Result<(), Refusal> {
        if let Ok(jobs) = self.inner.jobs.lock()
            && let Some(j) = jobs.iter().find(|j| {
                j.state == JobState::Queued && j.stack == req.stack && j.action == req.action
            })
        {
            return Err(Refusal::new(
                what,
                format!("the same action is already queued as job {}", j.job),
                "wait for it; the queue runs one action at a time",
            ));
        }
        Ok(())
    }

    /// A validated request, pressed: the checks a press gets (the queue,
    /// the working copy, the deploy guard), then the job. The route and the
    /// driver (feat-platform-10) both go through here, so a driven press
    /// is refused for exactly what a click is.
    pub async fn press(&self, req: ActionRequest, origin: Origin) -> Result<JobView, Refusal> {
        self.precheck(&req)?;
        let a2 = self.clone();
        let r2 = req.clone();
        if let Ok(Err(r)) = tokio::task::spawn_blocking(move || a2.guard(&r2)).await {
            return Err(r);
        }
        Ok(self.submit(req, origin))
    }

    /// Queue a validated request; the job as it stands now.
    pub fn submit(&self, req: ActionRequest, origin: Origin) -> JobView {
        let job = self.inner.next_job.fetch_add(1, Ordering::Relaxed);
        let view = JobView {
            job,
            origin,
            restarts_dashboard: actions::restarts_dashboard(&req.stack, req.action),
            stack: req.stack,
            action: req.action,
            args: req.args,
            state: JobState::Queued,
            queued_at: self.now(),
            started_at: None,
            finished_at: None,
            reqs: Vec::new(),
            message: None,
            cli: None,
            progress: None,
            bytes: None,
            flow: None,
            plan: None,
        };
        self.store(&view);
        let _ = self.inner.queue.send(Job { view: view.clone() });
        view
    }

    /// arch-deploy-guard, with the working copy's history.
    pub(super) fn guard(&self, req: &ActionRequest) -> Result<(), Refusal> {
        if !matches!(req.action, ActionKind::Deploy | ActionKind::DeployCommit) {
            return Ok(());
        }
        let applied = self.fleet_stack(&req.stack).and_then(|(_, a)| a);
        let ancestry = match applied.as_deref().and_then(deployguard::applied_commit) {
            None => Ancestry::Contained,
            Some(c) => self.inner.files.ancestry(c),
        };
        deployguard::decide(&req.stack, applied.as_deref(), ancestry, req.args.force).map_err(
            |why| {
                Refusal::new(
                    format!("{} {}", req.action.slug(), req.stack),
                    why,
                    "pull the working copy, or send force: true if undoing that deploy is the point",
                )
            },
        )
    }
}
