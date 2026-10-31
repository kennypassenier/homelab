//! The working copy's own routes: its status, the sync, the choice for
//! unpushed commits, and the Files editor's text check.

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use homelab_core::ops::deployguard;
use serde::Deserialize;

use super::EditCtx;
use super::http::{blocking, body, refusal};
use crate::core::actions::Refusal;
use crate::core::textcheck;
use crate::shell::workcopy::Unpushed;

/// arch-edit-txn: the stacks the fleet deployed from a commit the remote
/// lacks, as `(stack, commit)` JSON rows.
fn deployed_unpushed(
    fleet: &[(String, Option<String>)],
    status: &crate::shell::workcopy::RepoStatus,
) -> Vec<serde_json::Value> {
    fleet
        .iter()
        .filter_map(|(name, src)| {
            let applied = src.as_deref().and_then(deployguard::applied_commit)?;
            let hit = status
                .unpushed
                .iter()
                .find(|u| u.commit.starts_with(applied))?;
            Some(serde_json::json!({ "stack": name, "commit": hit.commit }))
        })
        .collect()
}

async fn repo_status_json(c: &EditCtx) -> serde_json::Value {
    let wc = c.wc.clone();
    let status = blocking(move || wc.status()).await.unwrap_or_default();
    let fleet: Vec<(String, Option<String>)> = c
        .shared
        .read()
        .await
        .fleet
        .as_ref()
        .map(|f| {
            f.stacks
                .iter()
                .map(|s| (s.name.clone(), s.applied_source.clone()))
                .collect()
        })
        .unwrap_or_default();
    let deployed_unpushed = deployed_unpushed(&fleet, &status);
    serde_json::json!({ "repo": status, "deployed_unpushed": deployed_unpushed })
}

pub(super) async fn publish_repo(c: &EditCtx) {
    let v = repo_status_json(c).await;
    c.publish.publish("repo", v);
}

pub(super) async fn repo_status(State(c): State<EditCtx>) -> Json<serde_json::Value> {
    Json(repo_status_json(&c).await)
}

pub(super) async fn repo_sync(State(c): State<EditCtx>) -> Response {
    let wc = c.wc.clone();
    let r = blocking(move || wc.sync()).await.and_then(|r| r);
    publish_repo(&c).await;
    match r {
        Ok(()) => Json(repo_status_json(&c).await).into_response(),
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UnpushedBody {
    choice: Unpushed,
    /// fix-wipe-1: a drop carries the typed word `drop`, checked here and
    /// not only in the browser.
    #[serde(default)]
    confirm: Option<String>,
}

pub(super) async fn repo_unpushed(
    State(c): State<EditCtx>,
    b: Result<Json<UnpushedBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the unpushed commits") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    if b.choice == Unpushed::Drop && b.confirm.as_deref() != Some("drop") {
        return refusal(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                "drop the unpushed commits",
                "the typed word does not match",
                "type drop exactly to confirm",
            ),
        );
    }
    let wc = c.wc.clone();
    let r = blocking(move || wc.resolve(b.choice)).await.and_then(|r| r);
    publish_repo(&c).await;
    match r {
        Ok(()) => Json(repo_status_json(&c).await).into_response(),
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckTextBody {
    path: String,
    text: String,
}

/// feat-kpadopt-2: `POST /data/check-text` `{ path, text }`: where a YAML or
/// TOML file stops reading (`{ "problem": { line, column, message } }`, or
/// `null`), for the Files editor's code field. Writes nothing; any other
/// kind of file has no problem.
pub(super) async fn check_text(b: Result<Json<CheckTextBody>, JsonRejection>) -> Response {
    let b = match body(b, "the text check") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    let problem = textcheck::kind_of(&b.path).and_then(|k| textcheck::check(k, &b.text));
    Json(serde_json::json!({ "problem": problem })).into_response()
}
