//! The dashboard's own JSON routes, mounted with `dashboard_routes`, so the
//! chassis login (and its Origin/Sec-Fetch-Site guard) sits in front of
//! every one of them, behind the two locks of the request guard.
//!
//! The report routes pass the host's JSON answer (feat-platform-1) through
//! unchanged; a failure is `{what, why, fix}` (arch-errors) with 502.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chassis::shell::live::Live;
use homelab_proto::Command;
use serde::Deserialize;

use std::sync::Arc;

use super::host_link::{now_s, publish_asks, HostClient, Shared};
use super::loki::Loki;
use super::slow::{RunQuery, SlowRead, WAIT};
use crate::core::asks::AnswerRequest;
use crate::core::guests::parse_status;
use crate::core::logs::LogQuery;

#[derive(Clone)]
struct Ctx {
    shared: Shared,
    host: HostClient,
    /// The doctor reads for about 30 s, the request guard's own limit: a
    /// slow read (`shell::slow`).
    doctor_read: Arc<SlowRead>,
}

/// The page's first paint: the newest snapshot, then SSE carries the rest.
async fn fleet(State(c): State<Ctx>) -> Json<serde_json::Value> {
    let s = c.shared.read().await;
    Json(serde_json::json!({
        "fleet": s.fleet,
        "host_version": s.host_version,
        "host_build": s.host_build,
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

/// The report answer as a value, for a slow read.
async fn report_value(
    host: &HostClient,
    what: &str,
    command: Command,
) -> (StatusCode, serde_json::Value) {
    let failed = |why: String| {
        (
            StatusCode::BAD_GATEWAY,
            serde_json::json!({
                "what": what,
                "why": why,
                "fix": "the dashboard asks the host over its one line; check that the host answers (homelab ping) and look at the dashboard's log",
            }),
        )
    };
    match host.ask(command).await {
        Ok(r) => match serde_json::from_str::<serde_json::Value>(&r.message) {
            Ok(v) => (
                StatusCode::OK,
                serde_json::json!({ "ok": r.ok, "report": v }),
            ),
            Err(_) => failed(format!(
                "the host answered text, not JSON: {}",
                r.message.chars().take(200).collect::<String>()
            )),
        },
        Err(e) => failed(e),
    }
}

async fn doctor(State(c): State<Ctx>, Query(q): Query<RunQuery>) -> Response {
    let host = c.host.clone();
    c.doctor_read
        .read(q.run, WAIT, move || async move {
            report_value(&host, "doctor", Command::Doctor { json: true }).await
        })
        .await
}

async fn incidents(State(c): State<Ctx>) -> Response {
    report(&c, "incidents", Command::Incidents { json: true }).await
}

async fn tiles(State(c): State<Ctx>) -> Response {
    report(&c, "the start page", Command::Tiles).await
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

pub fn router(
    shared: Shared,
    host: HostClient,
    publish: Arc<dyn super::actions::Publish>,
) -> Router {
    Router::new()
        .route("/data/fleet", get(fleet))
        .route("/data/doctor", get(doctor))
        .route("/data/incidents", get(incidents))
        .route("/data/manual-checks", get(manual_checks))
        .route("/data/tiles", get(tiles))
        .route("/data/current-op", get(current_op))
        .route("/data/history", get(history))
        .with_state(Ctx {
            shared,
            host,
            doctor_read: SlowRead::announced("doctor", "doctor", publish),
        })
}

/// The read milestone's routes that need more than the host line: the live
/// channel (questions) and Loki (logs).
#[derive(Clone)]
pub struct ReadCtx {
    pub shared: Shared,
    pub host: HostClient,
    pub live: Live,
    pub loki: Option<Loki>,
}

fn refused(status: StatusCode, what: &str, why: &str, fix: &str) -> Response {
    (
        status,
        Json(serde_json::json!({ "what": what, "why": why, "fix": fix })),
    )
        .into_response()
}

/// feat-overview-2: the host's own facts from the newest fleet reading.
async fn host(State(c): State<ReadCtx>) -> Json<serde_json::Value> {
    let s = c.shared.read().await;
    Json(serde_json::json!({
        "host": s.fleet.as_ref().map(|f| &f.host),
        "counts": s.fleet.as_ref().map(|f| &f.counts),
        "measured_at": s.fleet.as_ref().map(|f| f.measured_at),
        "host_version": s.host_version,
        "host_build": s.host_build,
        "link_error": s.link_error,
    }))
}

/// feat-overview-2: the containers on the host, from `status` (`pct list`).
async fn guests(State(c): State<ReadCtx>) -> Response {
    match c.host.ask(Command::Status).await {
        Ok(r) if r.ok => Json(serde_json::json!({
            "guests": parse_status(&r.message),
            "measured_at": now_s(),
        }))
        .into_response(),
        Ok(r) => failed(
            "the host's containers",
            format!(
                "the host answered: {}",
                r.message.chars().take(200).collect::<String>()
            ),
        ),
        Err(e) => failed("the host's containers", e),
    }
}

/// feat-ops-2: the questions the host is waiting on now.
async fn asks(State(c): State<ReadCtx>) -> Json<serde_json::Value> {
    let now = now_s();
    let open = c.shared.read().await.asks.open(now);
    Json(serde_json::json!({ "asks": open, "now": now }))
}

/// feat-ops-2: answer one question, or refuse a stale answer. The answer is
/// checked against the questions the dashboard heard (same start of the
/// host, same operation and step, not past the host's wait) before anything
/// is sent; the host checks the start again.
///
/// `send` puts the command on the host line (the route: `HostClient::ask`;
/// a test: a fake host).
pub async fn answer_ask<F, Fut>(
    shared: &Shared,
    live: &Live,
    req: AnswerRequest,
    now: u64,
    send: F,
) -> Response
where
    F: FnOnce(Command) -> Fut,
    Fut: std::future::Future<Output = Result<homelab_proto::RpcResponse, String>>,
{
    const WHAT: &str = "the answer";
    let command = match shared.read().await.asks.check(&req, now) {
        Ok(command) => command,
        Err(refusal) => {
            return refused(
                StatusCode::CONFLICT,
                WHAT,
                refusal.why(),
                "nothing was sent; the page shows the questions that are open now",
            )
        }
    };
    match send(command).await {
        Ok(r) => {
            // Delivered or not, the host no longer waits on this id.
            shared.write().await.asks.forget(&req.boot, req.id);
            publish_asks(shared, live).await;
            if r.ok {
                Json(serde_json::json!({ "ok": true, "message": r.message })).into_response()
            } else {
                refused(
                    StatusCode::CONFLICT,
                    WHAT,
                    &format!("the host did not take it: {}", r.message),
                    "the operation went on without this answer; its outcome is on the Activity page",
                )
            }
        }
        Err(e) => failed(
            WHAT,
            format!("{e}; whether the host received it is unknown"),
        ),
    }
}

async fn answer(State(c): State<ReadCtx>, Json(req): Json<AnswerRequest>) -> Response {
    let host = c.host.clone();
    answer_ask(&c.shared, &c.live, req, now_s(), |command| async move {
        host.ask(command).await
    })
    .await
}

/// feat-ops-4: a stack's container logs, asked of Loki by this server.
async fn logs(State(c): State<ReadCtx>, Query(q): Query<LogQuery>) -> Response {
    let Some(loki) = &c.loki else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the logs",
            "no Loki is configured for this dashboard",
            "set admin.loki_url (or HOMELAB_ADMIN_LOKI_URL) to Loki's address, e.g. http://10.10.10.13:3100",
        );
    };
    let now = now_s();
    match loki.query(&q, now).await {
        Ok(found) => Json(serde_json::json!({
            "lines": found.lines,
            "logql": found.logql,
            "from": found.from,
            "to": found.to,
            "measured_at": now,
        }))
        .into_response(),
        Err(e) => refused(
            StatusCode::BAD_GATEWAY,
            "the logs",
            &e,
            "check that the log store at HOMELAB_ADMIN_LOKI_URL answers queries from this dashboard",
        ),
    }
}

pub fn read_router(ctx: ReadCtx) -> Router {
    Router::new()
        .route("/data/host", get(host))
        .route("/data/host/guests", get(guests))
        .route("/data/asks", get(asks))
        .route("/data/asks/answer", post(answer))
        .route("/data/logs", get(logs))
        .with_state(ctx)
}
