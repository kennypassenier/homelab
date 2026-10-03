//! feat-stacks-8 (arch-schedule): the scheduler task. Schedules live in
//! `schedules.json` under the data dir (arch-state); every tick the task
//! asks `core::schedule::tick` what is due, queues it as an ordinary job
//! (the same queue a press uses), and turns every slot that passed without a
//! run into a notice. A missed slot is never caught up.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use tokio::sync::Mutex;

use super::actions::{Actions, Clock, Origin, Publish};
use super::actions_notify::NotifyCenter;
use super::actions_state::{StateError, read_json, write_json};
use crate::core::actions::Refusal;
use crate::core::notify::{Draft, Kind};
use crate::core::schedule::{
    self, LastRun, Missed, MissedWhy, Schedule, ScheduleFile, ScheduleInput,
};

pub struct Scheduler {
    path: PathBuf,
    state: Mutex<ScheduleFile>,
    actions: Actions,
    notify: Arc<NotifyCenter>,
    publish: Arc<dyn Publish>,
    clock: Clock,
    grace_s: i64,
    counter: AtomicU64,
}

impl Scheduler {
    pub fn load(
        path: PathBuf,
        actions: Actions,
        notify: Arc<NotifyCenter>,
        publish: Arc<dyn Publish>,
        clock: Clock,
        grace_s: u64,
    ) -> Result<Arc<Self>, StateError> {
        let state: ScheduleFile = read_json(&path)?.unwrap_or_default();
        state.check().map_err(|why| StateError::Parse {
            path: path.display().to_string(),
            why,
        })?;
        Ok(Arc::new(Scheduler {
            path,
            state: Mutex::new(state),
            actions,
            notify,
            publish,
            clock,
            grace_s: grace_s as i64,
            counter: AtomicU64::new(0),
        }))
    }

