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

use super::host_link::{HostClient, Shared, now_s, publish_asks};
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
    report(&c, "the start page", Command::Tiles { bare: false }).await
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
    pub prometheus: Option<crate::shell::prometheus::Prometheus>,
    /// replace-goaccess: the access log's Loki job (`admin.traffic_job`).
    pub traffic_job: Option<String>,
}

/// replace-grafana: which charts, over how long.
#[derive(serde::Deserialize)]
pub struct ChartQuery {
    /// A stack's name, or absent for the hypervisor's charts.
    #[serde(default)]
    stack: Option<String>,
    /// `1h`, `6h`, `24h`, `7d` or `30d`; default 24h.
    #[serde(default)]
    range: Option<String>,
}

/// Seconds a range word spans, and the step that gives about 240 points.
pub fn chart_window(range: Option<&str>) -> Option<(u64, u64)> {
    let secs = match range.unwrap_or("24h") {
        "1h" => 3_600,
        "6h" => 6 * 3_600,
        "24h" => 86_400,
        "7d" => 7 * 86_400,
        "30d" => 30 * 86_400,
        _ => return None,
    };
    Some((secs, (secs / 240).max(15)))
}

/// replace-grafana (Kenny, 2026-09-30): the charts of one stack, or of the
/// hypervisor, each panel with its series over the window.
async fn charts(State(c): State<ReadCtx>, Query(q): Query<ChartQuery>) -> Response {
    let Some(prom) = &c.prometheus else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the charts",
            "no Prometheus is configured for this dashboard",
            "set admin.prometheus_url (or HOMELAB_ADMIN_PROMETHEUS_URL) to Prometheus's address",
        );
    };
    let Some((span, step)) = chart_window(q.range.as_deref()) else {
        return refused(
            StatusCode::BAD_REQUEST,
            "the charts",
            "unknown range",
            "use one of 1h, 6h, 24h, 7d, 30d",
        );
    };
    let panels = match (&q.stack, &prom.host_label) {
        (Some(s), _) => homelab_core::charts::stack_panels(s),
        (None, Some(h)) => homelab_core::charts::host_panels(h),
        (None, None) => {
            return refused(
                StatusCode::SERVICE_UNAVAILABLE,
                "the host charts",
                "no charts_host is configured",
                "set admin.charts_host (or HOMELAB_ADMIN_CHARTS_HOST) to the hypervisor's host label in Prometheus",
            );
        }
    };
    let end = now_s();
    let start = end.saturating_sub(span);
    // fix-220: the vmid→stack pairs a network device or cgroup id's humanized
    // label is resolved against — the same fleet view the rest of this
    // dashboard reads, never a lookup of its own.
    let stacks_vn: Vec<homelab_core::charts::humanize::Stack> = c
        .shared
        .read()
        .await
        .fleet
        .as_ref()
        .map(|f| f.stacks.iter().map(|s| (s.vmid, s.name.clone())).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for p in panels {
        let r = prom
            .range(&p.query, start, end, step, p.legend.as_deref())
            .await;
        out.push(match r {
            Ok(series) => {
                let series =
                    homelab_core::charts::humanize::series(series, p.legend.as_deref(), &stacks_vn);
                serde_json::json!({ "panel": p, "series": series })
            }
            Err(e) => serde_json::json!({ "panel": p, "series": [], "error": e }),
        });
    }
    Json(serde_json::json!({
        "panels": out,
        "from": start,
        "to": end,
        "step": step,
    }))
    .into_response()
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
            );
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

/// replace-goaccess (Kenny, 2026-09-30): who visits the services from
/// outside, from the proxy's access log in Loki: requests per hostname and
/// per status over the window, and the busiest hostnames and client
/// addresses in it. The log's fields are the proxy's own JSON names.
async fn traffic(State(c): State<ReadCtx>, Query(q): Query<ChartQuery>) -> Response {
    let (Some(loki), Some(job)) = (&c.loki, &c.traffic_job) else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the traffic",
            "no Loki or no traffic_job is configured for this dashboard",
            "set admin.loki_url and admin.traffic_job (the access log's Loki job)",
        );
    };
    let Some((span, step)) = chart_window(q.range.as_deref()) else {
        return refused(
            StatusCode::BAD_REQUEST,
            "the traffic",
            "unknown range",
            "use one of 1h, 6h, 24h, 7d, 30d",
        );
    };
    Json(super::traffic::read(loki, job, span, step, now_s()).await).into_response()
}

/// The stack names the fleet currently reports, for a fleet-wide Prometheus
/// query (feat-overview-11, feat-overview-12, feat-firewall-3) — the same
/// source `/data/host` reads.
async fn fleet_stack_names(c: &ReadCtx) -> Vec<String> {
    c.shared
        .read()
        .await
        .fleet
        .as_ref()
        .map(|f| f.stacks.iter().map(|s| s.name.clone()).collect())
        .unwrap_or_default()
}

