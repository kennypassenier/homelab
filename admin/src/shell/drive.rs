//! feat-platform-10 (milestone follow): the driver. `homelab ui <step>`
//! arrives from the host line as `ServerMsg::Ui`; this applies it to the one
//! shared "Claude is driving" state (`core::drive`), runs the final press
//! through the action queue ONCE, pushes the result to every tab over the
//! live channel (`drive`), and answers the host with what is on screen.
//!
//! Tabs never run a driven press themselves: the job exists whether zero,
//! one or two tabs are open, and a tab only shows it.
//!
//! The edit forms (`core::driveedit`) go the same way: what an `open`
//! needs is read through the editor's own reads, and a plan, the data
//! folders and the final press (the commit and push, host.toml, the batch)
//! run through the very functions the routes a click reaches run.
//!
//! Live view (Kenny, 2026-09-29): before a step changes the screen, the
//! driver announces it to every tab and holds it for the countdown here, on
//! the server, so every tab sees the same wait and the CLI's answer comes
//! after the step ran. A viewer's Pause holds the step until Continue (the
//! host is told to wait longer, and the CLI hears who paused); Stop fails it
//! and ends the drive (`core::drivelive`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use homelab_proto::{Command, Scope, ServerMsg, UI_HOLD_MAX_S, UI_RELAY_WAIT_S, UiStep};
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::Instant;

use super::actions::{Actions, Clock, HostPort, Origin, PauseGate, Publish};
use super::edit::{self as ed, EditCtx};
use super::host_link::Shared;
use crate::core::actions::{self as act, ActionKind, Arg, Refusal};
use crate::core::drive::{Applied, Ctx, DriveState, Effect, Family, JobRef, Sources};
use crate::core::driveedit::{self, EditCall, EditKind};
use crate::core::drivelive::{self, Announce, Control};

/// Live view's timing: how long a step is announced, and how long a paused
/// step waits for Continue before it fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveTiming {
    pub announce: Duration,
    pub max_pause: Duration,
}

impl Default for LiveTiming {
    /// No announcement: a driver built without settings (the tests) takes
    /// every step at once; the dashboard sets the configured timing.
    fn default() -> Self {
        LiveTiming {
            announce: Duration::ZERO,
            max_pause: Duration::from_secs(drivelive::MAX_PAUSE_S),
        }
    }
}

/// What the relay does while a step is held: ask the host to wait
/// `wait_s` more seconds and hand the note to the CLI.
pub type Hold<'a> = &'a (dyn Fn(u64, Option<String>) + Send + Sync);

struct Inner {
    state: Mutex<DriveState>,
    /// One step at a time, in the order they came.
    turn: tokio::sync::Mutex<()>,
    actions: Actions,
    shared: Shared,
    publish: Arc<dyn Publish>,
    clock: Clock,
    /// The editor's reads and writes, for the edit forms.
    edit: Option<EditCtx>,
    /// Live view.
    timing: Mutex<LiveTiming>,
    /// A viewer pressed a button: the held step looks again. fix-172: also
    /// handed to the action queue ([`PauseGate::notified`]) so Pause holds
    /// a driven batch at the boundary between its jobs, not only a driven
    /// UI step that has not been sent yet.
    wake: Arc<tokio::sync::Notify>,
    /// Where the running countdown ends; None when none runs (or paused).
    deadline: Mutex<Option<Instant>>,
    announced: AtomicU64,
    /// Who pressed Continue last, for the CLI's note.
    continued_by: Mutex<Option<String>>,
}

#[derive(Clone)]
pub struct Driver {
    inner: Arc<Inner>,
}

impl Driver {
    pub fn new(actions: Actions, shared: Shared, publish: Arc<dyn Publish>, clock: Clock) -> Self {
        Self::with_edit(actions, shared, publish, clock, None)
    }

