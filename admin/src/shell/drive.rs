//! feat-platform-10 (milestone follow): the driver. `homelab ui <step>`
//! arrives from the host line as `ServerMsg::Ui`; this applies it to the one
//! shared "Claude is driving" state (`core::drive`), runs the final press
//! through the action queue ONCE, pushes the result to every tab over the
//! live channel (`drive`), and answers the host with what is on screen.
//!
//! Tabs never run a driven press themselves: the job exists whether zero,
//! one or two tabs are open, and a tab only shows it.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use homelab_proto::{Command, Scope, ServerMsg, UiStep};
use serde_json::{json, Value};
use tokio::sync::broadcast::error::RecvError;

use super::actions::{Actions, Clock, HostPort, Origin, Publish};
use super::host_link::Shared;
use crate::core::actions::{self as act, ActionKind, Arg, Refusal};
use crate::core::drive::{Applied, Ctx, DriveState, Effect, JobRef, Sources};

struct Inner {
    state: Mutex<DriveState>,
    /// One step at a time, in the order they came.
    turn: tokio::sync::Mutex<()>,
    actions: Actions,
    shared: Shared,
    publish: Arc<dyn Publish>,
    clock: Clock,
}

#[derive(Clone)]
pub struct Driver {
    inner: Arc<Inner>,
}

impl Driver {
    pub fn new(actions: Actions, shared: Shared, publish: Arc<dyn Publish>, clock: Clock) -> Self {
        Driver {
            inner: Arc::new(Inner {
                state: Mutex::new(DriveState::default()),
                turn: tokio::sync::Mutex::new(()),
                actions,
                shared,
                publish,
                clock,
            }),
        }
    }

