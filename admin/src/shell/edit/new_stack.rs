//! feat-stacks-3: a new stack from a preset, planned and committed with the
//! client's own scaffold; for the routes and the driven wizard.

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;

use super::EditCtx;
use super::check::{check_dir, others};
use super::follow::{follow_up, subject_with_id};
use super::http::{answer, blocking, body, refusal};
use super::planned::{Planned, plan_json};
use super::repo::publish_repo;
use super::taken::taken;
use crate::core::actions::Refusal;
use crate::core::editplan;
use crate::core::newstack::{self, NewStack, Taken};
use crate::core::stackedit::{FileChange, MANIFEST};
use crate::core::stackedit_tiles;
use crate::core::yamledit;
use crate::shell::actions::Origin;
use crate::shell::workcopy::{Committed, WorkingCopy};

/// The feature id of the wizard's plan and commit.
const FEATURE: &str = "feat-stacks-3";

fn scaffold_new(wc: &WorkingCopy, req: &NewStack) -> Result<Vec<FileChange>, Refusal> {
    let presets_base = wc.repo.join("presets");
    let list = homelab_client::scaffold::scan_presets(&presets_base);
    let preset = list.iter().find(|p| p.name == req.preset).cloned();
    let req = req.clone();
    let name = req.name.clone();
    wc.scaffolded(&name, move |base| {
        homelab_client::scaffold::scaffold_stack(
            base,
            &presets_base,
            &homelab_client::scaffold::StackParams {
                name: &req.name,
                vmid: req.vmid,
                ram_mb: req.ram_mb,
                cores: req.cores,
                disk_gb: req.disk_gb,
                swap_mb: req.swap_mb,
                preset: preset.as_ref(),
                no_data_paths: &req.no_data,
            },
        )
        .map(|_| ())
    })
}

/// feat-tiles-3: the wizard's own optional Tile step, applied to the
/// freshly scaffolded manifest in the very same commit — the same
/// `Op::Set` at `tiles.<hostname>` `StackEdit::PublishApp`'s and
/// `StackEdit::AddApp`'s own tile steps use, rather than a second,
/// best-effort `StackEdit::Tiles` commit once the stack already exists.
fn apply_new_stack_tile(
    req: &NewStack,
    mut changes: Vec<FileChange>,
) -> Result<Vec<FileChange>, Refusal> {
    let Some(tile) = &req.tile else {
        return Ok(changes);
    };
    let rel = format!("stacks/{}/{MANIFEST}", req.name);
    let Some(entry) = changes.iter_mut().find(|c| c.path == rel) else {
        return Err(Refusal::new(
            "the new stack's tile",
            format!("{rel} was not scaffolded"),
            "report this with the dashboard's log",
        ));
    };
    let text = entry.new.clone().unwrap_or_default();
    let ops = stackedit_tiles::set_tile_ops(&text, &tile.hostname, &tile.fields());
    let new = yamledit::edit(&text, &ops).map_err(|e| {
        Refusal::new(
            "the new stack's tile",
            format!("{rel}: {e}"),
            "correct the tile step's values",
        )
    })?;
    entry.new = Some(new);
    Ok(changes)
}

