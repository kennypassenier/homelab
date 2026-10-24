//! redesign-flows-6 (3.71.0, the senior review of the Update flow, items 3
//! and 4): the Update flow's ONE job, run by the dashboard's own queue —
//! back up, commit, deploy, verify, and the safety net — so the page only
//! follows it and a closed tab stops nothing. A child of `actions` so it
//! sends its host commands through the same `follow` every job uses (their
//! lines and step marks land in this job's log and progress).
//!
//! The pure half (the moves, the rows, the verdict, the undo records) is
//! `core::updateflow`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chassis::Error;
use chassis::storage::rusqlite::{Connection, Transaction, params};
use homelab_core::ops::pins::StackRuntime;
use homelab_proto::{Command, RpcResponse};

use super::{Actions, JobView, Origin, Stop};
use crate::core::actions::{ActionArgs, ActionKind, ActionRequest, Refusal};
use crate::core::actions_progress::Tracker;
use crate::core::updateflow::{
    self, FlowView, HEALTHY_WITHIN_S, ItemKind, RowState, UpdateItem, UpdateRecord,
};

/// The stack editor's commit of new image lines (fix-231's `StackEdit::
/// Settings {images}`), handed to the queue once the editor exists (mount),
/// so the flow commits exactly the way the stack editor does.
pub trait ImageCommitter: Send + Sync + 'static {
    /// Commit and push `images` (`<app>/<service>` → image line) for
    /// `stack`; the new commit's id.
    fn commit_images(
        &self,
        stack: String,
        images: BTreeMap<String, String>,
        subject: String,
        origin: Origin,
    ) -> super::BoxFut<'_, Result<String, Refusal>>;
}

/// Who started a job, in the words the undo list shows.
fn by_words(o: &Origin) -> String {
    match o {
        Origin::Manual => "a person on the dashboard".into(),
        Origin::Claude { by } => format!("Claude (Live view, {by})"),
        Origin::Schedule { .. } => "a schedule".into(),
        Origin::Batch { .. } => "a batch".into(),
    }
}

impl Actions {
    /// The editor's commit, for the flow (mount).
    pub fn set_committer(&self, c: Arc<dyn ImageCommitter>) {
        let _ = self.inner.committer.set(c);
    }

    /// Where the undo records live (mount: the app database).
    pub fn set_flow_store(&self, db: chassis::storage::AppDb) {
        let _ = self.inner.flow_store.set(db);
    }

    /// How often the verify reads the host, and how long it waits for a
    /// healthy app (tests shorten both; the default is every 5 s for 2 min).
    pub fn set_flow_timing(&self, poll: Duration, within: Duration) {
        let _ = self.inner.flow_timing.set((poll, within));
    }

    fn flow_timing(&self) -> (Duration, Duration) {
        self.inner.flow_timing.get().copied().unwrap_or((
            Duration::from_secs(5),
            Duration::from_secs(HEALTHY_WITHIN_S),
        ))
    }

    /// The updates still offered to undo (`GET /data/update-flows`): the
    /// finished ones of the last [`updateflow::UNDO_DAYS`] days, newest
    /// first, at most [`RECORDS_SHOWN`]. Older rows stay in the database
    /// until the Storage page deletes them.
    pub async fn update_records(&self, stack: Option<&str>) -> Vec<UpdateRecord> {
        let all = self.read_records().await;
        updateflow::undoable(&all, stack, self.now())
    }

