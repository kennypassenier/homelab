//! The dashboard's own JSON routes, mounted with `dashboard_routes`, so the
//! chassis login (and its Origin/Sec-Fetch-Site guard) sits in front of
//! every one of them, behind the two locks of the request guard.
//!
//! The report routes pass the host's JSON answer (feat-platform-1) through
//! unchanged; a failure is `{what, why, fix}` (arch-errors) with 502.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use homelab_proto::Command;
use serde::Deserialize;

use super::host_link::{HostClient, Shared};

#[derive(Clone)]
struct Ctx {
    shared: Shared,
    host: HostClient,
}

/// The page's first paint: the newest snapshot, then SSE carries the rest.
async fn fleet(State(c): State<Ctx>) -> Json<serde_json::Value> {
    let s = c.shared.read().await;
    Json(serde_json::json!({
        "fleet": s.fleet,
        "host_version": s.host_version,
        "link_error": s.link_error,
    }))
}

fn failed(what: &str, why: String) -> Response {
    (
        StatusCode::BAD_GATEWAY,
        Json(serde_json::json!({
            "what": what,
            "why": why,
            "fix": "the dashboard asks the host over its one line; check that the host answers (homelab ping) and look at the dashboard's log",
        })),
    )
        .into_response()
}

/// Ask the host and hand its JSON answer on.
async fn report(c: &Ctx, what: &str, command: Command) -> Response {
    match c.host.ask(command).await {
        Ok(r) => match serde_json::from_str::<serde_json::Value>(&r.message) {
            Ok(v) => Json(serde_json::json!({ "ok": r.ok, "report": v })).into_response(),
            Err(_) => failed(
                what,
                format!(
                    "the host answered text, not JSON: {}",
                    r.message.chars().take(200).collect::<String>()
                ),
            ),
        },
        Err(e) => failed(what, e),
    }
}

async fn doctor(State(c): State<Ctx>) -> Response {
    report(&c, "doctor", Command::Doctor { json: true }).await
}

async fn incidents(State(c): State<Ctx>) -> Response {
    report(&c, "incidents", Command::Incidents { json: true }).await
}

async fn manual_checks(State(c): State<Ctx>) -> Response {
    report(
        &c,
        "manual checks",
        Command::ListManualChecks { json: true },
    )
    .await
}

async fn current_op(State(c): State<Ctx>) -> Response {
    report(&c, "current operation", Command::CurrentOp).await
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default)]
    since: u64,
    #[serde(default = "history_limit")]
    limit: usize,
}
fn history_limit() -> usize {
    500
}

async fn history(State(c): State<Ctx>, Query(q): Query<HistoryQuery>) -> Response {
    report(
        &c,
        "history",
        Command::History {
            since: q.since,
            limit: q.limit.min(5000),
        },
    )
    .await
}

pub fn router(shared: Shared, host: HostClient) -> Router {
    Router::new()
        .route("/data/fleet", get(fleet))
        .route("/data/doctor", get(doctor))
        .route("/data/incidents", get(incidents))
        .route("/data/manual-checks", get(manual_checks))
        .route("/data/current-op", get(current_op))
        .route("/data/history", get(history))
        .with_state(Ctx { shared, host })
}
