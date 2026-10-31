//! Send one command to the host and follow its lines until its reply.

use homelab_proto::{Command, RpcResponse, ServerMsg, StepMark, StepPlan};
use tokio::sync::{broadcast, oneshot};

use super::{Actions, JobView, Stop};
use crate::core::actions_progress::Tracker;

impl Actions {
    /// Send one command and follow its lines until its reply.
    pub(super) async fn follow(
        &self,
        command: Command,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) -> Result<RpcResponse, Stop> {
        let mut events = self.inner.host.subscribe();
        let (sent_tx, mut sent_rx) = oneshot::channel();
        let host = self.inner.host.clone();
        let ask = host.ask_traced(command, self.inner.timeout, Some(sent_tx));
        tokio::pin!(ask);
        let mut req: Option<u64> = None;
        let mut waiting_for_id = true;
        let mut events_open = true;
        let mut early: Vec<ServerMsg> = Vec::new();
        let reply = loop {
            // Biased: the id and the lines before the reply, so a reply that
            // is ready at the same moment cannot overtake them.
            tokio::select! {
                biased;
                id = &mut sent_rx, if waiting_for_id => {
                    waiting_for_id = false;
                    if let Ok(id) = id {
                        self.got_request_id(id, &mut req, &mut early, view, tracker);
                    }
                }
                ev = events.recv(), if events_open => {
                    events_open = self.on_event(ev, req, &mut early, view, tracker);
                }
                r = &mut ask => break r,
            }
        };
        // The id is sent before the reply, but both can be ready at once.
        if waiting_for_id && let Ok(id) = sent_rx.try_recv() {
            self.got_request_id(id, &mut req, &mut early, view, tracker);
        }
        self.drain_events(&mut events, req, view, tracker);
        reply.map_err(Stop::Link)
    }

    /// Lines that came before the reply may still wait in the channel.
    fn drain_events(
        &self,
        events: &mut broadcast::Receiver<ServerMsg>,
        req: Option<u64>,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) {
        if let Some(id) = req {
            while let Ok(m) = events.try_recv() {
                self.line(m, id, view, tracker);
            }
        }
    }

    /// One broadcast message while the command runs; false once the channel
    /// is closed.
    fn on_event(
        &self,
        ev: Result<ServerMsg, broadcast::error::RecvError>,
        req: Option<u64>,
        early: &mut Vec<ServerMsg>,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) -> bool {
        match ev {
            Ok(m) => match req {
                Some(id) => self.line(m, id, view, tracker),
                None => early.push(m),
            },
            Err(broadcast::error::RecvError::Lagged(n)) => self.log_missed(view.job, req, n),
            Err(broadcast::error::RecvError::Closed) => return false,
        }
        true
    }

    /// The host's request id for this command arrived: note it and replay
    /// the lines that came before it.
    fn got_request_id(
        &self,
        id: u64,
        req: &mut Option<u64>,
        early: &mut Vec<ServerMsg>,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) {
        *req = Some(id);
        view.reqs.push(id);
        self.store(view);
        for m in std::mem::take(early) {
            self.line(m, id, view, tracker);
        }
    }

    fn log_missed(&self, job: u64, req: Option<u64>, n: u64) {
        self.log_event(serde_json::json!({
            "job": job, "req": req, "level": "warn", "source": "ADMIN",
            "msg": format!("{n} line(s) of the host were missed here; the activity page has them all"),
            "ts": self.now(),
        }));
    }

    /// steps-total-known-1: tell the page "step 0 of m" as soon as the
    /// total is known, before the first step mark, so the step list and the
    /// total show from the first frame.
    pub(super) fn announce_plan(&self, view: &mut JobView, tracker: &Tracker) {
        let op = format!("{}-{}", view.action.slug(), view.stack);
        if let Some(p) = tracker.planned(&op) {
            view.progress = Some(p.clone());
            self.inner.publish.publish(
                "action_progress",
                serde_json::json!({ "job": view.job, "progress": p }),
            );
        }
    }

    pub(super) fn line(&self, m: ServerMsg, id: u64, view: &mut JobView, tracker: &mut Tracker) {
        match m {
            // ask-kp11be-5: the host's byte counter of this job's operation.
            // An older host sends it without a request id; it then belongs
            // to no job and only the Log page shows it.
            ServerMsg::Transfer {
                label,
                done,
                total,
                req: Some(r),
                ..
            } if r == id => self.note_bytes(&label, done, total, id, view, tracker),
            ServerMsg::Log {
                level,
                source,
                msg,
                req: Some(r),
                ts,
                step,
                plan,
                ..
            } if r == id => {
                let ts = ts.map(|t| t as i64).unwrap_or_else(|| self.now());
                self.log_event(serde_json::json!({
                    "job": view.job, "req": id, "level": level, "source": source,
                    "msg": homelab_core::executor::mask_secrets(&msg), "ts": ts,
                }));
                self.progress_of_line(id, ts, step, plan, view, tracker);
            }
            _ => {}
        }
    }

    fn note_bytes(
        &self,
        label: &str,
        done: u64,
        total: Option<u64>,
        id: u64,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) {
        let b = tracker.on_bytes(label, done, total, self.now().max(0) as u64);
        view.bytes = Some(b.clone());
        self.inner.publish.publish(
            "action_bytes",
            serde_json::json!({ "job": view.job, "req": id, "bytes": b }),
        );
    }

    /// The step marks a host line carries move the job's progress.
    fn progress_of_line(
        &self,
        id: u64,
        ts: i64,
        step: Option<StepMark>,
        plan: Option<StepPlan>,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) {
        // fix-171 round 2: an op's own plan, before its first step mark —
        // for a single-command job this is what sets `m`; for Apply/a
        // batch item `execute` already fixed the whole total up front
        // (`Tracker::set_total`), so this only confirms it.
        if let Some(p) = plan {
            tracker.on_plan(&p.op, &p.steps);
            if view.progress.is_none() {
                self.announce_plan(view, tracker);
            }
        }
        if let Some(mark) = step {
            let p = tracker.on_mark(&mark, ts.max(0) as u64);
            view.progress = Some(p.clone());
            self.inner.publish.publish(
                "action_progress",
                serde_json::json!({ "job": view.job, "req": id, "progress": p }),
            );
        }
    }
}
