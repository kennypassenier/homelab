//! The broadcast sink: every pipeline event reaches the connected clients and
//! the journal tracing, with the requesting identity attached.

use crate::*;

pub(crate) struct BroadcastSink {
    pub(crate) log_tx: broadcast::Sender<ServerMsg>,
    /// feat-platform-3: the request this operation runs for, stamped on
    /// every line; None for work nobody asked for over the line.
    pub(crate) req: Option<u64>,
    /// feat-platform-3: the newest lines, for `CurrentOp`.
    pub(crate) recent: Arc<std::sync::Mutex<std::collections::VecDeque<ServerMsg>>>,
    pub(crate) recent_cap: usize,
    /// arch-history: this operation's steps with their times, and what its
    /// first step said it was about.
    pub(crate) timings: std::sync::Mutex<Vec<homelab_core::history::StepTiming>>,
    pub(crate) subject: std::sync::Mutex<Option<String>>,
    /// milestone act: the token name of the session that asked for this
    /// operation, stamped on every line; None for the host's own work.
    pub(crate) by: Option<String>,
}

tokio::task_local! {
    /// milestone act (homelab-admin, 2026-09-28): the token name of the
    /// session whose request is being handled. Set by `serve_ws` around the
    /// handler, read where an operation's sink is built, so the name reaches
    /// every line and the history entry without a new handler parameter.
    pub(crate) static REQUESTED_BY: String;
}

/// The token name of the session this task handles a request for, if any.
pub(crate) fn requested_by() -> Option<String> {
    REQUESTED_BY.try_with(|n| n.clone()).ok()
}

impl Sink for BroadcastSink {
    fn emit(&self, event: PipelineEvent) {
        match &event {
            PipelineEvent::StepStarted { op, step } => {
                if let Ok(mut subject) = self.subject.lock() {
                    subject.get_or_insert_with(|| op.clone());
                }
                if let Ok(mut t) = self.timings.lock() {
                    t.push(homelab_core::history::StepTiming {
                        step: step.clone(),
                        start: unix_now(),
                        end: 0,
                        changed: false,
                    });
                }
            }
            PipelineEvent::StepFinished { step, changed, .. } => {
                if let Ok(mut t) = self.timings.lock()
                    && let Some(open) = t.iter_mut().rev().find(|x| x.end == 0 && &x.step == step)
                {
                    open.end = unix_now();
                    open.changed = *changed;
                }
            }
            PipelineEvent::Plan { op, .. } => {
                if let Ok(mut subject) = self.subject.lock() {
                    subject.get_or_insert_with(|| op.clone());
                }
            }
            _ => {}
        }
        let msg = match event {
            PipelineEvent::Line { level, source, msg } => {
                // fix-122: at the line's own level, so `grep WARN` on the
                // journal finds the warnings an operation printed.
                use homelab_core::sink::Level;
                match level {
                    Level::Error => tracing::error!(source = %source, "{}", msg),
                    Level::Warn => tracing::warn!(source = %source, "{}", msg),
                    Level::Debug => tracing::debug!(source = %source, "{}", msg),
                    Level::Info => tracing::info!(source = %source, "{}", msg),
                }
                ServerMsg::Log {
                    level: level.into(),
                    source,
                    msg,
                    req: self.req,
                    ts: Some(unix_now()),
                    step: None,
                    plan: None,
                    by: self.by.clone(),
                }
            }
            // fix-122: step starts and ends reach the journal too; they went
            // only to connected clients, so a night's journal had the lines
            // of a step but not which step they belonged to.
            PipelineEvent::StepStarted { op, step } => {
                let msg = format!("[sync][run ] {} :: {}", op, step);
                tracing::info!("{}", msg);
                ServerMsg::Log {
                    level: homelab_proto::LogLevel::Info,
                    source: "HOST".into(),
                    msg,
                    req: self.req,
                    ts: Some(unix_now()),
                    step: Some(homelab_proto::StepMark {
                        op: op.clone(),
                        step: step.clone(),
                        finished: false,
                        changed: false,
                        skipped: false,
                    }),
                    plan: None,
                    by: self.by.clone(),
                }
            }
            PipelineEvent::StepFinished { op, step, changed } => {
                let msg = format!(
                    "[sync][exit] {} :: {} :: {}",
                    op,
                    step,
                    if changed { "changed" } else { "ok (no change)" }
                );
                tracing::info!("{}", msg);
                ServerMsg::Log {
                    level: homelab_proto::LogLevel::Info,
                    source: "HOST".into(),
                    msg,
                    req: self.req,
                    ts: Some(unix_now()),
                    step: Some(homelab_proto::StepMark {
                        op: op.clone(),
                        step: step.clone(),
                        finished: true,
                        changed,
                        skipped: false,
                    }),
                    plan: None,
                    by: self.by.clone(),
                }
            }
            PipelineEvent::Bytes {
                op,
                label,
                done,
                total,
            } => ServerMsg::Transfer {
                op,
                label,
                done,
                total,
                req: self.req,
            },
            // fix-171 round 2: the op's own announced plan, before its
            // first step mark — a client's total comes from this, not from
            // a past run's step count.
            PipelineEvent::Plan { op, steps } => {
                let msg = format!("[sync][plan] {} :: {} step(s) planned", op, steps.len());
                tracing::info!("{}", msg);
                ServerMsg::Log {
                    level: homelab_proto::LogLevel::Info,
                    source: "HOST".into(),
                    msg,
                    req: self.req,
                    ts: Some(unix_now()),
                    step: None,
                    plan: Some(homelab_proto::StepPlan { op, steps }),
                    by: self.by.clone(),
                }
            }
            PipelineEvent::StepSkipped { op, step } => {
                let msg = format!("[sync][skip] {} :: {}", op, step);
                tracing::info!("{}", msg);
                ServerMsg::Log {
                    level: homelab_proto::LogLevel::Info,
                    source: "HOST".into(),
                    msg,
                    req: self.req,
                    ts: Some(unix_now()),
                    step: Some(homelab_proto::StepMark {
                        op: op.clone(),
                        step: step.clone(),
                        finished: true,
                        changed: false,
                        skipped: true,
                    }),
                    plan: None,
                    by: self.by.clone(),
                }
            }
        };
        if matches!(msg, ServerMsg::Log { .. })
            && let Ok(mut ring) = self.recent.lock()
        {
            ring.push_back(msg.clone());
            while ring.len() > self.recent_cap {
                ring.pop_front();
            }
        }
        let _ = self.log_tx.send(msg);
    }
}
