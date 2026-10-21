//! feat-stacks-8 (arch-schedule): the scheduler task. Schedules live in the
//! `schedules` table of the app database (feat-kpadopt-2; they were
//! `schedules.json`), every operation one transaction; every tick the task
//! asks `core::schedule::tick` what is due, queues it as an ordinary job
//! (the same queue a press uses), and turns every slot that passed without a
//! run into a notice. A missed slot is never caught up.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};

use super::actions::{Actions, Clock, Origin, Publish};
use super::actions_notify::NotifyCenter;
use crate::core::actions::Refusal;
use crate::core::notify::{Draft, Kind};
use crate::core::schedule::{self, LastRun, Missed, MissedWhy, Schedule, ScheduleInput};
use chassis::Error;
use chassis::storage::AppDb;
use chassis::storage::rusqlite::{Connection, Row, Transaction, params};

pub struct Scheduler {
    db: AppDb,
    actions: Actions,
    notify: Arc<NotifyCenter>,
    publish: Arc<dyn Publish>,
    clock: Clock,
    grace_s: i64,
    counter: AtomicU64,
}

// ── the rows (feat-kpadopt-2: the `schedules` table) ────────────────────

const COLS: &str = "id, stack, action, enabled, note, created_at, handled_until, paused_until, \
     args, when_json, last_run, last_missed";

fn unreadable(what: &str, e: impl std::fmt::Display) -> Error {
    Error::internal(
        format!("a stored schedule does not read: {what}: {e}"),
        "the database file may be damaged; restore the pre-update copy or the last backup",
    )
}

fn parse<T: serde::de::DeserializeOwned>(what: &str, text: &str) -> Result<T, Error> {
    serde_json::from_str(text).map_err(|e| unreadable(what, e))
}

fn schedule_of(r: &Row) -> chassis::storage::rusqlite::Result<Result<Schedule, Error>> {
    let (args, when, last_run, last_missed): (String, String, Option<String>, Option<String>) =
        (r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?);
    let build = || -> Result<Schedule, Error> {
        Ok(Schedule {
            id: r.get(0)?,
            stack: r.get(1)?,
            action: r.get(2)?,
            enabled: r.get::<_, i64>(3)? != 0,
            note: r.get(4)?,
            created_at: r.get(5)?,
            handled_until: r.get(6)?,
            paused_until: r.get(7)?,
            args: parse("args", &args)?,
            when: parse("when", &when)?,
            last_run: last_run
                .as_deref()
                .map(|t| parse("last_run", t))
                .transpose()?,
            last_missed: last_missed
                .as_deref()
                .map(|t| parse("last_missed", t))
                .transpose()?,
        })
    };
    Ok(build())
}

/// Every schedule, in the order they were created.
pub(crate) fn all(c: &Connection) -> Result<Vec<Schedule>, Error> {
    let mut stmt = c.prepare(&format!("SELECT {COLS} FROM schedules ORDER BY rowid"))?;
    let rows = stmt
        .query_map([], schedule_of)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().collect()
}

fn find(c: &Connection, id: &str) -> Result<Option<Schedule>, Error> {
    let mut stmt = c.prepare(&format!("SELECT {COLS} FROM schedules WHERE id = ?1"))?;
    let row = stmt
        .query_map(params![id], schedule_of)?
        .next()
        .transpose()?;
    row.transpose()
}

/// Insert or replace one schedule (its row keeps its place in the list).
pub(crate) fn put_schedule(tx: &Transaction, s: &Schedule) -> Result<(), Error> {
    let json = |e: serde_json::Error| unreadable("a schedule to store", e);
    tx.execute(
        "INSERT INTO schedules (id, stack, action, enabled, note, created_at, handled_until, \
         paused_until, args, when_json, last_run, last_missed) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) \
         ON CONFLICT(id) DO UPDATE SET stack = excluded.stack, action = excluded.action, \
         enabled = excluded.enabled, note = excluded.note, created_at = excluded.created_at, \
         handled_until = excluded.handled_until, paused_until = excluded.paused_until, \
         args = excluded.args, when_json = excluded.when_json, last_run = excluded.last_run, \
         last_missed = excluded.last_missed",
        params![
            s.id,
            s.stack,
            s.action,
            s.enabled as i64,
            s.note,
            s.created_at,
            s.handled_until,
            s.paused_until,
            serde_json::to_string(&s.args).map_err(json)?,
            serde_json::to_string(&s.when).map_err(json)?,
            s.last_run
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(json)?,
            s.last_missed
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(json)?,
        ],
    )?;
    Ok(())
}

