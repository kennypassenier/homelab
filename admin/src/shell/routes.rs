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

/// redesign-stacks: the last day of the fleet, one point per five minutes
/// (`core::trend`), for the Stacks page's sparklines.
async fn fleet_trend(State(c): State<Ctx>) -> Json<serde_json::Value> {
    Json(c.shared.read().await.trend.view())
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
    /// ask-kp11be-8: the window's end, unix seconds; absent reads up to
    /// now.
    #[serde(default)]
    until: Option<u64>,
    #[serde(default = "history_limit")]
    limit: usize,
}
fn history_limit() -> usize {
    500
}

/// ask-kp11be-8: drop the entries that started after `until` from a
/// `{ "entries": [...] }` answer. A host that knows the bound has already
/// done it; an older host answers everything since `since`, and the window
/// the page asked for is still the window it gets.
fn cut_history_at(report: &mut serde_json::Value, until: u64) {
    if let Some(list) = report.get_mut("entries").and_then(|e| e.as_array_mut()) {
        list.retain(|e| {
            e.get("start")
                .and_then(|s| s.as_u64())
                .is_none_or(|s| s <= until)
        });
    }
}

async fn history(State(c): State<Ctx>, Query(q): Query<HistoryQuery>) -> Response {
    let (status, mut body) = report_value(
        &c.host,
        "history",
        Command::History {
            since: q.since,
            until: q.until,
            limit: q.limit.min(5000),
        },
    )
    .await;
    if let (Some(until), Some(report)) = (q.until, body.get_mut("report")) {
        cut_history_at(report, until);
    }
    (status, Json(body)).into_response()
}

pub fn router(
    shared: Shared,
    host: HostClient,
    publish: Arc<dyn super::actions::Publish>,
) -> Router {
    Router::new()
        .route("/data/fleet", get(fleet))
        .route("/data/fleet-trend", get(fleet_trend))
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
    /// ask-kp11be-1: a custom window in unix seconds (`from` and `to` come
    /// together). When both are present they win over `range`, so a chart
    /// zoom reads exactly the window that was zoomed to.
    #[serde(default)]
    from: Option<u64>,
    #[serde(default)]
    to: Option<u64>,
}

/// The widest custom window `/data/charts` reads, seconds (the dearest
/// preset, 30 d).
const MAX_CUSTOM_SPAN: u64 = 30 * 86_400;
/// The narrowest, so a zoom never asks Prometheus for a handful of points.
const MIN_CUSTOM_SPAN: u64 = 60;

/// The window a chart read covers: `(start, end, step)` in unix seconds. A
/// custom `from`/`to` pair wins over `range`; otherwise the preset ends
/// `now`. Err is a sentence for a 400.
pub fn chart_bounds(
    range: Option<&str>,
    from: Option<u64>,
    to: Option<u64>,
    now: u64,
) -> Result<(u64, u64, u64), &'static str> {
    match (from, to) {
        (Some(f), Some(t)) => {
            if t <= f {
                return Err("`to` must be after `from`");
            }
            let span = t - f;
            if span < MIN_CUSTOM_SPAN {
                return Err("the window is shorter than a minute");
            }
            if span > MAX_CUSTOM_SPAN {
                return Err("the window is longer than 30 days");
            }
            Ok((f, t, (span / 240).max(15)))
        }
        (None, None) => {
            let (span, step) = chart_window(range).ok_or("unknown range")?;
            Ok((now.saturating_sub(span), now, step))
        }
        _ => Err("`from` and `to` come together"),
    }
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
    let (start, end, step) = match chart_bounds(q.range.as_deref(), q.from, q.to, now_s()) {
        Ok(b) => b,
        Err(why) => {
            return refused(
                StatusCode::BAD_REQUEST,
                "the charts",
                why,
                "use range=1h, 6h, 24h, 7d or 30d, or from and to in unix seconds (60 s to 30 d apart)",
            );
        }
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

/// feat-overview-2: the containers on the host, from `status` (its fleet
/// snapshot; fix-371-1).
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

/// The Stacks cards' trend row: the last 24 hours of every stack's CPU in
/// one request (one Prometheus range query across the fleet), as
/// `{stacks: {name: [[t, v], …]}, from, to, step}`. A stack Prometheus has no
/// reading for is simply absent from `stacks`.
async fn fleet_cpu(State(c): State<ReadCtx>) -> Response {
    let Some(prom) = &c.prometheus else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the stacks' CPU trends",
            "no Prometheus is configured for this dashboard",
            "set admin.prometheus_url (or HOMELAB_ADMIN_PROMETHEUS_URL) to Prometheus's address",
        );
    };
    const DAY: u64 = 86_400;
    const STEP: u64 = 1_800;
    let end = now_s();
    let start = end.saturating_sub(DAY);
    let names = fleet_stack_names(&c).await;
    let mut stacks = serde_json::Map::new();
    if !names.is_empty() {
        let p = homelab_core::charts::fleet_cpu_query(&names);
        match prom
            .range(&p.query, start, end, STEP, p.legend.as_deref())
            .await
        {
            Ok(series) => {
                for s in series {
                    if let Some(label) = s["label"].as_str().filter(|l| !l.is_empty()) {
                        stacks.insert(label.to_string(), s["points"].clone());
                    }
                }
            }
            Err(e) => {
                return refused(
                    StatusCode::BAD_GATEWAY,
                    "the stacks' CPU trends",
                    &e,
                    "check that Prometheus answers queries from this dashboard",
                );
            }
        }
    }
    Json(serde_json::json!({
        "stacks": stacks,
        "from": start,
        "to": end,
        "step": STEP,
        "measured_at": end,
    }))
    .into_response()
}

