//! redesign-flows-6 (3.71.0, the senior review of the Update flow, items 3
//! and 4; FLOWS.md §3.1, demo `flows/update.html`): the Update flow as ONE
//! job on the dashboard's server — back up every touched stack, commit the
//! new image lines (fix-231's `StackEdit::Settings {images}`), deploy that
//! commit (or pull a moving tag), verify every app runs healthy at its new
//! version within 2 min, and put a pinned app's earlier version back by
//! itself when it does not. The page only follows the job, so a closed tab
//! stops nothing, and the job is in Activity like every other.
//!
//! This module is the pure half: the moves the page sends, the rows the job
//! shows, and the verdict over what the host reports after the deploy.

use std::collections::BTreeMap;

use homelab_core::ops::pins::{StackRuntime, pinned_version, runs_image};
use serde::{Deserialize, Serialize};

use super::actions::valid_stack_name;

/// How long the verify waits for an app to become healthy before the
/// safety net rolls it back (the demo's "if the app is not healthy within
/// 2 min").
pub const HEALTHY_WITHIN_S: u64 = 120;

/// How long the undo stays offered: "Roll back, 1 click, for 7 days, from
/// the stack's History" (the demo's Undo later tile).
pub const UNDO_DAYS: u64 = 7;

/// One app the flow moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateItem {
    pub stack: String,
    /// `pin`: a digest-pinned image whose line moves (`key`, `from`, `to`);
    /// `pull`: an app on a moving tag, pulled again (`app`).
    pub kind: ItemKind,
    /// pin: `<app>/<service>`, the settings edit's own key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// pull: the app to pull; pin: the container's name, for the words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    /// pin: the image line now and after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Pin,
    Pull,
}

impl UpdateItem {
    /// The words for one app: `beta-demo/api v2.3.0 → v3.0.0`.
    pub fn words(&self) -> String {
        match self.kind {
            ItemKind::Pin => format!(
                "{}/{} {} → {}",
                self.stack,
                self.key.as_deref().unwrap_or("?"),
                self.from
                    .as_deref()
                    .and_then(pinned_version)
                    .unwrap_or_default(),
                self.to
                    .as_deref()
                    .and_then(pinned_version)
                    .unwrap_or_default(),
            ),
            ItemKind::Pull => format!(
                "{}/{} at its tag's newest image",
                self.stack,
                self.app.as_deref().unwrap_or("every app")
            ),
        }
    }
}

/// The moves as the page sends them, checked: valid stack names, a pin with
/// its key and both image lines that name a version, a pull with its app.
pub fn parse_updates(s: &str) -> Result<Vec<UpdateItem>, String> {
    let items: Vec<UpdateItem> =
        serde_json::from_str(s).map_err(|e| format!("updates did not read: {e}"))?;
    if items.is_empty() {
        return Err("updates names no app".into());
    }
    if items.len() > 64 {
        return Err("updates names more than 64 apps".into());
    }
    for i in &items {
        if !valid_stack_name(&i.stack) {
            return Err(format!("{:?} is not a stack name", i.stack));
        }
        match i.kind {
            ItemKind::Pin => {
                let ok_line = |l: &Option<String>| {
                    l.as_deref().is_some_and(|l| {
                        !l.trim().is_empty() && l.len() <= 512 && !l.contains('\n')
                    })
                };
                if i.key.as_deref().is_none_or(|k| !k.contains('/'))
                    || !ok_line(&i.from)
                    || !ok_line(&i.to)
                {
                    return Err(format!(
                        "a pin of {} needs its <app>/<service> key and both image lines",
                        i.stack
                    ));
                }
                if i.to.as_deref().and_then(pinned_version).is_none() {
                    return Err(format!("the new image of {} names no version", i.stack));
                }
            }
            ItemKind::Pull => {
                if i.app.as_deref().is_some_and(|a| !valid_stack_name(a)) {
                    return Err(format!("{:?} is not an app name", i.app));
                }
            }
        }
    }
    Ok(items)
}

/// The stacks the moves touch, in the order first named.
pub fn stacks_of(items: &[UpdateItem]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for i in items {
        if !out.contains(&i.stack) {
            out.push(i.stack.clone());
        }
    }
    out
}