/// Refuse the wizard's values that the fleet or the presets cannot take.
fn refuse_problems(wc: &WorkingCopy, req: &NewStack, taken: &Taken) -> Result<(), Refusal> {
    let names: Vec<String> = homelab_client::scaffold::scan_presets(&wc.repo.join("presets"))
        .into_iter()
        .map(|p| p.name)
        .collect();
    let problems = newstack::problems(req, taken, &names);
    if problems.is_empty() {
        return Ok(());
    }
    Err(Refusal::new(
        "the new stack",
        problems
            .iter()
            .map(|(_, w)| w.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        "correct the marked fields in the wizard",
    ))
}

fn new_summary(req: &NewStack) -> String {
    if req.preset == newstack::EMPTY_PRESET {
        format!("new empty stack (CT {})", req.vmid)
    } else {
        format!("new stack from the {} preset (CT {})", req.preset, req.vmid)
    }
}

fn prepare_new(wc: &WorkingCopy, req: &NewStack, taken: &Taken) -> Result<Planned, Refusal> {
    let sync_error = wc.sync().err().map(|r| r.why);
    refuse_problems(wc, req, taken)?;
    let changes = scaffold_new(wc, req)?;
    let changes = apply_new_stack_tile(req, changes)?;
    let others = others(wc, &req.name);
    let name = req.name.clone();
    let checked = wc.with_staged(&req.name, &changes, |dir| check_dir(dir, &name, &others))?;
    Ok(Planned {
        changes,
        checked,
        old: None,
        head: wc.status().head.map(|h| h.commit),
        sync_error,
        summary: new_summary(req),
    })
}

pub(super) async fn new_plan(
    State(c): State<EditCtx>,
    b: Result<Json<NewStack>, JsonRejection>,
) -> Response {
    let req = match body(b, "the new stack") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(plan_new(&c, req).await)
}

/// feat-stacks-3: the new stack's plan, for the route and a driven wizard.
pub async fn plan_new(
    c: &EditCtx,
    req: NewStack,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let t = taken(c).await;
    let wc = c.wc.clone();
    let r2 = req.clone();
    let planned = blocking(move || prepare_new(&wc, &r2, &t))
        .await
        .and_then(|r| r)
        .map_err(|r| (StatusCode::CONFLICT, r))?;
    let (json, _, _) = plan_json(
        &req.name,
        "new",
        FEATURE,
        &planned,
        serde_json::json!({ "never": true, "changes": [] }),
    );
    Ok(json)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewCommitBody {
    stack: NewStack,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    follow: Option<String>,
}

pub(super) async fn new_commit(
    State(c): State<EditCtx>,
    b: Result<Json<NewCommitBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the new stack") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(commit_new(&c, b, Origin::Manual).await)
}

/// feat-stacks-3: the new stack's commit and push, then its first deploy
/// when asked; for the route and a driven wizard's final press.
pub async fn commit_new(
    c: &EditCtx,
    b: NewCommitBody,
    origin: Origin,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let t = taken(c).await;
    let wc = c.wc.clone();
    let req = b.stack.clone();
    let (subject_in, note) = (b.subject.clone(), b.note.clone().unwrap_or_default());
    let done = blocking(move || commit_new_prepared(&wc, &req, &t, subject_in.as_deref(), &note))
        .await
        .and_then(|r| r);
    publish_repo(c).await;
    match done {
        Ok(committed) => {
            let follow = follow_up(
                c,
                &b.stack.name,
                b.follow.as_deref(),
                &committed.commit,
                origin,
            );
            Ok(serde_json::json!({ "committed": committed, "follow": follow }))
        }
        Err(r) => Err((StatusCode::CONFLICT, r)),
    }
}

fn commit_new_prepared(
    wc: &WorkingCopy,
    req: &NewStack,
    t: &Taken,
    subject_in: Option<&str>,
    note: &str,
) -> Result<Committed, Refusal> {
    let planned = prepare_new(wc, req, t)?;
    if !planned.checked.problems.is_empty() {
        return Err(Refusal::new(
            format!("the new stack {}", req.name),
            planned.checked.problems.join("; "),
            "correct the wizard's values; nothing was written",
        ));
    }
    let (_, effects, diffs) =
        plan_json(&req.name, "new", FEATURE, &planned, serde_json::Value::Null);
    let default = editplan::commit_subject(&req.name, &planned.summary, FEATURE);
    let subject = subject_with_id(subject_in, &default, FEATURE);
    let message = editplan::commit_message(&subject, note, &effects, &diffs);
    let others = others(wc, &req.name);
    let name = req.name.clone();
    wc.transact(&req.name, &planned.changes, &message, move |dir| {
        check_dir(dir, &name, &others).problems
    })
}