    fn now(&self) -> i64 {
        (self.inner.clock)()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DriveState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The state as a tab or `homelab ui state` reads it now, the press's
    /// job as the queue has it.
    pub fn snapshot(&self) -> DriveState {
        let mut s = self.lock().snapshot(self.now());
        if let Some(f) = s.form.as_mut() {
            if let Some(j) = f.job.as_mut() {
                if let Some(v) = self.inner.actions.job(j.job) {
                    *j = job_ref(&v);
                }
            }
        }
        s
    }

    async fn stacks(&self) -> Vec<String> {
        self.inner
            .shared
            .read()
            .await
            .fleet
            .as_ref()
            .map(|f| f.stacks.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default()
    }

    /// The lists an `open` fills its choices from: the fleet's apps, and the
    /// working copy's commits and native units when the form asks.
    async fn sources(&self, step: &UiStep) -> Sources {
        let UiStep::Open {
            form,
            target: Some(stack),
        } = step
        else {
            return Sources::default();
        };
        let mut out = Sources {
            apps: self
                .inner
                .shared
                .read()
                .await
                .fleet
                .as_ref()
                .and_then(|f| f.stacks.iter().find(|s| &s.name == stack))
                .map(|s| s.apps.iter().map(|a| a.name.clone()).collect())
                .unwrap_or_default(),
            ..Sources::default()
        };
        let wants = ActionKind::from_slug(form)
            .map(|k| {
                k.args()
                    .iter()
                    .any(|a| matches!(a, Arg::Unit | Arg::Commit))
            })
            .unwrap_or(false);
        if wants {
            if let Ok(v) = self.inner.actions.rollback_options(stack.clone()).await {
                out.units = strings(&v["native_units"], None);
                out.commits = strings(&v["commits"], Some("commit"));
            }
        }
        out
    }

    /// Apply one step and answer `{ok, refusal, state}`.
    pub async fn step(&self, by: &str, scope: Scope, step: UiStep) -> Value {
        let _turn = self.inner.turn.lock().await;
        if step == UiStep::State {
            return reply(None, &self.snapshot());
        }
        let stacks = self.stacks().await;
        let sources = self.sources(&step).await;
        let cx = Ctx {
            now: self.now(),
            by,
            scope,
            stacks: &stacks,
            sources: &sources,
        };
        let applied = self.lock().apply(&step, &cx);
        let refusal = match applied {
            Err(r) => {
                tracing::info!(by, step = step.verb(), why = %r.why, "a driven step was refused");
                self.publish(&step, false, Some(&r));
                return reply(Some(&r), &self.snapshot());
            }
            Ok(Applied { effect, held }) => match effect {
                Effect::None => held,
                Effect::Preview => {
                    self.preview().await;
                    held
                }
                Effect::Run(args) => self.run(by, args).await,
            },
        };
        tracing::info!(by, step = step.verb(), "a driven step was applied");
        self.publish(&step, true, refusal.as_ref());
        reply(refusal.as_ref(), &self.snapshot())
    }

    fn publish(&self, step: &UiStep, applied: bool, refusal: Option<&Refusal>) {
        let state = self.snapshot();
        self.inner.publish.publish(
            "drive",
            json!({
                "seq": state.seq,
                "step": step,
                "applied": applied,
                "refusal": refusal,
                "state": state,
            }),
        );
    }

    /// The review step is on screen: the CLI line and the deploy guard, read
    /// the way the dialog's own preview reads them.
    async fn preview(&self) {
        let Some((stack, action, mut args, confirm)) = self.lock().form.as_ref().map(|f| {
            (
                f.stack.clone(),
                f.action.clone(),
                crate::core::drive::build_args(&f.desc, &f.values),
                f.desc.action.confirm(),
            )
        }) else {
            return;
        };
        // As `previewArgs`: a typed name not typed yet is filled in, and a
        // wipe keeps its list-only meaning unless the name was typed right.
        if confirm {
            args.confirm = Some(stack.clone());
        } else if args.confirm.as_deref().is_some_and(|c| c != stack) {
            args.confirm = None;
        }
        let Ok(req) = act::validate(&stack, &action, args) else {
            return;
        };
        let (line, guard, restarts) = self.inner.actions.preview_of(req).await;
        let mut st = self.lock();
        if let Some(f) = st.form.as_mut() {
            f.cli = line.ok();
            f.guard = guard;
            f.restarts_dashboard = restarts;
            f.refresh();
        }
    }

    /// The final press: exactly one job, through the same checks a click
    /// gets. What refuses is told to the driver and shown in the dialog.
    async fn run(&self, by: &str, args: act::ActionArgs) -> Option<Refusal> {
        let (stack, action) = {
            let st = self.lock();
            let f = st.form.as_ref()?;
            (f.stack.clone(), f.action.clone())
        };
        let outcome = match act::validate(&stack, &action, args) {
            Ok(req) => {
                self.inner
                    .actions
                    .press(req, Origin::Claude { by: by.to_string() })
                    .await
            }
            Err(r) => Err(r),
        };
        let mut st = self.lock();
        let f = st.form.as_mut()?;
        match outcome {
            Ok(view) => {
                tracing::info!(by, job = view.job, %stack, %action, "a driven press started a job");
                f.job = Some(job_ref(&view));
                f.run_error = None;
                f.refresh();
                None
            }
            Err(r) => {
                f.run_error = Some(r.clone());
                f.refresh();
                Some(r)
            }
        }
    }

    /// feat-platform-10: answer the host's `Ui` frames, one after the other,
    /// for as long as the process runs.
    pub fn spawn_relay(&self, host: Arc<dyn HostPort>) {
        let driver = self.clone();
        let mut rx = host.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(ServerMsg::Ui {
                        relay,
                        by,
                        scope,
                        step,
                    }) => {
                        let answer = driver.step(&by, scope, step).await;
                        let ok = answer["ok"] == Value::Bool(true);
                        let host = host.clone();
                        tokio::spawn(async move {
                            let r = host
                                .ask_traced(
                                    Command::UiReply {
                                        relay,
                                        ok,
                                        message: answer.to_string(),
                                    },
                                    Duration::from_secs(10),
                                    None,
                                )
                                .await;
                            if let Err(e) = r {
                                tracing::warn!(relay, error = %e, "the answer to a UI step did not reach the host");
                            }
                        });
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        });
    }
}

fn strings(v: &Value, key: Option<&str>) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| match key {
            Some(k) => x.get(k)?.as_str().map(str::to_string),
            None => x.as_str().map(str::to_string),
        })
        .collect()
}

fn job_ref(v: &super::actions::JobView) -> JobRef {
    JobRef {
        job: v.job,
        state: serde_json::to_value(v.state)
            .ok()
            .and_then(|s| s.as_str().map(str::to_string))
            .unwrap_or_default(),
        message: v.message.clone(),
        progress: v.progress.as_ref().map(|p| match p.m {
            Some(m) => format!("step {}/{m}: {}", p.n, p.step),
            None => format!("step {}: {}", p.n, p.step),
        }),
    }
}

fn reply(refusal: Option<&Refusal>, state: &DriveState) -> Value {
    json!({ "ok": refusal.is_none(), "refusal": refusal, "state": state })
}

// ── routes ──────────────────────────────────────────────────────────────

/// A tab that turns following on, or reconnects, catches up from here.
async fn state(State(d): State<Driver>) -> Json<Value> {
    Json(json!({ "state": d.snapshot() }))
}

/// Mounted with `dashboard_routes`: behind the login and both locks.
pub fn router(driver: Driver) -> Router {
    Router::new()
        .route("/data/drive", get(state))
        .with_state(driver)
}

/// The demo host only (`HOMELAB_ADMIN_DEMO_HOST`): a step as if the host had
/// relayed it, for the browser tests. Never mounted beside a real host.
async fn demo_step(State(d): State<Driver>, Json(step): Json<UiStep>) -> Response {
    Json(d.step("demo", Scope::Operate, step).await).into_response()
}

pub fn demo_router(driver: Driver) -> Router {
    Router::new()
        .route("/data/drive/demo-step", post(demo_step))
        .with_state(driver)
}