    /// Tick every `every` until the process ends.
    pub fn spawn(self: &Arc<Self>, every: Duration) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(every);
            loop {
                t.tick().await;
                me.tick().await;
            }
        });
    }

    fn save(&self, f: &ScheduleFile) -> Result<(), StateError> {
        write_json(&self.path, f)
    }

    /// One look: queue what is due, notify what was missed.
    pub async fn tick(&self) {
        let now = (self.clock)();
        let mut runs: Vec<(Schedule, i64)> = Vec::new();
        let mut missed: Vec<(Schedule, i64, bool)> = Vec::new();
        {
            let mut f = self.state.lock().await;
            for s in f.schedules.iter_mut() {
                let busy = s
                    .last_run
                    .as_ref()
                    .and_then(|l| self.actions.job(l.job))
                    .is_some_and(|j| !j.state.finished());
                let plan = schedule::tick(s, now, self.grace_s, busy);
                s.handled_until = plan.handled_until;
                if let Some(slot) = plan.run {
                    runs.push((s.clone(), slot));
                }
                for slot in plan.missed {
                    // The newest slot was skipped because a run was going.
                    let was_busy = busy && now - slot <= self.grace_s;
                    // redesign-schedules-3: the page names the newest one.
                    s.last_missed = Some(Missed {
                        slot,
                        why: if was_busy {
                            MissedWhy::Busy
                        } else {
                            MissedWhy::Down
                        },
                    });
                    missed.push((s.clone(), slot, was_busy));
                }
            }
            if let Err(e) = self.save(&f) {
                tracing::warn!(error = %e, "schedules not saved");
            }
        }
        for (s, slot) in runs {
            let input = ScheduleInput {
                stack: s.stack.clone(),
                action: s.action.clone(),
                args: s.args.clone(),
                when: s.when.clone(),
                enabled: true,
                note: s.note.clone(),
            };
            match schedule::check_input(&input) {
                Ok(req) => {
                    let job = self.actions.submit(
                        req,
                        Origin::Schedule {
                            schedule: s.id.clone(),
                            slot,
                        },
                    );
                    let mut f = self.state.lock().await;
                    if let Some(x) = f.schedules.iter_mut().find(|x| x.id == s.id) {
                        x.last_run = Some(LastRun { slot, job: job.job });
                    }
                    if let Err(e) = self.save(&f) {
                        tracing::warn!(error = %e, "schedules not saved");
                    }
                }
                Err(r) => {
                    {
                        let mut f = self.state.lock().await;
                        if let Some(x) = f.schedules.iter_mut().find(|x| x.id == s.id) {
                            x.last_missed = Some(Missed {
                                slot,
                                why: MissedWhy::Refused,
                            });
                        }
                        if let Err(e) = self.save(&f) {
                            tracing::warn!(error = %e, "schedules not saved");
                        }
                    }
                    self.missed_notice(&s, slot, &format!("{}: {}", r.what, r.why))
                        .await
                }
            }
        }
        for (s, slot, busy) in missed {
            let why = if busy {
                "its previous run was still going".to_string()
            } else {
                "the dashboard was not running at that time".to_string()
            };
            self.missed_notice(&s, slot, &why).await;
        }
        self.publish_list().await;
    }

    async fn missed_notice(&self, s: &Schedule, slot: i64, why: &str) {
        let on_stack = s.stack != crate::core::actions::HOST_TARGET;
        let mut d = Draft::new(
            Kind::ScheduleMissed,
            &format!("schedule-{}-{}", s.action, s.stack),
            &format!(
                "Skipped: {} {} of {}",
                s.action,
                s.stack,
                schedule::local_label(slot)
            ),
            &format!(
                "The planned {} of {} at {} ({}) did not run: {}. It is not made up \
                 for; the next slot runs as planned.",
                s.action,
                s.stack,
                schedule::local_label(slot),
                schedule::ZONE,
                why
            ),
        );
        d.stack = on_stack.then(|| s.stack.clone());
        // Decision notify-detail: since the slot, what it costs, what to do.
        d.detail = crate::core::notify::Detail {
            level: crate::core::notify::Level::Warning,
            since: Some(slot),
            consequence: Some(format!(
                "The {} of {} planned for then did not happen.",
                s.action, s.stack
            )),
            remedy: Some(format!(
                "Run it now if it matters before the next slot: the {} button on the {} page. \
                 Activity's Planned view shows the next slot.",
                s.action,
                if on_stack {
                    format!("{} stack", s.stack)
                } else {
                    "host".into()
                }
            )),
            link: Some(if on_stack {
                homelab_core::notify::page::stack(&s.stack)
            } else {
                "/activity?view=planned".into()
            }),
            label: Some(s.action.clone()),
            fixes: crate::core::notify::fix_for(&crate::core::notify::FixSource::Retry {
                action: &s.action,
                stack: &s.stack,
            })
            .into_iter()
            .collect(),
            ..Default::default()
        };
        self.notify.notify(d).await;
    }

    fn view(&self, s: &Schedule, now: i64) -> serde_json::Value {
        let next = schedule::next_run(s, now);
        serde_json::json!({
            "schedule": s,
            "next_run": next,
            "next_run_local": next.map(schedule::local_label),
            "last_job": s.last_run.as_ref().and_then(|l| self.actions.job(l.job)),
        })
    }

    pub async fn list(&self) -> serde_json::Value {
        let now = (self.clock)();
        let f = self.state.lock().await;
        serde_json::json!({
            "zone": f.zone,
            "schedules": f.schedules.iter().map(|s| self.view(s, now)).collect::<Vec<_>>(),
        })
    }

    async fn publish_list(&self) {
        let v = self.list().await;
        self.publish.publish("schedules", v);
    }

    fn new_id(&self, now: i64) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("s{:x}{:02x}", now.max(0), n % 256)
    }

    pub async fn create(&self, input: ScheduleInput) -> Result<serde_json::Value, Refusal> {
        let req = schedule::check_input(&input)?;
        let now = (self.clock)();
        let s = Schedule {
            id: self.new_id(now),
            stack: req.stack,
            action: req.action.slug().to_string(),
            args: req.args,
            when: input.when,
            enabled: input.enabled,
            note: input.note,
            created_at: now,
            // A new schedule starts now: nothing before it counts as missed.
            handled_until: now,
            last_run: None,
            last_missed: None,
        };
        let view = {
            let mut f = self.state.lock().await;
            f.schedules.push(s.clone());
            self.save(&f).map_err(saved)?;
            self.view(&s, now)
        };
        self.publish_list().await;
        Ok(view)
    }

    pub async fn update(
        &self,
        id: &str,
        input: ScheduleInput,
    ) -> Result<serde_json::Value, Refusal> {
        let req = schedule::check_input(&input)?;
        let now = (self.clock)();
        let view = {
            let mut f = self.state.lock().await;
            let Some(s) = f.schedules.iter_mut().find(|s| s.id == id) else {
                return Err(unknown(id));
            };
            s.stack = req.stack;
            s.action = req.action.slug().to_string();
            s.args = req.args;
            if s.when != input.when || (!s.enabled && input.enabled) {
                // A changed time (or switching it back on) starts from now:
                // slots of the old plan are not "missed" by the new one.
                s.handled_until = now;
            }
            s.when = input.when;
            s.enabled = input.enabled;
            s.note = input.note;
            let s = s.clone();
            self.save(&f).map_err(saved)?;
            self.view(&s, now)
        };
        self.publish_list().await;
        Ok(view)
    }

    /// redesign-schedules-2: skip the schedule's next run, `slot`, which
    /// must be the next run the page showed; the schedule stays on.
    pub async fn skip(&self, id: &str, slot: i64) -> Result<serde_json::Value, Refusal> {
        let now = (self.clock)();
        let view = {
            let mut f = self.state.lock().await;
            let Some(s) = f.schedules.iter_mut().find(|s| s.id == id) else {
                return Err(unknown(id));
            };
            s.handled_until = schedule::skip(s, slot, now).map_err(|why| {
                Refusal::new(
                    format!("skipping a run of schedule {id}"),
                    why,
                    "reload the schedules page and skip the run it shows next",
                )
            })?;
            let s = s.clone();
            self.save(&f).map_err(saved)?;
            self.view(&s, now)
        };
        self.publish_list().await;
        Ok(view)
    }

    pub async fn delete(&self, id: &str) -> Result<(), Refusal> {
        {
            let mut f = self.state.lock().await;
            let before = f.schedules.len();
            f.schedules.retain(|s| s.id != id);
            if f.schedules.len() == before {
                return Err(unknown(id));
            }
            self.save(&f).map_err(saved)?;
        }
        self.publish_list().await;
        Ok(())
    }
}