/// ask-kp11be-2 (SMART history): every drive's SMART state per day (or hour)
/// over up to 90 days, read from Prometheus on request. Prometheus is the
/// store, so its own retention is the cap; the window and step are clamped
/// here (at most 2160 hourly points per drive).
#[derive(Deserialize)]
struct DriveHistoryQuery {
    #[serde(default)]
    hours: Option<u64>,
    #[serde(default)]
    bucket_s: Option<u64>,
    #[serde(default)]
    offset_s: Option<i64>,
}

async fn drive_history(State(c): State<ReadCtx>, Query(q): Query<DriveHistoryQuery>) -> Response {
    let Some(prom) = &c.prometheus else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the drives' SMART history",
            "no Prometheus is configured for this dashboard",
            "set admin.prometheus_url (or HOMELAB_ADMIN_PROMETHEUS_URL) to Prometheus's address",
        );
    };
    let hours = q.hours.unwrap_or(24 * 90).clamp(1, 24 * 90);
    let bucket = (q.bucket_s.unwrap_or(86_400).max(3600) / 3600) * 3600;
    let offset = q.offset_s.unwrap_or(0).clamp(-14 * 3600, 14 * 3600);
    let end = now_s();
    let start = end.saturating_sub(hours * 3600);
    let series = match prom
        .range(
            "min by (device) (min_over_time(smart_device_health_ok[1h]))",
            start,
            end,
            3600,
            Some("device"),
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            return refused(
                StatusCode::BAD_GATEWAY,
                "the drives' SMART history",
                &e,
                "check that Prometheus answers queries from this dashboard",
            );
        }
    };
    let b = bucket as i64;
    let first = (start as i64 + offset).div_euclid(b) * b;
    let last = (end as i64 + offset).div_euclid(b) * b;
    let mut drives = serde_json::Map::new();
    for s in series {
        let Some(name) = s["label"].as_str().filter(|l| !l.is_empty()) else {
            continue;
        };
        let pts: Vec<(i64, f64)> = s["points"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| Some((p[0].as_f64()? as i64, p[1].as_f64()?)))
                    .collect()
            })
            .unwrap_or_default();
        let mut periods = Vec::new();
        let mut at = first;
        while at <= last {
            let (lo, hi) = (at - offset, at - offset + b);
            let inside: Vec<&(i64, f64)> = pts.iter().filter(|p| p.0 >= lo && p.0 < hi).collect();
            let bad = inside.iter().filter(|p| p.1 < 1.0).count();
            periods.push(serde_json::json!({
                "start": lo,
                "state": if inside.is_empty() { "none" } else if bad > 0 { "down" } else { "up" },
                "readings": inside.len(),
                "not_ok": bad,
            }));
            at += b;
        }
        drives.insert(name.to_string(), serde_json::json!({ "periods": periods }));
    }
    Json(serde_json::json!({
        "drives": drives, "hours": hours, "bucket_s": bucket, "measured_at": end,
    }))
    .into_response()
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