impl Scheduler {
    /// Nothing is read here: the database opens at start and every call
    /// finds its rows.
    pub fn load(
        db: AppDb,
        actions: Actions,
        notify: Arc<NotifyCenter>,
        publish: Arc<dyn Publish>,
        clock: Clock,
        grace_s: u64,
    ) -> Arc<Self> {
        Arc::new(Scheduler {
            db,
            actions,
            notify,
            publish,
            clock,
            grace_s: grace_s as i64,
            counter: AtomicU64::new(0),
        })
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

    /// Change one schedule in its own transaction; a vanished one is left.
    async fn amend(&self, id: &str, f: impl FnOnce(&mut Schedule) + Send + 'static) {
        let id = id.to_string();
        let done = self
            .db
            .write(move |tx| {
                if let Some(mut s) = find(tx, &id)? {
                    f(&mut s);
                    put_schedule(tx, &s)?;
                }
                Ok(())
            })
            .await;
        if let Err(e) = done {
            tracing::warn!(error = %e.message, "schedules not saved");
        }
    }

    /// One look: queue what is due, notify what was missed. The whole look
    /// at the stored schedules is ONE transaction.
    pub async fn tick(&self) {
        let now = (self.clock)();
        let (actions, grace_s) = (self.actions.clone(), self.grace_s);
        let looked = self
            .db
            .write(move |tx| {
                let mut runs: Vec<(Schedule, i64)> = Vec::new();
                let mut missed: Vec<(Schedule, i64, bool)> = Vec::new();
                for mut s in all(tx)? {
                    let before = s.clone();
                    let busy = s
                        .last_run
                        .as_ref()
                        .and_then(|l| actions.job(l.job))
                        .is_some_and(|j| !j.state.finished());
                    let plan = schedule::tick(&s, now, grace_s, busy);
                    s.handled_until = plan.handled_until;
                    if let Some(slot) = plan.run {
                        runs.push((s.clone(), slot));
                    }
                    for slot in plan.missed {
                        // The newest slot was skipped because a run was going.
                        let was_busy = busy && now - slot <= grace_s;
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
                    if s != before {
                        put_schedule(tx, &s)?;
                    }
                }
                Ok((runs, missed))
            })
            .await;
        let (runs, missed) = match looked {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e.message, "schedules not saved");
                (Vec::new(), Vec::new())
            }
        };
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
                    let job = job.job;
                    self.amend(&s.id, move |x| x.last_run = Some(LastRun { slot, job }))
                        .await;
                }
                Err(r) => {
                    self.amend(&s.id, move |x| {
                        x.last_missed = Some(Missed {
                            slot,
                            why: MissedWhy::Refused,
                        })
                    })
                    .await;
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
        let rows = match self.db.read(all).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(error = %e.message, "schedules not read");
                Vec::new()
            }
        };
        serde_json::json!({
            "zone": schedule::ZONE,
            "schedules": rows.iter().map(|s| self.view(s, now)).collect::<Vec<_>>(),
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
            paused_until: None,
        };
        let stored = s.clone();
        self.db
            .write(move |tx| put_schedule(tx, &stored))
            .await
            .map_err(saved)?;
        let view = self.view(&s, now);
        self.publish_list().await;
        Ok(view)
    }

    /// Change the stored schedule `id` in one transaction. `f` returns the
    /// refusal when the change is not allowed.
    async fn change(
        &self,
        id: &str,
        f: impl FnOnce(&mut Schedule) -> Result<(), Refusal> + Send + 'static,
    ) -> Result<Schedule, Refusal> {
        let key = id.to_string();
        let done = self
            .db
            .write(move |tx| {
                let Some(mut s) = find(tx, &key)? else {
                    return Ok(Err(unknown(&key)));
                };
                if let Err(r) = f(&mut s) {
                    return Ok(Err(r));
                }
                put_schedule(tx, &s)?;
                Ok(Ok(s))
            })
            .await
            .map_err(saved)??;
        Ok(done)
    }

    pub async fn update(
        &self,
        id: &str,
        input: ScheduleInput,
    ) -> Result<serde_json::Value, Refusal> {
        let req = schedule::check_input(&input)?;
        let now = (self.clock)();
        let s = self
            .change(id, move |s| {
                s.stack = req.stack;
                s.action = req.action.slug().to_string();
                s.args = req.args;
                if s.when != input.when || (!s.enabled && input.enabled) {
                    // A changed time (or switching it back on) starts from
                    // now: slots of the old plan are not "missed" by the new
                    // one.
                    s.handled_until = now;
                }
                s.when = input.when;
                s.enabled = input.enabled;
                s.note = input.note;
                Ok(())
            })
            .await?;
        let view = self.view(&s, now);
        self.publish_list().await;
        Ok(view)
    }

    /// redesign-schedules-2: skip the schedule's next run, `slot`, which
    /// must be the next run the page showed; the schedule stays on.
    pub async fn skip(&self, id: &str, slot: i64) -> Result<serde_json::Value, Refusal> {
        let now = (self.clock)();
        let name = id.to_string();
        let s = self
            .change(id, move |s| {
                s.handled_until = schedule::skip(s, slot, now).map_err(|why| {
                    Refusal::new(
                        format!("skipping a run of schedule {name}"),
                        why,
                        "reload the schedules page and skip the run it shows next",
                    )
                })?;
                Ok(())
            })
            .await?;
        let view = self.view(&s, now);
        self.publish_list().await;
        Ok(view)
    }

    /// feat-kp11demo-9.5: pause the schedule until `until` (unix seconds),
    /// or resume it with `None`. Its slots before then pass quietly.
    pub async fn pause(&self, id: &str, until: Option<i64>) -> Result<serde_json::Value, Refusal> {
        let now = (self.clock)();
        let until = schedule::pause(until, now).map_err(|why| {
            Refusal::new(
                format!("pausing schedule {id}"),
                why,
                "pick a moment between now and a year from now",
            )
        })?;
        let s = self
            .change(id, move |s| {
                s.paused_until = until;
                Ok(())
            })
            .await?;
        let view = self.view(&s, now);
        self.publish_list().await;
        Ok(view)
    }

    pub async fn delete(&self, id: &str) -> Result<(), Refusal> {
        let key = id.to_string();
        let gone = self
            .db
            .write(move |tx| Ok(tx.execute("DELETE FROM schedules WHERE id = ?1", params![key])?))
            .await
            .map_err(saved)?;
        if gone == 0 {
            return Err(unknown(id));
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

fn saved(e: Error) -> Refusal {
    Refusal::new(
        "saving the schedules",
        e.message,
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

/// The body of `POST /data/schedules/{id}/pause`: `until` in unix seconds,
/// or null to resume.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PauseBody {
    until: Option<i64>,
}

async fn pause_schedule(
    State(s): State<Arc<Scheduler>>,
    UrlPath(id): UrlPath<String>,
    b: Result<Json<PauseBody>, JsonRejection>,
) -> Response {
    let until = match b {
        Ok(Json(b)) => b.until,
        Err(e) => {
            return bad(
                StatusCode::BAD_REQUEST,
                Refusal::new(
                    "pause",
                    format!("the request body does not read: {}", e.body_text()),
                    "send {until: <unix seconds>} to pause, or {until: null} to resume",
                ),
            );
        }
    };
    match s.pause(&id, until).await {
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
        .route("/data/schedules/{id}/pause", post(pause_schedule))
        .with_state(s)
}
