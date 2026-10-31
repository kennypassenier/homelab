//! feat-preset-1 (D): editing `presets/` itself, planned and committed.

use std::path::Path;

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;

use super::EditCtx;
use super::follow::subject_with_id;
use super::http::{answer, blocking, body, refusal};
use super::repo::publish_repo;
use crate::core::actions::Refusal;
use crate::core::editplan;
use crate::core::presetedit;
use crate::core::stackedit::FileChange;
use crate::shell::workcopy::{Committed, WorkingCopy};

/// The feature id a preset edit's commit subject carries.
const FEATURE: &str = "feat-preset-1";

struct PlannedPreset {
    changes: Vec<FileChange>,
    problems: Vec<String>,
    head: Option<String>,
    sync_error: Option<String>,
}

fn prepare_preset(
    wc: &WorkingCopy,
    edit: &presetedit::PresetEdit,
) -> Result<PlannedPreset, Refusal> {
    let sync_error = wc.sync().err().map(|r| r.why);
    let texts = wc.preset_texts();
    let changes = presetedit::changes(&texts, edit)?;
    let problems = wc.with_staged_presets(&changes, check_presets_dir)?;
    let head = wc.status().head.map(|h| h.commit);
    Ok(PlannedPreset {
        changes,
        problems,
        head,
        sync_error,
    })
}

/// Every `preset.yml` under the staged `presets/` reads back as a preset —
/// the same net `check_dir` casts over a stack's `service.yml` files.
fn check_presets_dir(dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return problems;
    };
    for e in entries.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let meta_path = e.path().join(presetedit::PRESET_META);
        if !meta_path.exists() {
            continue;
        }
        let ok = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|t| serde_yaml::from_str::<homelab_client::scaffold::PresetMeta>(&t).ok())
            .is_some();
        if !ok {
            problems.push(format!(
                "presets/{name}/preset.yml does not read as a preset"
            ));
        }
    }
    problems
}

/// The default commit subject for a preset edit (`subject_with_id`'s
/// input): `commit_subject` always prefixes `stacks/`, which is wrong here.
fn preset_default_subject(edit: &presetedit::PresetEdit) -> String {
    format!(
        "presets/{}: {}",
        edit.preset_name(),
        presetedit::describe(edit)
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PresetPlanBody {
    edit: presetedit::PresetEdit,
}

pub(super) async fn presets_plan(
    State(c): State<EditCtx>,
    b: Result<Json<PresetPlanBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the plan for a preset") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(plan_preset(&c, b.edit).await)
}

pub async fn plan_preset(
    c: &EditCtx,
    edit: presetedit::PresetEdit,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let wc = c.wc.clone();
    let e2 = edit.clone();
    let planned = blocking(move || prepare_preset(&wc, &e2))
        .await
        .and_then(|r| r)
        .map_err(|r| (StatusCode::CONFLICT, r))?;
    let diffs = editplan::file_diffs(&planned.changes);
    let subject = subject_with_id(None, &preset_default_subject(&edit), FEATURE);
    Ok(serde_json::json!({
        "stack": format!("presets/{}", edit.preset_name()),
        "kind": edit.kind(),
        "head": planned.head,
        "sync_error": planned.sync_error,
        "files": diffs,
        // A preset commit deploys nothing by itself (feat-stacks-3 reads
        // it later, when a stack adds its app) — the plan dialog's `Plan`
        // shape still wants these, empty rather than absent.
        "effects": Vec::<serde_json::Value>::new(),
        "follow_ups": Vec::<String>::new(),
        "applied": serde_json::Value::Null,
        "restarts_dashboard": false,
        "problems": planned.problems,
        "valid": planned.problems.is_empty() && !planned.changes.is_empty(),
        "unchanged": planned.changes.is_empty(),
        "subject": subject,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetCommitBody {
    pub edit: presetedit::PresetEdit,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

pub(super) async fn presets_commit(
    State(c): State<EditCtx>,
    b: Result<Json<PresetCommitBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the commit to a preset") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(commit_preset(&c, b).await)
}

pub async fn commit_preset(
    c: &EditCtx,
    b: PresetCommitBody,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let wc = c.wc.clone();
    let edit = b.edit.clone();
    let (subject_in, note) = (b.subject.clone(), b.note.clone().unwrap_or_default());
    let done = blocking(move || commit_prepared(&wc, &edit, subject_in.as_deref(), &note))
        .await
        .and_then(|r| r);
    publish_repo(c).await;
    match done {
        Ok(committed) => Ok(serde_json::json!({ "committed": committed })),
        Err(r) => Err((StatusCode::CONFLICT, r)),
    }
}

fn commit_prepared(
    wc: &WorkingCopy,
    edit: &presetedit::PresetEdit,
    subject_in: Option<&str>,
    note: &str,
) -> Result<Committed, Refusal> {
    let planned = prepare_preset(wc, edit)?;
    if !planned.problems.is_empty() {
        return Err(Refusal::new(
            "the commit to presets/",
            planned.problems.join("; "),
            "correct the edit; nothing was written",
        ));
    }
    if planned.changes.is_empty() {
        return Err(Refusal::new(
            "the commit to presets/",
            "nothing changes",
            "change something first",
        ));
    }
    let diffs = editplan::file_diffs(&planned.changes);
    let default = preset_default_subject(edit);
    let subject = subject_with_id(subject_in, &default, FEATURE);
    let message = editplan::commit_message(&subject, note, &[], &diffs);
    wc.transact_presets(&planned.changes, &message, check_presets_dir)
}
