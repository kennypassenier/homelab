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

    /// Where the undo records live (mount: beside the notifications file).
    pub fn set_flow_store(&self, path: std::path::PathBuf) {
        let _ = self.inner.flow_store.set(path);
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

    /// The updates still offered to undo (`GET /data/update-flows`).
    pub fn update_records(&self, stack: Option<&str>) -> Vec<UpdateRecord> {
        let all = self.read_records();
        updateflow::undoable(&all, stack, self.now())
    }

    fn read_records(&self) -> Vec<UpdateRecord> {
        let Some(path) = self.inner.flow_store.get() else {
            return Vec::new();
        };
        super::super::actions_state::read_json::<Vec<UpdateRecord>>(path)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn keep_record(&self, rec: UpdateRecord) {
        let Some(path) = self.inner.flow_store.get() else {
            return;
        };
        let mut all = self.read_records();
        all.push(rec);
        updateflow::prune(&mut all, self.now());
        if let Err(e) = super::super::actions_state::write_json(path, &all) {
            tracing::warn!("the update record was not kept: {e}");
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

        // 3 · Back up every touched stack; a failure stops everything.
        flow.set("backup", RowState::Run, None, self.now());
        self.show_flow(view, &flow);
        for s in &stacks {
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
                flow.fail("backup", why.clone(), self.now());
                self.show_flow(view, &flow);
                self.keep_flow_record(view, &flow, "failed");
                return Ok(done(false, why));
            }
        }
        flow.set(
            "backup",
            RowState::Ok,
            Some(format!("{} backed up", stacks.len())),
            self.now(),
        );
        self.show_flow(view, &flow);

        // 4 · Commit the new image lines and deploy that commit; pull the
        // apps on a moving tag.
        flow.deploy_at = Some(self.now());
        let pin_stacks: Vec<String> = stacks
            .iter()
            .filter(|s| !updateflow::images_to(&items, s).is_empty())
            .cloned()
            .collect();
        if !pin_stacks.is_empty() {
            flow.set("commit", RowState::Run, None, self.now());
            flow.set("deploy", RowState::Run, None, self.now());
            self.show_flow(view, &flow);
        }
        for s in &pin_stacks {
            let subject = format!(
                "Update {} (Update flow)",
                items
                    .iter()
                    .filter(|i| &i.stack == s && i.kind == ItemKind::Pin)
                    .map(|i| i.words())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if let Err(why) = self
                .commit_and_deploy(
                    s,
                    updateflow::images_to(&items, s),
                    subject,
                    view,
                    &mut tracker,
                    &mut flow,
                )
                .await
            {
                let row = if why.contains("commit to") || why.contains("stack editor") {
                    "commit"
                } else {
                    "deploy"
                };
                if row == "deploy" {
                    flow.set("commit", RowState::Ok, None, self.now());
                }
                let why = format!(
                    "{why}. {}",
                    if row == "commit" {
                        "Nothing was deployed."
                    } else {
                        "Roll back… on the result puts the earlier version back."
                    }
                );
                flow.fail(row, why.clone(), self.now());
                self.show_flow(view, &flow);
                self.keep_flow_record(view, &flow, "failed");
                return Ok(done(false, why));
            }
        }
        if !pin_stacks.is_empty() {
            flow.set(
                "commit",
                RowState::Ok,
                Some(flow.commits.join(", ")),
                self.now(),
            );
        }
        flow.set("deploy", RowState::Run, None, self.now());
        self.show_flow(view, &flow);
        for i in items.iter().filter(|i| i.kind == ItemKind::Pull) {
            let r = self
                .run_sub(
                    ActionRequest {
                        stack: i.stack.clone(),
                        action: ActionKind::Update,
                        args: ActionArgs {
                            app: i.app.clone(),
                            ..Default::default()
                        },
                    },
                    view,
                    &mut tracker,
                )
                .await;
            let why = match r {
                Ok(r) if r.ok => None,
                Ok(r) => Some(format!(
                    "The update of {} ended: {}. The host's update rolls an unhealthy app back by itself.",
                    i.stack, r.message
                )),
                Err(Stop::Refused(r)) => {
                    Some(format!("The update of {} was refused: {}.", i.stack, r.why))
                }
                Err(Stop::Link(e)) => Some(format!("The update of {}: {e}.", i.stack)),
            };
            if let Some(why) = why {
                flow.fail("deploy", why.clone(), self.now());
                self.show_flow(view, &flow);
                self.keep_flow_record(view, &flow, "failed");
                return Ok(done(false, why));
            }
        }
        flow.set("deploy", RowState::Ok, None, self.now());

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
            self.keep_flow_record(view, &flow, "done");
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
            self.keep_flow_record(view, &flow, "failed");
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
        self.keep_flow_record(view, &flow, if back_ok { "rolled back" } else { "failed" });
        Ok(done(false, why))
    }

    fn keep_flow_record(&self, view: &JobView, flow: &FlowView, outcome: &str) {
        self.keep_record(UpdateRecord {
            job: view.job,
            at: view.started_at.unwrap_or_else(|| self.now()),
            by: by_words(&view.origin),
            items: flow.items.clone(),
            commits: flow.commits.clone(),
            outcome: outcome.into(),
        });
    }
}
