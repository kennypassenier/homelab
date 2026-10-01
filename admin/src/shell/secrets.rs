//! feat-secrets-1/2: the Secrets page. Listing what a stack declares reads
//! the working copy the same way the stack editor's latch form does
//! (`core::stackedit_latch::current`); revealing a value and staging a new
//! one go to the host / the in-memory stage. The write itself is
//! `ActionKind::ChangeSecret` (`shell::actions`'s existing catalog/job/
//! progress machinery, driven by `POST /data/actions/{stack}/change-secret`
//! with `secret_ref` + the `stage_token` this module's `stage` route hands
//! back) — nothing here writes anything.

use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use homelab_proto::{Command, SecretRef};

use super::edit::EditCtx;
use crate::core::actions::{Refusal, valid_stack_name};

const ASK_TIMEOUT: Duration = Duration::from_secs(30);

fn refusal(status: StatusCode, what: &str, why: impl ToString) -> Response {
    (status, Json(Refusal::new(what, why.to_string(), ""))).into_response()
}

/// feat-secrets-1: which secrets this stack declares — no values, no latch
/// call, read straight from the working copy's own `lxc-compose.yml`, the
/// same source the stack editor's latch form already reads.
async fn list(State(c): State<EditCtx>, UrlPath(stack): UrlPath<String>) -> Response {
    if !valid_stack_name(&stack) {
        return refusal(StatusCode::BAD_REQUEST, "secrets", "not a stack name");
    }
    let wc = c.wc.clone();
    let s2 = stack.clone();
    let texts = tokio::task::spawn_blocking(move || wc.stack_texts(&s2)).await;
    let texts = match texts {
        Ok(Ok(t)) => t,
        Ok(Err(r)) => return refusal(StatusCode::CONFLICT, "secrets", r.why),
        Err(e) => return refusal(StatusCode::INTERNAL_SERVER_ERROR, "secrets", e),
    };
    let current = texts
        .get(crate::core::stackedit::MANIFEST)
        .map(|t| crate::core::stackedit_latch::current(t))
        .unwrap_or_default();
    Json(serde_json::json!({
        "secrets": current.latch_secrets,
        "files": current.latch_files,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct RevealBody {
    secret: SecretRef,
}

/// feat-secrets-1: on explicit click only — the page never fetches this
/// ahead of a press, and re-hides on navigation (the page's own state, not
/// this route's concern). Every call is audited on the host; the value
/// never is, here or there.
async fn reveal(
    State(c): State<EditCtx>,
    UrlPath(stack): UrlPath<String>,
    body: axum::extract::Json<RevealBody>,
) -> Response {
    if !valid_stack_name(&stack) {
        return refusal(StatusCode::BAD_REQUEST, "reveal", "not a stack name");
    }
    match c
        .host
        .ask_traced(
            Command::RevealSecret {
                stack,
                secret: body.0.secret,
            },
            ASK_TIMEOUT,
            None,
        )
        .await
    {
        Ok(r) if r.ok => Json(serde_json::json!({ "value": r.message })).into_response(),
        Ok(r) => refusal(StatusCode::BAD_GATEWAY, "reveal", r.message),
        Err(e) => refusal(StatusCode::BAD_GATEWAY, "reveal", e),
    }
}

#[derive(Deserialize)]
struct StageBody {
    content: String,
}

#[derive(serde::Serialize)]
struct StageAnswer {
    stage_token: String,
}

/// feat-secrets-2: stage a new value, token back. The value is held in
/// memory only (`Actions::stage_secret`), taken exactly once by the
/// `change-secret` job the token is for, and dropped unused after five
/// minutes either way — never written to a log, a job's history or a
/// "copy as CLI" line (see `ActionKind::ChangeSecret`'s doc comment).
async fn stage(State(c): State<EditCtx>, body: axum::extract::Json<StageBody>) -> Response {
    let token = c.actions.stage_secret(body.0.content);
    Json(StageAnswer { stage_token: token }).into_response()
}

pub fn router(ctx: EditCtx) -> Router {
    Router::new()
        .route("/data/secrets/{stack}", get(list))
        .route("/data/secrets/{stack}/reveal", post(reveal))
        .route("/data/secrets/stage", post(stage))
        .with_state(ctx)
}
