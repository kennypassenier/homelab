//! eten-home-ip-1: the job behind `update-home-address`. A child of
//! `actions` so it uses the same job view, progress and log the other jobs
//! use; it sends nothing to the host. The decisions are
//! `core::homeaddress`, the calls `shell::homeaddress`.

use homelab_proto::{RpcResponse, StepMark};

use super::{Actions, JobView, Stop};
use crate::core::actions::{ActionKind, ActionRequest, Refusal};
use crate::core::actions_progress::Tracker;
use crate::shell::homeaddress::{Report, Step};

/// Turns the service's steps and lines into the job's progress and log.
struct JobReport<'a> {
    actions: &'a Actions,
    view: &'a mut JobView,
    tracker: Tracker,
    op: String,
}

impl Report for JobReport<'_> {
    fn step(&mut self, step: Step, finished: bool, changed: bool) {
        let mark = StepMark {
            op: self.op.clone(),
            step: step.name().to_string(),
            finished,
            changed,
            skipped: false,
        };
        let ts = self.actions.now().max(0) as u64;
        let p = self.tracker.on_mark(&mark, ts);
        self.view.progress = Some(p.clone());
        self.actions.inner.publish.publish(
            "action_progress",
            serde_json::json!({ "job": self.view.job, "progress": p }),
        );
    }

    fn line(&mut self, text: &str) {
        self.actions.log_event(serde_json::json!({
            "job": self.view.job, "req": null, "level": "info", "source": "ADMIN",
            "msg": text, "ts": self.actions.now(),
        }));
    }
}

impl Actions {
    /// The home-address service, once the dashboard is mounted (and in tests).
    pub fn set_home_address(&self, h: std::sync::Arc<crate::shell::homeaddress::HomeAddress>) {
        let _ = self.inner.home_address.set(h);
    }

    pub(super) async fn run_home_address(
        &self,
        req: &ActionRequest,
        view: &mut JobView,
    ) -> Result<RpcResponse, Stop> {
        let Some(home) = self.inner.home_address.get().cloned() else {
            return Err(Stop::Refused(Refusal::new(
                "update-home-address",
                "the home address service is not mounted in this dashboard",
                "start the dashboard the usual way",
            )));
        };
        let op = format!("{}-{}", req.action.slug(), view.stack);
        debug_assert_eq!(req.action, ActionKind::UpdateHomeAddress);
        let mut tracker = Tracker::new(Vec::new());
        tracker.set_total(Step::ALL.len());
        self.announce_plan(view, &tracker);
        let mut report = JobReport {
            actions: self,
            view,
            tracker,
            op,
        };
        let result = home.update(&mut report).await;
        let (ok, message) = match result {
            Ok(m) => (true, m),
            Err(m) => (false, m),
        };
        Ok(RpcResponse {
            id: 0,
            ok,
            message,
            deferred: None,
        })
    }
}