/// fix-371-1 (Kenny approved the Host tiles demo 2026-10-04): the Host
/// page's eight KPI tiles. Per figure (`homelab_core::charts::
/// host_kpi_queries`): the last 24 hours at 10 minutes for the sparkline,
/// and the last 15 minutes at one minute, whose mean is the tile's big
/// number ("avg 15 min") and whose last point is "now".
async fn host_trend(State(c): State<ReadCtx>) -> Response {
    let Some(prom) = &c.prometheus else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the host's trends",
            "no Prometheus is configured for this dashboard",
            "set admin.prometheus_url (or HOMELAB_ADMIN_PROMETHEUS_URL) to Prometheus's address",
        );
    };
    let Some(h) = &prom.host_label else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "the host's trends",
            "no charts_host is configured",
            "set admin.charts_host (or HOMELAB_ADMIN_CHARTS_HOST) to the hypervisor's host label in Prometheus",
        );
    };
    const DAY: u64 = 86_400;
    const DAY_STEP: u64 = 600;
    const RECENT: u64 = 14 * 60;
    const RECENT_STEP: u64 = 60;
    let end = now_s();
    let first = |r: Result<Vec<serde_json::Value>, String>| {
        r.map(|s| {
            s.into_iter()
                .next()
                .map(|x| x["points"].clone())
                .unwrap_or_else(|| serde_json::json!([]))
        })
    };
    let reads = homelab_core::charts::host_kpi_queries(h)
        .into_iter()
        .map(|(key, q)| async move {
            let (day, recent) = futures_util::future::join(
                prom.range(&q, end.saturating_sub(DAY), end, DAY_STEP, None),
                prom.range(&q, end.saturating_sub(RECENT), end, RECENT_STEP, None),
            )
            .await;
            match (first(day), first(recent)) {
                (Ok(series), Ok(recent)) => {
                    serde_json::json!({ "key": key, "series": series, "recent": recent })
                }
                (Err(e), _) | (_, Err(e)) => {
                    serde_json::json!({ "key": key, "series": [], "recent": [], "error": e })
                }
            }
        });
    let figures = futures_util::future::join_all(reads).await;
    Json(serde_json::json!({
        "figures": figures,
        "from": end.saturating_sub(DAY),
        "to": end,
        "step": DAY_STEP,
        "measured_at": end,
    }))
    .into_response()
}

pub fn read_router(ctx: ReadCtx) -> Router {
    Router::new()
        .route("/data/host", get(host))
        .route("/data/host/guests", get(guests))
        .route("/data/host/trend", get(host_trend))
        .route("/data/asks", get(asks))
        .route("/data/asks/answer", post(answer))
        .route("/data/logs", get(logs))
        .route("/data/charts", get(charts))
        .route("/data/traffic", get(traffic))
        .route("/data/capacity", get(capacity))
        .route("/data/disk-growth", get(disk_growth))
        .route("/data/fleet-traffic", get(fleet_traffic))
        .route("/data/fleet-cpu", get(fleet_cpu))
        .route("/data/drive-history", get(drive_history))
        .with_state(ctx)
}

#[cfg(test)]
mod tests {
    use super::{chart_bounds, cut_history_at};

    #[test]
    fn an_older_hosts_unbounded_history_is_cut_at_the_window_end() {
        let mut r = serde_json::json!({ "entries": [
            { "kind": "op", "start": 100 }, { "kind": "phase", "start": 200 },
            { "kind": "op", "start": 201 }
        ] });
        cut_history_at(&mut r, 200);
        assert_eq!(r["entries"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn a_preset_ends_now() {
        assert_eq!(
            chart_bounds(Some("1h"), None, None, 10_000),
            Ok((6_400, 10_000, 15))
        );
        assert_eq!(
            chart_bounds(None, None, None, 100_000).map(|b| b.1),
            Ok(100_000)
        );
        assert!(chart_bounds(Some("2h"), None, None, 1).is_err());
    }

    #[test]
    fn a_custom_window_wins_over_the_range() {
        assert_eq!(
            chart_bounds(Some("7d"), Some(1_000), Some(25_000), 99_999),
            Ok((1_000, 25_000, 100))
        );
    }

    #[test]
    fn a_bad_custom_window_is_refused() {
        assert!(chart_bounds(None, Some(1_000), None, 5).is_err());
        assert!(chart_bounds(None, Some(2_000), Some(1_000), 5).is_err());
        assert!(chart_bounds(None, Some(1_000), Some(1_030), 5).is_err());
        assert!(chart_bounds(None, Some(0), Some(31 * 86_400), 5).is_err());
    }
}