/// The image lines one stack's pins move to (`key` → line), for the commit.
pub fn images_to(items: &[UpdateItem], stack: &str) -> BTreeMap<String, String> {
    items
        .iter()
        .filter(|i| i.stack == stack && i.kind == ItemKind::Pin)
        .filter_map(|i| Some((i.key.clone()?, i.to.clone()?)))
        .collect()
}

/// The image lines one stack's pins move back to, for the safety net.
pub fn images_back(items: &[UpdateItem], stack: &str) -> BTreeMap<String, String> {
    items
        .iter()
        .filter(|i| i.stack == stack && i.kind == ItemKind::Pin)
        .filter_map(|i| Some((i.key.clone()?, i.from.clone()?)))
        .collect()
}

/// One row's state, as the page paints it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowState {
    Wait,
    Run,
    Ok,
    Bad,
    Skip,
}

/// One row of the running job (flows/update.html steps 3-5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRow {
    pub id: String,
    pub step: u8,
    pub title: String,
    pub desc: String,
    pub state: RowState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took_s: Option<i64>,
}

/// What the job shows: its rows, the step it is on, and what it did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowView {
    pub items: Vec<UpdateItem>,
    pub rows: Vec<FlowRow>,
    /// 3 Back up · 4 Update · 5 Verify · 6 Done.
    pub step: u8,
    /// Short commits of the new image lines (and of a roll back).
    pub commits: Vec<String>,
    /// When the deploys began (the logs comparison starts here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_at: Option<i64>,
    /// Why the job stopped, in the words step 6 shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    /// The safety net put the earlier version back.
    #[serde(default)]
    pub rolled_back: bool,
    /// The verify's own words per app (the Done summary).
    #[serde(default)]
    pub verdict: Vec<String>,
}

impl FlowView {
    /// The rows of these moves, all waiting.
    pub fn new(items: Vec<UpdateItem>) -> FlowView {
        let stacks = stacks_of(&items);
        let pins = items.iter().any(|i| i.kind == ItemKind::Pin);
        let row = |id: &str, step: u8, title: String, desc: &str| FlowRow {
            id: id.into(),
            step,
            title,
            desc: desc.into(),
            state: RowState::Wait,
            note: None,
            started_at: None,
            took_s: None,
        };
        let mut rows = vec![row(
            "backup",
            3,
            format!("Back up {}", stacks.join(", ")),
            "a restic snapshot of each app's data; if it fails, nothing else happens",
        )];
        if pins {
            rows.push(row(
                "commit",
                4,
                "Change the files and commit".into(),
                "the image line moves to the new version, pushed to the repository",
            ));
        }
        rows.push(row(
            "deploy",
            4,
            "Deploy: pull the image and restart".into(),
            "only the chosen apps restart; nothing else",
        ));
        rows.push(row(
            "health",
            5,
            "Verify: the app is healthy".into(),
            "container running, healthcheck answers, version reported",
        ));
        FlowView {
            items,
            rows,
            step: 3,
            ..Default::default()
        }
    }

    /// Set one row's state (and its note); a row that starts is timed.
    pub fn set(&mut self, id: &str, state: RowState, note: Option<String>, now: i64) {
        if let Some(r) = self.rows.iter_mut().find(|r| r.id == id) {
            if state == RowState::Run {
                r.started_at = Some(now);
            }
            if matches!(state, RowState::Ok | RowState::Bad) {
                r.took_s = r.started_at.map(|t| (now - t).max(0));
            }
            r.state = state;
            if note.is_some() {
                r.note = note;
            }
            self.step = self.step.max(r.step);
        }
    }

    /// Add the safety net's row (only when it runs).
    pub fn add_rollback(&mut self) {
        if self.rows.iter().any(|r| r.id == "rollback") {
            return;
        }
        self.rows.push(FlowRow {
            id: "rollback".into(),
            step: 5,
            title: "Safety net: put the earlier version back".into(),
            desc:
                "the app was not healthy within 2 min: the old image line, committed and deployed"
                    .into(),
            state: RowState::Wait,
            note: None,
            started_at: None,
            took_s: None,
        });
    }

    /// Stop at a failed row: the rest is skipped, step 6 says why.
    pub fn fail(&mut self, id: &str, why: String, now: i64) {
        self.set(id, RowState::Bad, Some(why.clone()), now);
        for r in &mut self.rows {
            if r.state == RowState::Wait || r.state == RowState::Run {
                r.state = RowState::Skip;
            }
        }
        self.failed = Some(why);
        self.step = 6;
    }
}