    /// A driver that can drive the edit forms too.
    pub fn with_edit(
        actions: Actions,
        shared: Shared,
        publish: Arc<dyn Publish>,
        clock: Clock,
        edit: Option<EditCtx>,
    ) -> Self {
        Driver {
            inner: Arc::new(Inner {
                state: Mutex::new(DriveState::default()),
                turn: tokio::sync::Mutex::new(()),
                actions,
                shared,
                publish,
                clock,
                edit,
                timing: Mutex::new(LiveTiming::default()),
                wake: Arc::new(tokio::sync::Notify::new()),
                deadline: Mutex::new(None),
                announced: AtomicU64::new(0),
                continued_by: Mutex::new(None),
            }),
        }
    }

    /// Live view's timing, from the dashboard's settings.
    pub fn set_timing(&self, t: LiveTiming) {
        *self
            .inner
            .timing
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = t;
    }

    /// Decision "23 constants": `idle_s` and `release_after_job_s`, from
    /// `ActConfig` (mount only); `core::drive::IDLE_S` and
    /// `RELEASE_AFTER_JOB_S` otherwise.
    pub fn set_limits(&self, idle_s: i64, release_after_job_s: i64) {
        let mut s = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        s.idle_s = idle_s;
        s.release_after_job_s = release_after_job_s;
    }