/// feat-overview-11 (capacity map): CPU, memory and disk side by side for
/// every stack, one Prometheus call per metric across the whole fleet.
async fn capacity(State(c): State<ReadCtx>) -> Response {
    let Some(prom) = &c.prometheus else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the capacity map",
            "no Prometheus is configured for this dashboard",
            "set admin.prometheus_url (or HOMELAB_ADMIN_PROMETHEUS_URL) to Prometheus's address",
        );
    };
    let stacks = fleet_stack_names(&c).await;
    if stacks.is_empty() {
        return Json(serde_json::json!({ "panels": [] })).into_response();
    }
    let mut out = Vec::new();
    for p in homelab_core::charts::fleet_capacity_panels(&stacks) {
        let r = prom.instant(&p.query, p.legend.as_deref()).await;
        out.push(match r {
            Ok(series) => serde_json::json!({ "panel": p, "series": series }),
            Err(e) => serde_json::json!({ "panel": p, "series": [], "error": e }),
        });
    }
    Json(serde_json::json!({ "panels": out, "measured_at": now_s() })).into_response()
}

/// feat-overview-12 (disk-growth prediction): every stack's root disk, and
/// every filesystem of the hypervisor, fitted over `range` (default 7d) and
/// warned about within `within_days` (default
/// `homelab_core::diskgrowth::DEFAULT_WARN_DAYS`).
#[derive(Deserialize)]
struct GrowthQuery {
    #[serde(default)]
    range: Option<String>,
    #[serde(default)]
    within_days: Option<f64>,
}

async fn disk_growth(State(c): State<ReadCtx>, Query(q): Query<GrowthQuery>) -> Response {
    let Some(prom) = &c.prometheus else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "disk growth",
            "no Prometheus is configured for this dashboard",
            "set admin.prometheus_url (or HOMELAB_ADMIN_PROMETHEUS_URL) to Prometheus's address",
        );
    };
    let Some((span, step)) = chart_window(q.range.as_deref().or(Some("7d"))) else {
        return refused(
            StatusCode::BAD_REQUEST,
            "disk growth",
            "unknown range",
            "use one of 1h, 6h, 24h, 7d, 30d",
        );
    };
    let within_days = q
        .within_days
        .unwrap_or(homelab_core::diskgrowth::DEFAULT_WARN_DAYS);
    let end = now_s();
    let start = end.saturating_sub(span);
    let stacks = fleet_stack_names(&c).await;
    let mut rows = Vec::new();
    let mut queries: Vec<(String, homelab_core::charts::Panel)> = Vec::new();
    if !stacks.is_empty() {
        queries.push((
            "stacks".into(),
            homelab_core::charts::fleet_disk_growth_query(&stacks),
        ));
    }
    if let Some(h) = &prom.host_label {
        queries.push((
            "host".into(),
            homelab_core::charts::host_disk_growth_query(h),
        ));
    }
    for (scope, p) in queries {
        let series = prom
            .range(&p.query, start, end, step, p.legend.as_deref())
            .await;
        let Ok(series) = series else { continue };
        for s in series {
            let label = s["label"].as_str().unwrap_or("").to_string();
            let points: Vec<(f64, f64)> = s["points"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|p| Some((p[0].as_f64()?, p[1].as_f64()?)))
                        .collect()
                })
                .unwrap_or_default();
            let Some(fit) = homelab_core::diskgrowth::fit(&points) else {
                continue;
            };
            let warning = homelab_core::diskgrowth::is_warning(&fit, within_days);
            rows.push(serde_json::json!({
                "scope": scope,
                "subject": label,
                "fit": fit,
                "warning": warning,
            }));
        }
    }
    Json(serde_json::json!({
        "rows": rows,
        "within_days": within_days,
        "from": start,
        "to": end,
        "measured_at": end,
    }))
    .into_response()
}

/// feat-firewall-3 (measured traffic on the topology): total network
/// throughput per stack — see `homelab_core::charts::fleet_traffic_panels`
/// for why this is per-node, not per-edge.
async fn fleet_traffic(State(c): State<ReadCtx>) -> Response {
    let Some(prom) = &c.prometheus else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the measured traffic",
            "no Prometheus is configured for this dashboard",
            "set admin.prometheus_url (or HOMELAB_ADMIN_PROMETHEUS_URL) to Prometheus's address",
        );
    };
    let stacks = fleet_stack_names(&c).await;
    if stacks.is_empty() {
        return Json(serde_json::json!({ "panels": [] })).into_response();
    }
    let mut out = Vec::new();
    for p in homelab_core::charts::fleet_traffic_panels(&stacks) {
        let r = prom.instant(&p.query, p.legend.as_deref()).await;
        out.push(match r {
            Ok(series) => serde_json::json!({ "panel": p, "series": series }),
            Err(e) => serde_json::json!({ "panel": p, "series": [], "error": e }),
        });
    }
    Json(serde_json::json!({ "panels": out, "measured_at": now_s() })).into_response()
}

pub fn read_router(ctx: ReadCtx) -> Router {
    Router::new()
        .route("/data/host", get(host))
        .route("/data/host/guests", get(guests))
        .route("/data/asks", get(asks))
        .route("/data/asks/answer", post(answer))
        .route("/data/logs", get(logs))
        .route("/data/charts", get(charts))
        .route("/data/traffic", get(traffic))
        .route("/data/capacity", get(capacity))
        .route("/data/disk-growth", get(disk_growth))
        .route("/data/fleet-traffic", get(fleet_traffic))
        .with_state(ctx)
}