fn unknown(id: &str) -> Refusal {
    Refusal::new(
        format!("schedule {id}"),
        "there is no such schedule",
        "reload the schedules page",
    )
}

fn saved(e: StateError) -> Refusal {
    Refusal::new(
        "saving the schedules",
        e.to_string(),
        "check the dashboard's data directory (HOMELAB_ADMIN_DATA_DIR)",
    )
}

// ── routes ──────────────────────────────────────────────────────────────

fn bad(status: StatusCode, r: Refusal) -> Response {
    (status, Json(r)).into_response()
}

fn input(b: Result<Json<ScheduleInput>, JsonRejection>) -> Result<ScheduleInput, Refusal> {
    b.map(|Json(i)| i).map_err(|e| {
        Refusal::new(
            "schedule",
            format!("the request body does not read: {}", e.body_text()),
            "send {stack, action, args, when: {every: day|week|once, ...}, enabled, note}",
        )
    })
}

async fn list(State(s): State<Arc<Scheduler>>) -> Json<serde_json::Value> {
    Json(s.list().await)
}

async fn create(
    State(s): State<Arc<Scheduler>>,
    b: Result<Json<ScheduleInput>, JsonRejection>,
) -> Response {
    let i = match input(b) {
        Ok(i) => i,
        Err(r) => return bad(StatusCode::BAD_REQUEST, r),
    };
    match s.create(i).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(r) => bad(StatusCode::BAD_REQUEST, r),
    }
}

async fn update(
    State(s): State<Arc<Scheduler>>,
    UrlPath(id): UrlPath<String>,
    b: Result<Json<ScheduleInput>, JsonRejection>,
) -> Response {
    let i = match input(b) {
        Ok(i) => i,
        Err(r) => return bad(StatusCode::BAD_REQUEST, r),
    };
    match s.update(&id, i).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => bad(StatusCode::BAD_REQUEST, r),
    }
}

/// The body of `POST /data/schedules/{id}/skip`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SkipBody {
    slot: i64,
}

async fn skip_run(
    State(s): State<Arc<Scheduler>>,
    UrlPath(id): UrlPath<String>,
    b: Result<Json<SkipBody>, JsonRejection>,
) -> Response {
    let slot = match b {
        Ok(Json(b)) => b.slot,
        Err(e) => {
            return bad(
                StatusCode::BAD_REQUEST,
                Refusal::new(
                    "skip",
                    format!("the request body does not read: {}", e.body_text()),
                    "send {slot: <the next run, unix seconds>}",
                ),
            );
        }
    };
    match s.skip(&id, slot).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => bad(StatusCode::BAD_REQUEST, r),
    }
}

async fn remove(State(s): State<Arc<Scheduler>>, UrlPath(id): UrlPath<String>) -> Response {
    match s.delete(&id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(r) => bad(StatusCode::NOT_FOUND, r),
    }
}

/// Mounted with `dashboard_routes`.
pub fn router(s: Arc<Scheduler>) -> Router {
    Router::new()
        .route("/data/schedules", get(list).post(create))
        .route("/data/schedules/{id}", put(update).delete(remove))
        .route("/data/schedules/{id}/skip", post(skip_run))
        .with_state(s)
}