    fn timing(&self) -> LiveTiming {
        *self
            .inner
            .timing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn set_deadline(&self, d: Option<Instant>) {
        *self
            .inner
            .deadline
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = d;
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
        if let (Some(a), Some(d)) = (
            s.announce.as_mut(),
            *self
                .inner
                .deadline
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        ) {
            a.left_ms = d.saturating_duration_since(Instant::now()).as_millis() as u64;
        }
        if let Some(f) = s.form.as_mut() {
            if let Some(j) = f.job.as_mut()
                && let Some(v) = self.inner.actions.job(j.job)
            {
                *j = job_ref(&v);
            }
            // A batch's final press answered `{batch}`: the batch as it
            // stands now rides along, so `ui finish` can follow it.
            if let Some(r) = f.edit.as_mut().and_then(|e| e.result.as_mut())
                && let Some(b) = r.get("batch").and_then(Value::as_u64)
                && let Some(v) = self.inner.actions.batch_view(b)
            {
                r["progress"] = v;
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
        // dashboard-latest: a unit picked while install-native's dialog is
        // open re-reads that unit's release list, the same re-fetch the
        // browser's own listener does when the unit field changes — so a
        // later `ui pick act-tag <tag>` is checked against the right repo.
        if let UiStep::Pick { field, value } = step
            && field.as_str() == "act-unit"
            && let Some(stack) = self.open_form_stack("install-native")
            && let Ok(v) = self
                .inner
                .actions
                .release_options(&stack, Some(value.as_str()))
                .await
        {
            return Sources {
                releases: strings(&v["releases"], Some("value")),
                ..Sources::default()
            };
        }
        if let UiStep::Open { form, target } = step
            && let Some(kind) = EditKind::from_form(form)
        {
            // Owner decision 2026-09-30: `open batch <action>` with no
            // stacks named previews the fleet table's own selection.
            let selected;
            let target = match (kind, target.as_deref()) {
                (EditKind::Batch, None) => {
                    selected = self.lock().selected.join(",");
                    Some(selected.as_str())
                }
                (_, t) => t,
            };
            return Sources {
                edit: self.edit_read(kind, form, target).await,
                ..Sources::default()
            };
        }
        // TUI parity: the host-wide forms whose choices come from the host.
        if let UiStep::Open { form, .. } = step {
            match form.as_str() {
                "answer-check" => {
                    return Sources {
                        checks: self.check_ids().await,
                        ..Sources::default()
                    };
                }
                "template-build" => {
                    return Sources {
                        templates: self.os_templates().await,
                        ..Sources::default()
                    };
                }
                // dashboard-latest: update-host's tag dropdown, the
                // homelab repository's own release list.
                "update-host" => {
                    let v = self
                        .inner
                        .actions
                        .release_options(act::HOST_TARGET, None)
                        .await;
                    return Sources {
                        releases: v
                            .map(|v| strings(&v["releases"], Some("value")))
                            .unwrap_or_default(),
                        ..Sources::default()
                    };
                }
                _ => {}
            }
        }
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
        let args = ActionKind::from_slug(form).map(|k| k.args()).unwrap_or(&[]);
        if args.iter().any(|a| matches!(a, Arg::Unit | Arg::Commit))
            && let Ok(v) = self.inner.actions.rollback_options(stack.clone()).await
        {
            out.units = strings(&v["native_units"], None);
            out.commits = strings(&v["commits"], Some("commit"));
        }
        // dashboard-latest: install-native's tag dropdown. The unit is not
        // picked yet at `open` time; a single-service stack still resolves
        // (its release_repo needs no unit named), a multi-service one waits
        // for the Pick branch above.
        if args.contains(&Arg::Tag)
            && let Ok(v) = self.inner.actions.release_options(stack, None).await
        {
            out.releases = strings(&v["releases"], Some("value"));
        }
        out
    }

    /// The stack of the open dialog, when it is driving `action`.
    fn open_form_stack(&self, action: &str) -> Option<String> {
        self.lock()
            .form
            .as_ref()
            .filter(|f| f.action == action)
            .map(|f| f.stack.clone())
    }

    /// The manual checks' ids, as the checks page lists them.
    async fn check_ids(&self) -> Vec<String> {
        let r = self
            .inner
            .actions
            .ask(
                Command::ListManualChecks { json: true },
                Duration::from_secs(60),
            )
            .await;
        r.ok()
            .and_then(|r| serde_json::from_str::<Value>(&r.message).ok())
            .map(|v| strings(&v["checks"], Some("id")))
            .unwrap_or_default()
    }

    /// The host's OS templates, for template-build's base.
    async fn os_templates(&self) -> Vec<String> {
        let r = self
            .inner
            .actions
            .ask(Command::ListTemplates, Duration::from_secs(90))
            .await;
        r.ok()
            .filter(|r| r.ok)
            .map(|r| crate::core::templates::parse(&r.message).os)
            .unwrap_or_default()
    }

    /// What an edit form's `open` reads first, through the editor's own
    /// reads; `{"error": refusal}` when it could not be read.
    async fn edit_read(&self, kind: EditKind, form: &str, target: Option<&str>) -> Value {
        let err = |r: Refusal| json!({ "error": r });
        let Some(c) = self.inner.edit.as_ref() else {
            return err(Refusal::new(
                "the edit forms",
                "this dashboard has no editor",
                "use a dashboard with its working copy",
            ));
        };
        match kind {
            EditKind::Settings
            | EditKind::Raw
            | EditKind::AddApp
            | EditKind::Firewall
            | EditKind::AddNative
            // feat-stacks-9/10/11: also one stack's own edit, the same read.
            | EditKind::SettingsExt
            | EditKind::Apps
            | EditKind::Latch
            | EditKind::Tiles => {
                match target {
                    Some(t) if act::valid_stack_name(t) => ed::read_stack_edit(c, t)
                        .await
                        .unwrap_or_else(|(_, r)| err(r)),
                    _ => Value::Null,
                }
            }
            // feat-checks-1/feat-publish-1: `<stack>/<app>` — only the
            // stack part is a stack name; the same read has every app's
            // checks.yml (`e.checks`), which is all either form needs.
            EditKind::Checks | EditKind::PublishApp => {
                let stack = target.map(|t| t.split_once('/').map_or(t, |(s, _)| s));
                match stack {
                    Some(t) if act::valid_stack_name(t) => ed::read_stack_edit(c, t)
                        .await
                        .unwrap_or_else(|(_, r)| err(r)),
                    _ => Value::Null,
                }
            }
            // feat-native-1: `<stack>[/<unit>]` — only the stack part is a
            // stack name, and the same read has every unit's manifest.
            EditKind::Native => {
                let stack = target.map(|t| t.split_once('/').map_or(t, |(s, _)| s));
                match stack {
                    Some(t) if act::valid_stack_name(t) => ed::read_stack_edit(c, t)
                        .await
                        .unwrap_or_else(|(_, r)| err(r)),
                    _ => Value::Null,
                }
            }
            // feat-preset-1: an existing preset's `preset.yml` and files;
            // a new preset reads nothing (the name is typed in the form).
            EditKind::Preset => match target {
                Some(t) => ed::read_preset_edit(c, t).await.unwrap_or_else(|(_, r)| err(r)),
                None => Value::Null,
            },
            EditKind::NewPreset => Value::Null,
            // The taken names and numbers, and whether there is a working
            // copy, as the new-stack wizard reads them.
            EditKind::NewStack | EditKind::Import => ed::read_presets(c).await,
            EditKind::HostSettings => ed::read_host_settings(c)
                .await
                .unwrap_or_else(|(_, r)| err(r)),
            EditKind::Rollback => match target {
                Some(t) if act::valid_stack_name(t) => self
                    .inner
                    .actions
                    .rollback_options(t.to_string())
                    .await
                    .unwrap_or_else(err),
                _ => Value::Null,
            },
            EditKind::Batch => {
                // Each stack's own preview, as the dialog reads it: whether
                // its deploy guard refuses (force is offered only then).
                let action = form.strip_prefix("batch:").unwrap_or("");
                let Some(kind) = ActionKind::from_slug(action) else {
                    return Value::Null;
                };
                let mut guarded = 0;
                for s in target.unwrap_or("").split(',').map(str::trim) {
                    if !act::valid_stack_name(s) {
                        continue;
                    }
                    let args = act::ActionArgs {
                        confirm: kind.confirm().then(|| s.to_string()),
                        ..Default::default()
                    };
                    if let Ok(req) = act::validate(s, action, args)
                        && self.inner.actions.preview_of(req, false).await.1.is_some() {
                            guarded += 1;
                        }
                }
                json!({ "guarded": guarded })
            }
        }
    }

    /// An edit form's call to the dashboard's server, through the route's
    /// own function; the answer goes into the form.
    async fn edit_call(&self, by: &str, call: EditCall) -> Option<Refusal> {
        let Some(c) = self.inner.edit.clone() else {
            let r = Refusal::new(
                "the edit forms",
                "this dashboard has no editor",
                "use a dashboard with its working copy",
            );
            if let Some(f) = self.lock().form.as_mut() {
                driveedit::done(f, &call, Err(r.clone()));
            }
            return Some(r);
        };
        let origin = || Origin::Claude { by: by.to_string() };
        let parse = |what: &str, v: &Value| -> Result<Value, Refusal> {
            Err(Refusal::new(
                what,
                format!("the driven form built a body the server does not read: {v}"),
                "report this with the dashboard's log",
            ))
        };
        let outcome: Result<Value, Refusal> = match &call {
            EditCall::Plan { stack, edit } => match serde_json::from_value(edit.clone()) {
                Ok(e) => ed::plan_stack(&c, stack, e).await.map_err(|(_, r)| r),
                Err(_) => parse("the plan", edit),
            },
            EditCall::Commit { stack, body } => match serde_json::from_value(body.clone()) {
                Ok(b) => ed::commit_stack(&c, stack, b, origin())
                    .await
                    .map_err(|(_, r)| r),
                Err(_) => parse("the commit", body),
            },
            EditCall::Appdata { body } => match serde_json::from_value(body.clone()) {
                Ok(b) => Ok(ed::appdata_paths(&c, b).await),
                Err(_) => Ok(json!({ "appdata": [] })),
            },
            EditCall::NewPlan { body } => match serde_json::from_value(body.clone()) {
                Ok(b) => ed::plan_new(&c, b).await.map_err(|(_, r)| r),
                Err(_) => parse("the new stack's plan", body),
            },
            EditCall::NewCommit { body } => match serde_json::from_value(body.clone()) {
                Ok(b) => ed::commit_new(&c, b, origin()).await.map_err(|(_, r)| r),
                Err(_) => parse("the new stack", body),
            },
            EditCall::HostWrite { body } => match serde_json::from_value(body.clone()) {
                Ok(b) => ed::save_host_settings(&c, b, origin())
                    .await
                    .map_err(|(_, r)| r),
                Err(_) => parse("the host settings", body),
            },
            EditCall::Batch { body } => match serde_json::from_value(body.clone()) {
                Ok(b) => self.inner.actions.run_batch(b).map_err(|(_, r)| r),
                Err(_) => parse("the batch", body),
            },
            EditCall::ImportPlan { body } => match serde_json::from_value(body.clone()) {
                Ok(b) => ed::plan_import(&c, b).await.map_err(|(_, r)| r),
                Err(_) => parse("the import", body),
            },
            EditCall::ImportCommit { body } => match serde_json::from_value(body.clone()) {
                Ok(b) => ed::commit_import(&c, b, origin()).await.map_err(|(_, r)| r),
                Err(_) => parse("the import", body),
            },
            EditCall::PresetPlan { edit } => match serde_json::from_value(edit.clone()) {
                Ok(e) => ed::plan_preset(&c, e).await.map_err(|(_, r)| r),
                Err(_) => parse("the preset's plan", edit),
            },
            EditCall::PresetCommit { body } => match serde_json::from_value(body.clone()) {
                Ok(b) => ed::commit_preset(&c, b).await.map_err(|(_, r)| r),
                Err(_) => parse("the preset's commit", body),
            },
        };
        if call.final_press() {
            match &outcome {
                Ok(_) => {
                    tracing::info!(by, call = ?std::mem::discriminant(&call), "a driven edit's final press ran")
                }
                Err(r) => {
                    tracing::info!(by, why = %r.why, "a driven edit's final press was refused")
                }
            }
        }
        let refusal = outcome.as_ref().err().cloned();
        let job = outcome
            .as_ref()
            .ok()
            .and_then(|v| v["follow"]["job"].as_u64())
            .and_then(|j| self.inner.actions.job(j));
        let mut st = self.lock();
        if let Some(f) = st.form.as_mut() {
            driveedit::done(f, &call, outcome);
            if let Some(v) = job {
                f.job = Some(job_ref(&v));
                f.refresh();
            }
        }
        // A plan that could not be made is shown in the dialog, as a click
        // shows it; only a refused final press is the driver's refusal.
        refusal.filter(|_| call.final_press())
    }

    /// Apply one step and answer `{ok, refusal, state}`.
    pub async fn step(&self, by: &str, scope: Scope, step: UiStep) -> Value {
        self.step_held(by, scope, step, &|_, _| {}).await
    }

    /// [`Driver::step`], announced and held first (Live view); `hold` asks
    /// the host to wait longer while a viewer has paused.
    pub async fn step_held(&self, by: &str, scope: Scope, step: UiStep, hold: Hold<'_>) -> Value {
        // Reading the screen never waits behind a held step.
        if step == UiStep::State {
            return reply(None, &self.snapshot());
        }
        let _turn = self.inner.turn.lock().await;
        let stacks = self.stacks().await;
        let sources = self.sources(&step).await;
        let cx = Ctx {
            now: self.now(),
            by,
            scope,
            stacks: &stacks,
            sources: &sources,
        };
        let applied = match self.held(&step, &cx, hold).await {
            Ok(applied) => applied,
            Err(r) => Err(r),
        };
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
                Effect::Run(args) => self.run(by, *args).await,
                Effect::Edit(call) => self.edit_call(by, call).await.or(held),
            },
        };
        tracing::info!(by, step = step.verb(), "a driven step was applied");
        self.publish(&step, true, refusal.as_ref());
        reply(refusal.as_ref(), &self.snapshot())
    }

    /// Live view: announce `step`, hold it for the countdown and for as
    /// long as a viewer has paused, then apply it, all against the one
    /// state. The pause and the stop are checked under the same lock the
    /// step is applied under, so a step is taken once or not at all.
    /// `Err`: the step was not taken (stopped, or paused too long).
    async fn held(
        &self,
        step: &UiStep,
        cx: &Ctx<'_>,
        hold: Hold<'_>,
    ) -> Result<Result<Applied, Refusal>, Refusal> {
        let t = self.timing();
        let countdown = drivelive::counts_down(step) && !t.announce.is_zero();
        {
            let mut st = self.lock();
            if !drivelive::holds(step) || (!countdown && st.paused_by.is_none()) {
                return Ok(st.apply(step, cx));
            }
            // A step the dashboard will refuse is refused now, not announced.
            if let Err(r) = st.clone().apply(step, cx) {
                return Ok(Err(r));
            }
            let total = if countdown {
                t.announce
            } else {
                Duration::ZERO
            };
            st.announce = Some(Announce {
                id: self.inner.announced.fetch_add(1, Ordering::Relaxed) + 1,
                step: step.clone(),
                text: drivelive::describe(step, &st),
                countdown,
                total_ms: total.as_millis() as u64,
                left_ms: total.as_millis() as u64,
            });
        }
        let mut left = if countdown {
            t.announce
        } else {
            Duration::ZERO
        };
        let mut deadline = Instant::now() + left;
        let mut paused: Option<(Instant, String)> = None;
        {
            let paused_now = self.lock().paused_by.is_some();
            self.set_deadline((!paused_now).then_some(deadline));
        }
        self.publish_live("announce");
        let max_pause_s = t.max_pause.as_secs();
        loop {
            let woken = self.inner.wake.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            let (paused_by, stopped_by) = {
                let st = self.lock();
                (st.paused_by.clone(), st.stopped_by.clone())
            };
            if let Some(who) = stopped_by {
                self.set_deadline(None);
                return Err(drivelive::stopped_refusal(step, &who));
            }
            match paused_by {
                Some(who) => {
                    let since = match &paused {
                        Some((since, _)) => *since,
                        None => {
                            left = deadline.saturating_duration_since(Instant::now());
                            self.set_deadline(None);
                            if let Some(a) = self.lock().announce.as_mut() {
                                a.left_ms = left.as_millis() as u64;
                            }
                            hold(
                                max_pause_s + UI_RELAY_WAIT_S + left.as_secs() + 30,
                                Some(format!(
                                    "paused by {who}: `ui {}` waits for Continue, at most {} min",
                                    step.verb(),
                                    max_pause_s / 60
                                )),
                            );
                            tracing::info!(viewer = %who, step = step.verb(), "a viewer paused a driven step");
                            let now = Instant::now();
                            paused = Some((now, who.clone()));
                            self.publish_live("control");
                            now
                        }
                    };
                    tokio::select! {
                        _ = &mut woken => continue,
                        _ = tokio::time::sleep_until(since + t.max_pause) => {
                            {
                                let mut st = self.lock();
                                st.announce = None;
                                st.paused_by = None;
                            }
                            self.set_deadline(None);
                            self.publish_live("control");
                            tracing::info!(viewer = %who, step = step.verb(), "a paused step waited past the longest pause");
                            return Err(drivelive::pause_expired(step, &who, max_pause_s));
                        }
                    }
                }
                None => {
                    if paused.take().is_some() {
                        let who = self
                            .inner
                            .continued_by
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone()
                            .unwrap_or_else(|| "a viewer".into());
                        if countdown {
                            left = left.max(Duration::from_secs(1));
                        }
                        deadline = Instant::now() + left;
                        self.set_deadline(Some(deadline));
                        hold(
                            UI_RELAY_WAIT_S + left.as_secs() + 1,
                            Some(format!("continued by {who}")),
                        );
                        self.publish_live("control");
                    }
                    if Instant::now() >= deadline {
                        let mut st = self.lock();
                        if st.paused_by.is_some() {
                            continue;
                        }
                        if let Some(who) = &st.stopped_by {
                            return Err(drivelive::stopped_refusal(step, who));
                        }
                        st.announce = None;
                        self.set_deadline(None);
                        return Ok(st.apply(step, cx));
                    }
                    tokio::select! {
                        _ = &mut woken => {}
                        _ = tokio::time::sleep_until(deadline) => {}
                    }
                }
            }
        }
    }

    /// A viewer pressed Pause, Continue or Stop in the announcement bar.
    pub fn control(&self, c: Control, who: &str) -> Result<DriveState, Refusal> {
        // Read before Stop clears the form: the batch its final press began.
        let batch = self
            .lock()
            .form
            .as_ref()
            .and_then(|f| f.edit.as_ref())
            .and_then(|e| e.result.as_ref())
            .and_then(|r| r.get("batch"))
            .and_then(Value::as_u64);
        self.lock().control(c, who, self.now())?;
        tracing::info!(
            viewer = who,
            button = c.word(),
            "a viewer pressed a Live view button"
        );
        match c {
            Control::Continue => {
                *self
                    .inner
                    .continued_by
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(who.to_string());
                self.publish_live("control");
            }
            Control::Pause => self.publish_live("control"),
            // The drive ended: every tab closes the dialog, as on `done`.
            // A batch the drive started stops too: the job running now
            // ends on its own, the queued ones never start.
            Control::Stop => {
                if let Some(b) = batch {
                    let n = self.inner.actions.stop_batch(b, who);
                    tracing::info!(
                        viewer = who,
                        batch = b,
                        refused = n,
                        "Stop ended a driven batch"
                    );
                }
                self.set_deadline(None);
                self.publish(&UiStep::Done, true, None);
            }
        }
        self.inner.wake.notify_waiters();
        Ok(self.snapshot())
    }

    /// fix-172: the action queue asks this before it takes the next job
    /// off the queue, so a viewer's Pause holds a driven batch at the
    /// boundary between its jobs the same way it already holds a driven UI
    /// step that has not been sent yet.
    fn paused_by_now(&self) -> Option<String> {
        self.lock().paused_by.clone()
    }

    /// The announcement or the pause changed; no step was taken.
    fn publish_live(&self, kind: &str) {
        let state = self.snapshot();
        self.inner.publish.publish(
            "drive",
            json!({
                "kind": kind,
                "seq": state.seq,
                "step": state.announce.as_ref().map(|a| &a.step),
                "applied": false,
                "refusal": null,
                "state": state,
            }),
        );
    }

    fn publish(&self, step: &UiStep, applied: bool, refusal: Option<&Refusal>) {
        let state = self.snapshot();
        self.inner.publish.publish(
            "drive",
            json!({
                "kind": "step",
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
        let Some((stack, action, mut args, confirm)) = self.lock().form.as_ref().and_then(|f| {
            let Family::Action(kind) = f.desc.family else {
                return None;
            };
            Some((
                f.stack.clone(),
                f.action.clone(),
                crate::core::drive::build_args_of(f.desc.fields(), &f.values),
                kind.confirm(),
            ))
        }) else {
            return;
        };
        // As the preview route: a typed name not typed yet is filled in
        // (the line then leaves the name to the CLI), and a wipe keeps its
        // list-only meaning unless the name was typed right.
        let typed = args.confirm.as_deref() == Some(stack.as_str());
        if confirm {
            args.confirm = Some(stack.clone());
        } else if !typed {
            args.confirm = None;
        }
        let Ok(req) = act::validate(&stack, &action, args) else {
            return;
        };
        let (line, guard, restarts) = self.inner.actions.preview_of(req, typed).await;
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

    /// fix-163 (Kenny, 2026-09-29): after `ui press confirm` the dialog shows
    /// the job's end; when no step follows within 30 s of it, close the
    /// dialog and give the tabs back, as `homelab ui done` does, so the
    /// viewer is not locked out until the 10-minute idle release. `true`
    /// when it released. A step being taken now is never cut short.
    pub fn release_if_done(&self) -> bool {
        let Ok(_turn) = self.inner.turn.try_lock() else {
            return false;
        };
        let job = self
            .lock()
            .form
            .as_ref()
            .and_then(|f| f.job.as_ref())
            .map(|j| j.job);
        let Some(job) = job else {
            return false;
        };
        let ended = self
            .inner
            .actions
            .job(job)
            .filter(|v| {
                !matches!(
                    v.state,
                    super::actions::JobState::Queued | super::actions::JobState::Running
                )
            })
            .and_then(|v| v.finished_at);
        let now = self.now();
        {
            let mut st = self.lock();
            if !st.release_due(now, ended) {
                return false;
            }
            st.release(now);
        }
        tracing::info!(
            job,
            "a confirmed dialog's job ended and no step followed: the drive was released"
        );
        self.publish(&UiStep::Done, true, None);
        true
    }

    /// fix-163: look every few seconds whether a finished drive is due to
    /// be released.
    pub fn spawn_release(&self) {
        let driver = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                driver.release_if_done();
            }
        });
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
                        // Each step on its own task: a held step (Live view)
                        // must not keep `ui state` or a viewer's button
                        // waiting. Steps still run one at a time (`turn`).
                        let (driver, host) = (driver.clone(), host.clone());
                        tokio::spawn(async move {
                            let asker = host.clone();
                            let hold = move |wait_s: u64, note: Option<String>| {
                                let host = asker.clone();
                                tokio::spawn(async move {
                                    let r = host
                                        .ask_traced(
                                            Command::UiHold {
                                                relay,
                                                wait_s: wait_s.min(UI_HOLD_MAX_S),
                                                note,
                                            },
                                            Duration::from_secs(10),
                                            None,
                                        )
                                        .await;
                                    if let Err(e) = r {
                                        tracing::warn!(relay, error = %e, "the host was not told to hold a UI step");
                                    }
                                });
                            };
                            let answer = driver.step_held(&by, scope, step, &hold).await;
                            let ok = answer["ok"] == Value::Bool(true);
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

#[derive(serde::Deserialize)]
struct ControlBody {
    #[serde(rename = "do")]
    what: Control,
}

/// Who pressed a Live view button, as the driver reads it after "paused
/// by": "the viewer <the Cloudflare Access login of the request>" (lock 1
/// verified its token before this route runs), else "a viewer". A label,
/// never a credential.
fn viewer(headers: &HeaderMap) -> String {
    headers
        .get("cf-access-jwt-assertion")
        .and_then(|v| v.to_str().ok())
        .and_then(|t| crate::core::access::parse_jwt(t).ok())
        .and_then(|j| j.claims.email)
        .map(|e| format!("the viewer {e}"))
        .unwrap_or_else(|| "a viewer".into())
}

/// Pause, Continue or Stop, from the announcement bar of any tab in Live
/// view; anyone logged in may press them.
async fn control(
    State(d): State<Driver>,
    headers: HeaderMap,
    Json(body): Json<ControlBody>,
) -> Response {
    match d.control(body.what, &viewer(&headers)) {
        Ok(state) => Json(json!({ "state": state })).into_response(),
        Err(r) => (StatusCode::CONFLICT, Json(r)).into_response(),
    }
}

/// fix-172: the action queue's view of Live view's pause, wired in
/// `shell::actions::mount` once both the queue and the driver exist
/// (`Actions::set_pause_gate`).
impl PauseGate for Driver {
    fn paused_by(&self) -> Option<String> {
        self.paused_by_now()
    }

    fn notified(&self) -> Arc<tokio::sync::Notify> {
        self.inner.wake.clone()
    }
}

/// Mounted with `dashboard_routes`: behind the login and both locks.
pub fn router(driver: Driver) -> Router {
    Router::new()
        .route("/data/drive", get(state))
        .route("/data/drive/control", post(control))
        .with_state(driver)
}

/// The demo host only (`HOMELAB_ADMIN_DEMO_HOST`, in a `demo-host` build): a
/// step as if the host had relayed it, for the browser tests. Never mounted
/// beside a real host.
#[cfg(feature = "demo-host")]
async fn demo_step(State(d): State<Driver>, Json(step): Json<UiStep>) -> Json<Value> {
    Json(d.step("demo", Scope::Operate, step).await)
}

#[cfg(feature = "demo-host")]
pub fn demo_router(driver: Driver) -> Router {
    Router::new()
        .route("/data/drive/demo-step", axum::routing::post(demo_step))
        .with_state(driver)
}
