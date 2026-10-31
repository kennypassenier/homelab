//! The routes under `/data/actions`, and the router that mounts them.

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use super::preview_route::preview;
use super::routes_jobs::{cancel_batch, cancel_job, get_batch, job_log, jobs, stop_batch};
use super::{Actions, JobView, Origin};
use crate::core::actions::{self, ActionArgs, BatchRequest, Refusal};
use crate::core::actions_progress::Outcome;

pub(super) fn refusal(status: StatusCode, r: Refusal) -> Response {
    (status, Json(r)).into_response()
}

fn body<T>(b: Result<Json<T>, JsonRejection>, what: &str) -> Result<T, Refusal> {
    b.map(|Json(t)| t).map_err(|e| {
        Refusal::new(
            what,
            format!("the request body does not read: {}", e.body_text()),
            "send the JSON the catalog describes",
        )
    })
}

async fn catalog() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "actions": actions::catalog(),
        "host_target": actions::HOST_TARGET,
        "self_stack": actions::SELF_STACK,
    }))
}

/// An empty body is no arguments.
pub(super) fn parse_args(
    b: Result<Json<ActionArgs>, JsonRejection>,
    action: &str,
    stack: &str,
) -> Result<ActionArgs, Refusal> {
    match b {
        Err(JsonRejection::MissingJsonContentType(_)) => Ok(ActionArgs::default()),
        other => body(other, &format!("{action} {stack}")),
    }
}

fn accepted(view: &JobView) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "job": view.job,
            "stack": view.stack,
            "action": view.action,
            "state": view.state,
            "restarts_dashboard": view.restarts_dashboard,
        })),
    )
        .into_response()
}

async fn start(
    State(a): State<Actions>,
    UrlPath((stack, action)): UrlPath<(String, String)>,
    b: Result<Json<ActionArgs>, JsonRejection>,
) -> Response {
    let args = match parse_args(b, &action, &stack) {
        Ok(args) => args,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    let req = match actions::validate(&stack, &action, args) {
        Ok(r) => r,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    match a.press(req, Origin::Manual).await {
        Ok(view) => accepted(&view),
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

async fn batch(State(a): State<Actions>, b: Result<Json<BatchRequest>, JsonRejection>) -> Response {
    let b = match body(b, "batch") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    match a.run_batch(b) {
        Ok(v) => (StatusCode::ACCEPTED, Json(v)).into_response(),
        Err((status, r)) => refusal(status, r),
    }
}

async fn rollback_options(State(a): State<Actions>, UrlPath(stack): UrlPath<String>) -> Response {
    match a.rollback_options(stack).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => refusal(StatusCode::BAD_REQUEST, r),
    }
}

#[derive(Deserialize)]
struct ReleasesQuery {
    unit: Option<String>,
}

/// dashboard-latest: the "release tag" dropdown's options.
async fn releases(
    State(a): State<Actions>,
    UrlPath(stack): UrlPath<String>,
    Query(q): Query<ReleasesQuery>,
) -> Response {
    match a.release_options(&stack, q.unit.as_deref()).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => refusal(StatusCode::BAD_REQUEST, r),
    }
}

#[derive(Deserialize)]
struct NotesQuery {
    repo: String,
    tag: String,
}

/// ask-kp11be-6: `/data/release-notes?repo=owner/name&tag=v1.2.3`.
async fn release_notes(State(a): State<Actions>, Query(q): Query<NotesQuery>) -> Response {
    match a.release_notes(&q.repo, &q.tag).await {
        Ok(n) => Json(n).into_response(),
        Err(r) => refusal(StatusCode::BAD_GATEWAY, r),
    }
}

#[derive(Deserialize)]
struct OutcomeQuery {
    op: String,
    since: i64,
}

/// arch-self: a job that restarted the dashboard, read back from the
/// host's history by the tab that started it (design in the module doc of
/// `core::actions_progress`).
async fn outcome(State(a): State<Actions>, Query(q): Query<OutcomeQuery>) -> Json<Outcome> {
    Json(a.outcome_since(&q.op, q.since).await)
}

#[derive(Deserialize)]
struct FlowsQuery {
    #[serde(default)]
    stack: Option<String>,
}

/// redesign-flows-6: the updates a stack's History offers to undo (7 days).
async fn update_flows(
    State(a): State<Actions>,
    Query(q): Query<FlowsQuery>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "updates": a.update_records(q.stack.as_deref()).await,
        "days": crate::core::updateflow::UNDO_DAYS,
        "now": a.now(),
    }))
}

/// Mounted with `dashboard_routes`: the login and both locks stand before
/// every one of them.
pub fn router(actions: Actions) -> Router {
    Router::new()
        .route("/data/actions/catalog", get(catalog))
        .route("/data/actions/jobs", get(jobs))
        .route("/data/actions/batch", post(batch))
        .route("/data/actions/batch/{batch}/stop", post(stop_batch))
        .route("/data/actions/batch/{batch}", get(get_batch))
        .route("/data/actions/batch/{batch}/cancel", post(cancel_batch))
        .route("/data/actions/jobs/{job}/cancel", post(cancel_job))
        .route("/data/actions/jobs/{job}/log", get(job_log))
        .route(
            "/data/actions/{stack}/rollback-options",
            get(rollback_options),
        )
        .route("/data/actions/{stack}/releases", get(releases))
        .route("/data/actions/{stack}/{action}", post(start))
        .route("/data/actions/{stack}/{action}/preview", post(preview))
        .route("/data/actions/outcome", get(outcome))
        .route("/data/update-flows", get(update_flows))
        .route("/data/release-notes", get(release_notes))
        .with_state(actions)
}