    async fn read_records(&self) -> Vec<UpdateRecord> {
        let Some(db) = self.inner.flow_store.get() else {
            return Vec::new();
        };
        let since = self.now() - (updateflow::UNDO_DAYS as i64) * 86_400;
        match db.read(move |c| records_since(c, since)).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("the update records were not read: {}", e.message);
                Vec::new()
            }
        }
    }

    async fn keep_record(&self, rec: UpdateRecord) {
        let Some(db) = self.inner.flow_store.get() else {
            return;
        };
        if let Err(e) = db.write(move |tx| put_record(tx, &rec)).await {
            tracing::warn!("the update record was not kept: {}", e.message);
        }
    }

    fn show_flow(&self, view: &mut JobView, flow: &FlowView) {
        view.flow = serde_json::to_value(flow).ok();
        self.store(view);
    }

    /// One sub-action of the flow (a backup, a deploy of a commit, a pull),
    /// sent through the same machinery a job of its own would use.
    async fn run_sub(
        &self,
        req: ActionRequest,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) -> Result<RpcResponse, Stop> {
        let material = self.material(&req).await.map_err(Stop::Refused)?;
        let commands = crate::core::actions::commands(&req, material).map_err(Stop::Refused)?;
        let mut last = None;
        for c in commands {
            let r = self.follow(c, view, tracker).await?;
            let ok = r.ok;
            last = Some(r);
            if !ok {
                break;
            }
        }
        last.ok_or_else(|| Stop::Link("nothing was sent".into()))
    }

    /// What each touched stack's containers run now, from the host.
    async fn read_runtimes(
        &self,
        stacks: &[String],
    ) -> BTreeMap<String, Result<StackRuntime, String>> {
        let mut out = BTreeMap::new();
        for s in stacks {
            let r = self
                .inner
                .host
                .ask_traced(
                    Command::StackRuntime { stack: s.clone() },
                    self.inner.timeout,
                    None,
                )
                .await;
            let read = match r {
                Ok(r) if r.ok => serde_json::from_str::<serde_json::Value>(&r.message)
                    .ok()
                    .and_then(|v| serde_json::from_value::<StackRuntime>(v["runtime"].clone()).ok())
                    .ok_or_else(|| "the host's answer did not read".to_string()),
                Ok(r) => Err(r.message),
                Err(e) => Err(e),
            };
            out.insert(s.clone(), read);
        }
        out
    }

    /// Read until every app is healthy at its new version or the window
    /// closes; the last verdict either way.
    async fn verify(&self, items: &[UpdateItem]) -> updateflow::Verdict {
        let (poll, within) = self.flow_timing();
        let stacks = updateflow::stacks_of(items);
        let end = tokio::time::Instant::now() + within;
        loop {
            let v = updateflow::judge(items, &self.read_runtimes(&stacks).await);
            if v.healthy || tokio::time::Instant::now() + poll > end {
                return v;
            }
            tokio::time::sleep(poll).await;
        }
    }

    /// The commit of `images` for `stack`, then the deploy of exactly that
    /// commit; the short commit id.
    /// flow-stage-by-stage-1: the commit alone; the deploy is its own stage.
    async fn commit_only(
        &self,
        stack: &str,
        images: BTreeMap<String, String>,
        subject: String,
        view: &JobView,
    ) -> Result<String, String> {
        let committer = self
            .inner
            .committer
            .get()
            .cloned()
            .ok_or_else(|| "the dashboard has no stack editor to commit with".to_string())?;
        committer
            .commit_images(stack.to_string(), images, subject, view.origin.clone())
            .await
            .map_err(|r| format!("the commit to stacks/{stack} was refused: {}", r.why))
    }

    async fn commit_and_deploy(
        &self,
        stack: &str,
        images: BTreeMap<String, String>,
        subject: String,
        view: &mut JobView,
        tracker: &mut Tracker,
        flow: &mut FlowView,
    ) -> Result<String, String> {
        let committer = self
            .inner
            .committer
            .get()
            .cloned()
            .ok_or_else(|| "the dashboard has no stack editor to commit with".to_string())?;
        let sha = committer
            .commit_images(stack.to_string(), images, subject, view.origin.clone())
            .await
            .map_err(|r| format!("the commit to stacks/{stack} was refused: {}", r.why))?;
        let short: String = sha.chars().take(7).collect();
        flow.commits.push(short.clone());
        self.show_flow(view, flow);
        let req = ActionRequest {
            stack: stack.to_string(),
            action: ActionKind::DeployCommit,
            args: ActionArgs {
                commit: Some(sha),
                ..Default::default()
            },
        };
        match self.run_sub(req, view, tracker).await {
            Ok(r) if r.ok => Ok(short),
            Ok(r) => Err(format!("the deploy of {stack} ended: {}", r.message)),
            Err(Stop::Refused(r)) => Err(format!("the deploy of {stack} was refused: {}", r.why)),
            Err(Stop::Link(e)) => Err(format!("the deploy of {stack}: {e}")),
        }
    }

    /// bundle-release-rollout: a release of this repository moves the host
    /// and the dashboard only (Kenny, 2026-10-24: "dat is enkel de homelab
    /// repo he, dus host, admin dashboard en hun deploy, niet de andere
    /// paketten"): update the host to the release, then install it on the
    /// dashboard, whose restart ends the job (arch-self: its end is read
    /// back after the restart). Each part is one sub-job of this one job, so
    /// the page shows one progress view and stops at the first part that
    /// fails.
    pub(super) async fn run_release_rollout(
        &self,
        req: &ActionRequest,
        view: &mut JobView,
    ) -> Result<RpcResponse, Stop> {
        let mut tracker = Tracker::new(self.history().await);
        let tag = req.args.tag.clone();
        let host = crate::core::actions::HOST_TARGET.to_string();
        let parts: [(&str, ActionRequest); 2] = [
            (
                "updating the host",
                ActionRequest {
                    stack: host.clone(),
                    action: ActionKind::UpdateHost,
                    args: ActionArgs {
                        tag: tag.clone(),
                        ..Default::default()
                    },
                },
            ),
            (
                "installing the release on the dashboard",
                ActionRequest {
                    stack: crate::core::actions::SELF_STACK.to_string(),
                    action: ActionKind::ReleaseUpdateNative,
                    args: ActionArgs::default(),
                },
            ),
        ];
        for (i, (what, sub)) in parts.into_iter().enumerate() {
            if i > 0 {
                self.wait_host_link().await;
            }
            // The host's own update keeps its full path (it waits for the
            // shipped version to answer, and rolls back when it does not).
            let ran = if sub.action == ActionKind::UpdateHost {
                Box::pin(self.execute(&sub, view)).await
            } else {
                self.run_sub(sub, view, &mut tracker).await
            };
            match ran {
                Ok(r) if r.ok => {}
                Ok(r) => {
                    return Ok(RpcResponse {
                        id: 0,
                        ok: false,
                        message: format!("The roll-out stopped while {what}: {}", r.message),
                        deferred: None,
                    });
                }
                Err(e) => return Err(e),
            }
        }
        Ok(RpcResponse {
            id: 0,
            ok: true,
            message: format!(
                "Rolled out {}: the host and the dashboard run it.",
                tag.as_deref().unwrap_or("the newest release")
            ),
            deferred: None,
        })
    }

    /// bundle-secret-restart: write the secret, then deploy the stack so the
    /// new value is on the container (a latch file is written on deploy).
    pub(super) async fn run_secret_then_deploy(
        &self,
        req: &ActionRequest,
        view: &mut JobView,
    ) -> Result<RpcResponse, Stop> {
        let mut tracker = Tracker::new(self.history().await);
        let mut write = req.clone();
        write.args.then_deploy = false;
        let r = self.run_sub(write, view, &mut tracker).await?;
        if !r.ok {
            return Ok(r);
        }
        let deploy = ActionRequest {
            stack: req.stack.clone(),
            action: ActionKind::Deploy,
            args: ActionArgs::default(),
        };
        let d = self.run_sub(deploy, view, &mut tracker).await?;
        Ok(RpcResponse {
            message: if d.ok {
                format!(
                    "The secret is written and {} is deployed with it.",
                    req.stack
                )
            } else {
                format!(
                    "The secret is written, but the deploy of {} ended: {}",
                    req.stack, d.message
                )
            },
            ..d
        })
    }

    /// bundle-hosttoml-deploys: restart the host so a host.toml change takes
    /// effect, then deploy every stack whose files changed.
    pub(super) async fn run_restart_then_apply(
        &self,
        req: &ActionRequest,
        view: &mut JobView,
    ) -> Result<RpcResponse, Stop> {
        let mut tracker = Tracker::new(self.history().await);
        let mut restart = req.clone();
        restart.args.then_apply = false;
        // The restart's own path waits until the host answers again.
        let r = Box::pin(self.execute(&restart, view)).await?;
        if !r.ok {
            return Ok(r);
        }
        self.wait_host_link().await;
        let apply = ActionRequest {
            stack: crate::core::actions::HOST_TARGET.to_string(),
            action: ActionKind::Apply,
            args: ActionArgs::default(),
        };
        let apply = match self.apply_leaving_self().await {
            Ok(Some(args)) => ActionRequest { args, ..apply },
            Ok(None) => {
                return Ok(RpcResponse {
                    id: 0,
                    ok: true,
                    message: "The host restarted with its new settings; no stack's files changed."
                        .into(),
                    deferred: None,
                });
            }
            Err(r) => return Err(Stop::Refused(r)),
        };
        match self.run_sub(apply, view, &mut tracker).await {
            Ok(a) => Ok(RpcResponse {
                message: if a.ok {
                    "The host restarted with its new settings and every changed stack is deployed."
                        .into()
                } else {
                    format!("The host restarted, but the deploys ended: {}", a.message)
                },
                ..a
            }),
            Err(e) => Err(e),
        }
    }

    /// The apply of every stack whose files changed, the dashboard's own
    /// left out (its deploy restarts the job's own process); `None` when
    /// nothing else changed. A stack that does not build refuses, as
    /// Deploy all changes does.
    async fn apply_leaving_self(&self) -> Result<Option<ActionArgs>, Refusal> {
        let (plan, _) = self.apply_plan(true).await?;
        let me = crate::core::actions::SELF_STACK;
        if plan.deploy.iter().all(|s| s == me) {
            return Ok(None);
        }
        Ok(Some(ActionArgs {
            leave_out: plan.deploy.iter().any(|s| s == me).then(|| me.to_string()),
            ..Default::default()
        }))
    }

    /// After a part that restarts the host: wait (at most 2 min) until the
    /// host answers again before sending the next part.
    async fn wait_host_link(&self) {
        for _ in 0..60 {
            let r = self
                .inner
                .host
                .ask_traced(Command::Ping, Duration::from_secs(5), None)
                .await;
            if r.is_ok_and(|r| r.ok) {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// The Update flow's job: back up → commit → deploy → verify, and the
    /// safety net. `Ok(ok=false)` for a stop the flow explains itself.
    pub(super) async fn run_update_flow(
        &self,
        req: &ActionRequest,
        view: &mut JobView,
    ) -> Result<RpcResponse, Stop> {
        let items = updateflow::parse_updates(req.args.updates.as_deref().unwrap_or("")).map_err(
            |why| {
                Stop::Refused(Refusal::new(
                    "update-apps",
                    why,
                    "start it from the Update flow",
                ))
            },
        )?;
        let stacks = updateflow::stacks_of(&items);
        let mut flow = FlowView::new(items.clone());
        self.show_flow(view, &flow);
        let history = self.history().await;
        let mut tracker = Tracker::new(history);
        let done = |ok: bool, message: String| RpcResponse {
            id: 0,
            ok,
            message,
            deferred: None,
        };

        // 3 · Back up every touched stack, one row each; a failure stops
        // everything (flow-stage-by-stage-1: stage by stage, stack by stack).
        for s in &stacks {
            let row = format!("backup:{s}");
            flow.set(&row, RowState::Run, None, self.now());
            self.show_flow(view, &flow);
            let r = self
                .run_sub(
                    ActionRequest {
                        stack: s.clone(),
                        action: ActionKind::Backup,
                        args: ActionArgs::default(),
                    },
                    view,
                    &mut tracker,
                )
                .await;
            let why = match r {
                Ok(r) if r.ok => None,
                Ok(r) => Some(format!(
                    "The backup of {s} ended: {}. Nothing was changed.",
                    r.message
                )),
                Err(Stop::Refused(r)) => Some(format!(
                    "The backup of {s} was refused: {}. Nothing was changed.",
                    r.why
                )),
                Err(Stop::Link(e)) => Some(format!("The backup of {s}: {e}. Nothing was changed.")),
            };
            if let Some(why) = why {
                flow.fail(&row, why.clone(), self.now());
                self.show_flow(view, &flow);
                self.keep_flow_record(view, &flow, "failed").await;
                return Ok(done(false, why));
            }
            flow.set(&row, RowState::Ok, Some("backed up".into()), self.now());
            self.show_flow(view, &flow);
        }

        // 4a · Commit the new image lines, one stack after the other.
        flow.deploy_at = Some(self.now());
        let pin_stacks: Vec<String> = stacks
            .iter()
            .filter(|s| !updateflow::images_to(&items, s).is_empty())
            .cloned()
            .collect();
        let mut shas: BTreeMap<String, String> = BTreeMap::new();
        for s in &pin_stacks {
            let row = format!("commit:{s}");
            flow.set(&row, RowState::Run, None, self.now());
            self.show_flow(view, &flow);
            let subject = format!(
                "Update {} (Update flow)",
                items
                    .iter()
                    .filter(|i| &i.stack == s && i.kind == ItemKind::Pin)
                    .map(|i| i.words())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            match self
                .commit_only(s, updateflow::images_to(&items, s), subject, view)
                .await
            {
                Ok(sha) => {
                    let short: String = sha.chars().take(7).collect();
                    flow.commits.push(short.clone());
                    flow.set(&row, RowState::Ok, Some(short), self.now());
                    shas.insert(s.clone(), sha);
                }
                Err(why) => {
                    let why = format!(
                        "{why}. {}",
                        if shas.is_empty() {
                            "Nothing was deployed."
                        } else {
                            "Nothing was deployed; the commits already made stay in the repository."
                        }
                    );
                    flow.fail(&row, why.clone(), self.now());
                    self.show_flow(view, &flow);
                    self.keep_flow_record(view, &flow, "failed").await;
                    return Ok(done(false, why));
                }
            }
            self.show_flow(view, &flow);
        }

        // 4b · Deploy, one stack after the other: its commit, then a pull of
        // its apps on a moving tag.
        for s in &stacks {
            let row = format!("deploy:{s}");
            flow.set(&row, RowState::Run, None, self.now());
            self.show_flow(view, &flow);
            let mut subs: Vec<ActionRequest> = Vec::new();
            if let Some(sha) = shas.get(s) {
                subs.push(ActionRequest {
                    stack: s.clone(),
                    action: ActionKind::DeployCommit,
                    args: ActionArgs {
                        commit: Some(sha.clone()),
                        ..Default::default()
                    },
                });
            }
            for i in items
                .iter()
                .filter(|i| &i.stack == s && i.kind == ItemKind::Pull)
            {
                subs.push(ActionRequest {
                    stack: i.stack.clone(),
                    action: ActionKind::Update,
                    args: ActionArgs {
                        app: i.app.clone(),
                        ..Default::default()
                    },
                });
            }
            for sub in subs {
                let what = if sub.action == ActionKind::DeployCommit {
                    "deploy"
                } else {
                    "update"
                };
                let r = self.run_sub(sub, view, &mut tracker).await;
                let why = match r {
                    Ok(r) if r.ok => None,
                    Ok(r) => Some(format!("The {what} of {s} ended: {}", r.message)),
                    Err(Stop::Refused(r)) => {
                        Some(format!("The {what} of {s} was refused: {}", r.why))
                    }
                    Err(Stop::Link(e)) => Some(format!("The {what} of {s}: {e}")),
                };
                if let Some(why) = why {
                    let why = format!(
                        "{why}. {}",
                        if what == "deploy" {
                            "Roll back… on the result puts the earlier version back."
                        } else {
                            "The host's update rolls an unhealthy app back by itself."
                        }
                    );
                    flow.fail(&row, why.clone(), self.now());
                    self.show_flow(view, &flow);
                    self.keep_flow_record(view, &flow, "failed").await;
                    return Ok(done(false, why));
                }
            }
            flow.set(&row, RowState::Ok, None, self.now());
            self.show_flow(view, &flow);
        }

        // 5 · Verify: every container of every touched stack runs, and every
        // pinned app runs its new version, within 2 min.
        flow.set("health", RowState::Run, None, self.now());
        self.show_flow(view, &flow);
        let v = self.verify(&items).await;
        flow.verdict = v.words.clone();
        if v.healthy {
            flow.set(
                "health",
                RowState::Ok,
                Some(v.words.join(" · ")),
                self.now(),
            );
            flow.step = 6;
            self.show_flow(view, &flow);
            self.keep_flow_record(view, &flow, "done").await;
            return Ok(done(
                true,
                format!(
                    "Updated {}: healthy at the new version. Roll back from the stack's History for {} days.",
                    items
                        .iter()
                        .map(|i| i.words())
                        .collect::<Vec<_>>()
                        .join("; "),
                    updateflow::UNDO_DAYS
                ),
            ));
        }
        let not = format!(
            "Not healthy within {} min: {}",
            self.flow_timing().1.as_secs().div_ceil(60),
            v.words.join(" · ")
        );
        flow.set("health", RowState::Bad, Some(not.clone()), self.now());
        // The safety net: a pinned app's earlier version, back the same way.
        if pin_stacks.is_empty() {
            let why = format!(
                "{not}. An app on a moving tag has no earlier line to put back; open its Logs, or restore the backup from step 3."
            );
            flow.failed = Some(why.clone());
            flow.step = 6;
            self.show_flow(view, &flow);
            self.keep_flow_record(view, &flow, "failed").await;
            return Ok(done(false, why));
        }
        flow.add_rollback();
        flow.set("rollback", RowState::Run, None, self.now());
        self.show_flow(view, &flow);
        let mut back_ok = true;
        let mut back_words = Vec::new();
        for s in &pin_stacks {
            let subject = format!(
                "Roll back {} (Update flow safety net: not healthy within 2 min)",
                items
                    .iter()
                    .filter(|i| &i.stack == s && i.kind == ItemKind::Pin)
                    .map(|i| i.words())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            match self
                .commit_and_deploy(
                    s,
                    updateflow::images_back(&items, s),
                    subject,
                    view,
                    &mut tracker,
                    &mut flow,
                )
                .await
            {
                Ok(c) => back_words.push(format!("{s}: earlier version back ({c})")),
                Err(why) => {
                    back_ok = false;
                    back_words.push(why);
                }
            }
        }
        flow.rolled_back = back_ok;
        flow.set(
            "rollback",
            if back_ok { RowState::Ok } else { RowState::Bad },
            Some(back_words.join(" · ")),
            self.now(),
        );
        let why = if back_ok {
            format!(
                "{not}. The safety net put the earlier version back; the backup from step 3 holds the data as it was."
            )
        } else {
            format!(
                "{not}. Putting the earlier version back failed too: {}. Restore the backup from step 3.",
                back_words.join("; ")
            )
        };
        flow.failed = Some(why.clone());
        flow.step = 6;
        self.show_flow(view, &flow);
        self.keep_flow_record(view, &flow, if back_ok { "rolled back" } else { "failed" })
            .await;
        Ok(done(false, why))
    }

    async fn keep_flow_record(&self, view: &JobView, flow: &FlowView, outcome: &str) {
        self.keep_record(UpdateRecord {
            job: view.job,
            at: view.started_at.unwrap_or_else(|| self.now()),
            by: by_words(&view.origin),
            items: flow.items.clone(),
            commits: flow.commits.clone(),
            outcome: outcome.into(),
        })
        .await;
    }
}

/// The most records one read returns: the undo list is a short list, and
/// a year of updates must not make the page read a year.
pub const RECORDS_SHOWN: i64 = 200;

fn stored(what: &str, e: impl std::fmt::Display) -> Error {
    Error::internal(
        format!("a stored update record does not read: {what}: {e}"),
        "the database file may be damaged; restore the pre-update copy or the last backup",
    )
}

/// The finished records since `since` (unix seconds), newest first.
pub(crate) fn records_since(c: &Connection, since: i64) -> Result<Vec<UpdateRecord>, Error> {
    let mut stmt = c.prepare(
        "SELECT job, at, by_whom, outcome, items, commits FROM update_records \
         WHERE at >= ?1 AND outcome = 'done' ORDER BY at DESC, job DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![since, RECORDS_SHOWN], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(job, at, by, outcome, items, commits)| {
            Ok(UpdateRecord {
                job: job as u64,
                at,
                by,
                items: serde_json::from_str(&items).map_err(|e| stored("items", e))?,
                commits: serde_json::from_str(&commits).map_err(|e| stored("commits", e))?,
                outcome,
            })
        })
        .collect()
}

/// Store one record (a job id is stored once; a repeat replaces it), in the
/// caller's transaction. `stacks` is the touched stacks as `,a,b,`, which the
/// Storage page filters by.
pub(crate) fn put_record(tx: &Transaction, r: &UpdateRecord) -> Result<(), Error> {
    let json = |e: serde_json::Error| stored("a record to store", e);
    tx.execute(
        "INSERT OR REPLACE INTO update_records (job, at, by_whom, outcome, stacks, items, commits) \
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            r.job as i64,
            r.at,
            r.by,
            r.outcome,
            stacks_cell(r),
            serde_json::to_string(&r.items).map_err(json)?,
            serde_json::to_string(&r.commits).map_err(json)?,
        ],
    )?;
    Ok(())
}

/// The `stacks` column: the touched stacks, sorted, as `,a,b,`.
pub(crate) fn stacks_cell(r: &UpdateRecord) -> String {
    let mut stacks: Vec<&str> = r.items.iter().map(|i| i.stack.as_str()).collect();
    stacks.sort_unstable();
    stacks.dedup();
    format!(",{},", stacks.join(","))
}