/// The verify's verdict over what the host reported after the deploy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Every touched stack runs all its containers, and every pinned app
    /// runs its new version (or the host does not report versions).
    pub healthy: bool,
    /// Per stack and app, in words.
    pub words: Vec<String>,
}

/// Judge the moves against each stack's runtime as the host reported it
/// (`Err`: it could not be read, or the host is too old to report it).
pub fn judge(
    items: &[UpdateItem],
    runtime: &BTreeMap<String, Result<StackRuntime, String>>,
) -> Verdict {
    let mut healthy = true;
    let mut words = Vec::new();
    for stack in stacks_of(items) {
        match runtime.get(&stack) {
            Some(Ok(rt)) => {
                let total = rt.containers.len();
                let up = rt.containers.iter().filter(|(_, r)| *r).count();
                if total == 0 || up < total {
                    healthy = false;
                    let down: Vec<&str> = rt
                        .containers
                        .iter()
                        .filter(|(_, r)| !*r)
                        .map(|(n, _)| n.as_str())
                        .collect();
                    words.push(format!(
                        "{stack}: {up} of {total} containers running{}",
                        if down.is_empty() {
                            String::new()
                        } else {
                            format!(" ({} not)", down.join(", "))
                        }
                    ));
                } else {
                    words.push(format!("{stack}: {up} of {total} containers running"));
                }
                for i in items
                    .iter()
                    .filter(|i| i.stack == stack && i.kind == ItemKind::Pin)
                {
                    let to = i.to.as_deref().unwrap_or("");
                    let v = pinned_version(to).unwrap_or_default();
                    let key = i.key.as_deref().unwrap_or("?");
                    match runs_image(&rt.images, to) {
                        Some(true) => words.push(format!("{stack}/{key} reports version {v}")),
                        Some(false) => {
                            healthy = false;
                            let now: Vec<String> = rt
                                .images
                                .values()
                                .filter_map(|r| pinned_version(&r.image))
                                .collect();
                            words.push(format!(
                                "{stack}/{key} does not run {v} (runs {})",
                                if now.is_empty() {
                                    "an unnamed version".into()
                                } else {
                                    now.join(", ")
                                }
                            ));
                        }
                        None => {
                            healthy = false;
                            words.push(format!("{stack}/{key}: no container runs that image"));
                        }
                    }
                }
            }
            Some(Err(why)) if why.contains("unknown variant") || why.contains("stack_runtime") => {
                // A host before 3.71.0: the deploy's own health check is all
                // there is; said, never invented.
                words.push(format!(
                    "{stack}: the deploy's health check passed; this host version does not report the running version"
                ));
            }
            Some(Err(why)) => {
                healthy = false;
                words.push(format!("{stack}: could not read its containers ({why})"));
            }
            None => {
                healthy = false;
                words.push(format!("{stack}: not read"));
            }
        }
    }
    Verdict { healthy, words }
}

/// One update the stack's History offers to undo (`GET
/// /data/update-flows?stack=`): kept for [`UNDO_DAYS`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRecord {
    pub job: u64,
    pub at: i64,
    /// Who started it ("Kenny", "Claude (Live view)", "a schedule").
    pub by: String,
    pub items: Vec<UpdateItem>,
    pub commits: Vec<String>,
    /// done, failed, rolled back.
    pub outcome: String,
}

/// The records still offered: newer than [`UNDO_DAYS`], newest first,
/// optionally one stack's.
pub fn undoable(records: &[UpdateRecord], stack: Option<&str>, now: i64) -> Vec<UpdateRecord> {
    let since = now - (UNDO_DAYS as i64) * 86_400;
    let mut out: Vec<UpdateRecord> = records
        .iter()
        .filter(|r| r.at >= since && r.outcome == "done")
        .filter(|r| stack.is_none_or(|s| r.items.iter().any(|i| i.stack == s)))
        .cloned()
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.at));
    out
}

/// What the store keeps: never more than 200 records, none older than 30
/// days (nothing grows without a cap).
pub fn prune(records: &mut Vec<UpdateRecord>, now: i64) {
    let since = now - 30 * 86_400;
    records.retain(|r| r.at >= since);
    records.sort_by_key(|r| std::cmp::Reverse(r.at));
    records.truncate(200);
}
