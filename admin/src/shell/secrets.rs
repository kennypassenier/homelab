//! feat-secrets-1/2: the Secrets page. Listing what a stack declares reads
//! the working copy the same way the stack editor's latch form does
//! (`core::stackedit_latch`); revealing a value and staging a new one go to
//! the host / the in-memory stage. The write itself is
//! `ActionKind::ChangeSecret` (`shell::actions`'s existing catalog/job/
//! progress machinery, driven by `POST /data/actions/{stack}/change-secret`
//! with `secret_ref` + the `stage_token` this module's `stage` route hands
//! back) — nothing here writes anything.
//!
//! redesign-3.71 secrets (Kenny, 2026-10-03): one read for every stack's
//! counts (`GET /data/secrets`), and every reveal and copy names who asked
//! and why, so the host's history (what Activity reads) says "Kenny
//! revealed gateway/traefik/.env" — never the value.

use std::time::Duration;

use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use homelab_proto::{Command, RevealAudit, RevealPurpose, SecretRef};

use super::drive::Driver;
use super::edit::EditCtx;
use crate::core::actions::{Refusal, valid_stack_name};
use crate::core::secrets as pure;

const ASK_TIMEOUT: Duration = Duration::from_secs(30);

/// The Secrets routes' state: the editor's context (working copy, host,
/// stage), Live view's driver (to tell Claude's clicks from a person's) and
/// the name a person's reveal is recorded under.
#[derive(Clone)]
pub struct SecretsCtx {
    pub edit: EditCtx,
    pub driver: Option<Driver>,
    pub viewer_name: Option<String>,
}

fn refusal(status: StatusCode, what: &str, why: impl ToString) -> Response {
    (status, Json(Refusal::new(what, why.to_string(), ""))).into_response()
}

/// The manifest's text, `None` when the working copy has no such stack
/// directory or no `lxc-compose.yml` in it.
async fn manifest(c: &SecretsCtx, stack: &str) -> Result<Option<String>, Response> {
    let wc = c.edit.wc.clone();
    let s2 = stack.to_string();
    match tokio::task::spawn_blocking(move || {
        let known = wc.stack_names().contains(&s2);
        known
            .then(|| wc.stack_texts(&s2))
            .transpose()
            .map(|t| t.and_then(|t| t.get(crate::core::stackedit::MANIFEST).cloned()))
    })
    .await
    {
        Ok(Ok(t)) => Ok(t),
        Ok(Err(r)) => Err((StatusCode::CONFLICT, Json(r)).into_response()),
        Err(e) => Err(refusal(StatusCode::INTERNAL_SERVER_ERROR, "secrets", e)),
    }
}

/// feat-secrets-1: which secrets this stack declares — no values, no latch
/// call, read straight from the working copy's own `lxc-compose.yml`.
async fn list(State(c): State<SecretsCtx>, UrlPath(stack): UrlPath<String>) -> Response {
    if !valid_stack_name(&stack) {
        return refusal(StatusCode::BAD_REQUEST, "secrets", "not a stack name");
    }
    match manifest(&c, &stack).await {
        Ok(t) => Json(pure::declared(t.as_deref())).into_response(),
        Err(r) => r,
    }
}

/// redesign-3.71: every stack in the working copy at once, for the left
/// pane's exact counts; a stack whose file does not read says why.
async fn all(State(c): State<SecretsCtx>) -> Response {
    let wc = c.edit.wc.clone();
    // No working copy is not "every stack declares none": say so.
    if !wc.present() {
        return (
            StatusCode::CONFLICT,
            Json(Refusal::new(
                "the stacks' secrets",
                "the dashboard has no working copy of the repository yet",
                "wait for its first fetch, or read the working copy's error on the Settings page",
            )),
        )
            .into_response();
    }
    let read = tokio::task::spawn_blocking(move || {
        wc.stack_names()
            .into_iter()
            .map(|s| {
                let d = match wc.stack_texts(&s) {
                    Ok(t) => pure::declared(t.get(crate::core::stackedit::MANIFEST).map(|x| &**x)),
                    Err(r) => pure::Declared {
                        unreadable: Some(format!("{} :: {}", r.why, r.fix)),
                        ..Default::default()
                    },
                };
                (s, d)
            })
            .collect::<std::collections::BTreeMap<_, _>>()
    })
    .await;
    match read {
        Ok(stacks) => Json(serde_json::json!({ "stacks": stacks })).into_response(),
        Err(e) => refusal(StatusCode::INTERNAL_SERVER_ERROR, "secrets", e),
    }
}

#[derive(Deserialize)]
struct RevealBody {
    secret: SecretRef,
    /// redesign-3.71: "reveal" (shown on screen) or "copy" (to the
    /// clipboard); an older page sends neither and means a reveal.
    #[serde(default)]
    purpose: RevealPurpose,
    /// The page's word that Live view clicked (a click no person made); the
    /// server believes it only while Live view is actually driving.
    #[serde(default)]
    driven: bool,
}

/// The Cloudflare Access login of this request, if it carries one (lock 1
/// verified it before this route runs).
fn access_email(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cf-access-jwt-assertion")
        .and_then(|v| v.to_str().ok())
        .and_then(|t| crate::core::access::parse_jwt(t).ok())
        .and_then(|j| j.claims.email)
}

/// feat-secrets-1: on explicit click only — the page never fetches this
/// ahead of a press, and re-hides on its own (30 s, or leaving the page).
/// Every call is audited on the host, who and why included; the value
/// never is, here or there.
async fn reveal(
    State(c): State<SecretsCtx>,
    UrlPath(stack): UrlPath<String>,
    headers: HeaderMap,
    body: axum::extract::Json<RevealBody>,
) -> Response {
    if !valid_stack_name(&stack) {
        return refusal(StatusCode::BAD_REQUEST, "reveal", "not a stack name");
    }
    let RevealBody {
        secret,
        purpose,
        driven,
    } = body.0;
    let live = c.driver.as_ref().is_some_and(|d| d.snapshot().active);
    let by = pure::actor(
        driven,
        live,
        c.viewer_name.as_deref(),
        access_email(&headers).as_deref(),
    );
    let ask = |audit: Option<RevealAudit>| Command::RevealSecret {
        stack: stack.clone(),
        secret: secret.clone(),
        audit,
    };
    let mut answer = c
        .edit
        .host
        .ask_traced(ask(Some(RevealAudit { purpose, by })), ASK_TIMEOUT, None)
        .await;
    // fix-211 / invariant 14: a host before 3.71.0 does not know `audit`;
    // ask once more in the shape it knows (it still audits in audit.log).
    if let Ok(r) = &answer
        && !r.ok
        && pure::refused_for_audit_field(&r.message)
    {
        answer = c.edit.host.ask_traced(ask(None), ASK_TIMEOUT, None).await;
    }
    match answer {
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
async fn stage(State(c): State<SecretsCtx>, body: axum::extract::Json<StageBody>) -> Response {
    let token = c.edit.actions.stage_secret(body.0.content);
    Json(StageAnswer { stage_token: token }).into_response()
}

pub fn router(ctx: SecretsCtx) -> Router {
    Router::new()
        .route("/data/secrets", get(all))
        .route("/data/secrets/{stack}", get(list))
        .route("/data/secrets/{stack}/reveal", post(reveal))
        .route("/data/secrets/stage", post(stage))
        .with_state(ctx)
}
