//! The TUI parity round's read routes (Kenny, 2026-09-28: "Wat nu in de TUI
//! kan, moet nog altijd kunnen in ons systeem"): today with its verdict, the
//! fleet check, one incident bundle, the templates, ping with where the
//! address came from, the versions and the update badge, drift per stack,
//! the apply plan and a stack's deploy plan, and the files a browser
//! downloads (the runbook, a stack's export bundle, its Grafana dashboard).
//!
//! Every write goes through the action queue (`shell::actions`), so a press
//! here and a driven press are the same job; these routes only read.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use homelab_proto::Command;
use serde::Deserialize;

use super::actions::{Actions, HostPort, LocalStacks, StackFiles};
use super::host_link::{now_s, Shared};
use super::slow::{RunQuery, SlowRead, WAIT};
use crate::core::actions::{valid_stack_name, Refusal};
use crate::core::drift::drift_state;

/// How long a drift reading is reused (it runs latch once per stack).
pub const DRIFT_REUSE_S: u64 = 300;

#[derive(Clone)]
pub struct ParityCtx {
    pub host: Arc<dyn HostPort>,
    pub shared: Shared,
    pub actions: Actions,
    pub files: Arc<dyn StackFiles>,
    /// The working copy (`…/repo`); its `stacks/` is what the CLI reads.
    pub repo: PathBuf,
    /// Where a download is written before it is read back and removed.
    pub scratch: PathBuf,
    drift: Arc<Mutex<Option<(u64, LocalStacks)>>>,
    /// Today and the fleet check take about 90 s: started once, asked after
    /// (`shell::slow`).
    today_read: Arc<SlowRead>,
    check_read: Arc<SlowRead>,
}

impl ParityCtx {
    pub fn new(
        host: Arc<dyn HostPort>,
        shared: Shared,
        actions: Actions,
        files: Arc<dyn StackFiles>,
        repo: PathBuf,
        scratch: PathBuf,
        publish: Arc<dyn super::actions::Publish>,
    ) -> Self {
        ParityCtx {
            host,
            shared,
            actions,
            files,
            repo,
            scratch,
            drift: Arc::new(Mutex::new(None)),
            today_read: SlowRead::announced("today", "today", publish.clone()),
            check_read: SlowRead::announced("the fleet check", "fleet-check", publish),
        }
    }
}

fn refused(status: StatusCode, r: Refusal) -> Response {
    (status, Json(r)).into_response()
}

fn bad_gateway(what: &str, why: String) -> Response {
    refused(
        StatusCode::BAD_GATEWAY,
        Refusal::new(
            what,
            why,
            "the dashboard asks the host over its one line; check that it answers (Ping on the host page)",
        ),
    )
}

async fn ask(
    c: &ParityCtx,
    command: Command,
    secs: u64,
) -> Result<homelab_proto::RpcResponse, String> {
    c.host
        .ask_traced(command, Duration::from_secs(secs), None)
        .await
}

/// The working copy's stack files with the vmid each claims, and their
/// digests, as `homelab check` and `homelab today` send them. Empty (with
/// the reason) without a working copy: the host's half is still asked.
fn stack_side(
    repo: &std::path::Path,
) -> (
    Vec<(String, u16)>,
    Vec<homelab_core::ops::fleetcheck::StackDigest>,
    Option<String>,
) {
    let base = repo.join("stacks");
    if !base.is_dir() {
        return (
            Vec::new(),
            Vec::new(),
            Some(
                "the dashboard has no working copy, so only what the host can see was checked; \
                 the half that compares the stack files with the fleet was skipped"
                    .into(),
            ),
        );
    }
    let files = homelab_client::spec::stack_files_with_vmids(&base.display().to_string());
    let digests = files
        .iter()
        .filter_map(|(dir, _)| homelab_client::spec::stack_digest(std::path::Path::new(dir)).ok())
        .collect();
    (files, digests, None)
}

// ── today, the fleet check ───────────────────────────────────────────────

/// A failed host read as the slow read's answer: `{what, why, fix}`, 502.
fn gateway_value(what: &str, why: String) -> (StatusCode, serde_json::Value) {
    (
        StatusCode::BAD_GATEWAY,
        serde_json::to_value(Refusal::new(
            what,
            why,
            "the dashboard asks the host over its one line; check that it answers (Ping on the host page)",
        ))
        .unwrap_or_default(),
    )
}

/// fix-68 (`homelab today`): doctor, the fleet check with its manual checks
/// and the open incidents as one list and one verdict. About 90 s on pve,
/// so it is a slow read: started once, asked after with `?run=`.
async fn today(State(c): State<ParityCtx>, Query(q): Query<RunQuery>) -> Response {
    let read = c.today_read.clone();
    read.read(q.run, WAIT, move || read_today(c)).await
}

/// The host's Today reading with the working copy's stack files: the list,
/// how many stack files went with it, and why a half was skipped. The page
/// and the 09:00 digest both read it here.
async fn today_with_files(
    host: &Arc<dyn HostPort>,
    repo: &std::path::Path,
) -> Result<(homelab_core::ops::today::Today, usize, Option<String>), String> {
    let repo = repo.to_path_buf();
    let (stack_files, digests, skipped) = tokio::task::spawn_blocking(move || stack_side(&repo))
        .await
        .unwrap_or_default();
    let n = stack_files.len();
    let r = host
        .ask_traced(
            Command::Today {
                stack_files,
                digests,
            },
            Duration::from_secs(180),
            None,
        )
        .await?;
    serde_json::from_str::<homelab_core::ops::today::Today>(&r.message)
        .map(|t| (t, n, skipped))
        .map_err(|_| {
            format!(
                "the host answered: {}",
                r.message.chars().take(200).collect::<String>()
            )
        })
}

/// Kenny, 2026-09-30 09:16: every row (a Today item, a finding) whose
/// remedy names a command the dashboard runs carries it as `fix`, the
/// action dialog its button opens (`core::notify::fix_for`).
pub fn with_fixes(mut rows: serde_json::Value) -> serde_json::Value {
    if let Some(list) = rows.as_array_mut() {
        for row in list.iter_mut() {
            let fix = row.get("remedy").and_then(|r| r.as_str()).and_then(|r| {
                crate::core::notify::fix_for(&crate::core::notify::FixSource::Text(r))
            });
            if let (Some(o), Some(f)) = (row.as_object_mut(), fix) {
                o.insert("fix".into(), serde_json::to_value(f).unwrap_or_default());
            }
        }
    }
    rows
}

/// Decision daily-digest: the open Today items.
pub async fn fetch_today(
    host: &Arc<dyn HostPort>,
    repo: &std::path::Path,
) -> Result<homelab_core::ops::today::Today, String> {
    today_with_files(host, repo).await.map(|(t, _, _)| t)
}

async fn read_today(c: ParityCtx) -> (StatusCode, serde_json::Value) {
    match today_with_files(&c.host, &c.repo).await {
        Ok((t, n, skipped)) => {
            let mut today = serde_json::to_value(&t).unwrap_or_default();
            today["items"] = with_fixes(today["items"].clone());
            (
                StatusCode::OK,
                serde_json::json!({
                    "today": today,
                    "verdict": t.verdict(),
                    "needs_you": t.needs_you(),
                    "stack_files": n,
                    "skipped": skipped,
                    "measured_at": now_s(),
                }),
            )
        }
        Err(e) => gateway_value("today", e),
    }
}

/// Y4 (`homelab check`, the TUI's c): the repository against the fleet.
/// The edge and pin comparisons need a workstation's Cloudflare token and
/// registry access; the page says so. About 90 s on pve: a slow read.
async fn fleet_check(State(c): State<ParityCtx>, Query(q): Query<RunQuery>) -> Response {
    let read = c.check_read.clone();
    read.read(q.run, WAIT, move || read_fleet_check(c)).await
}

async fn read_fleet_check(c: ParityCtx) -> (StatusCode, serde_json::Value) {
    let repo = c.repo.clone();
    let (stack_files, digests, skipped) = tokio::task::spawn_blocking(move || stack_side(&repo))
        .await
        .unwrap_or_default();
    let n = stack_files.len();
    match ask(
        &c,
        Command::FleetCheck {
            stack_files,
            digests,
            json: true,
        },
        180,
    )
    .await
    {
        Ok(r) => match serde_json::from_str::<serde_json::Value>(&r.message) {
            Ok(v) => (
                StatusCode::OK,
                serde_json::json!({
                    "passes": v["passes"],
                    "findings": with_fixes(v["findings"].clone()),
                    "stack_files": n,
                    "skipped": skipped,
                    "not_here": "The Cloudflare edge and the registries' pinned digests are compared from a workstation: homelab check.",
                    "measured_at": now_s(),
                }),
            ),
            Err(_) => gateway_value(
                "the fleet check",
                format!(
                    "the host answered text, not JSON: {}",
                    r.message.chars().take(200).collect::<String>()
                ),
            ),
        },
        Err(e) => gateway_value("the fleet check", e),
    }
}

// ── one incident, the templates, ping, versions ─────────────────────────

/// A bundle name as `Incidents` lists them (`1790530911-deploy-uptime`).
pub fn valid_incident(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !name.contains("..")
}

/// fix-131 (`homelab incidents show <name>`): one bundle, the error, the
/// versions and the end of its transcript.
async fn incident(State(c): State<ParityCtx>, UrlPath(name): UrlPath<String>) -> Response {
    if !valid_incident(&name) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("incident {name:?}"),
                "not a bundle name",
                "pick one from the incidents list",
            ),
        );
    }
    match ask(&c, Command::IncidentShow { name: name.clone() }, 60).await {
        Ok(r) if r.ok => Json(serde_json::json!({
            "name": name,
            "text": homelab_core::executor::mask_secrets(&r.message),
        }))
        .into_response(),
        Ok(r) => refused(
            StatusCode::NOT_FOUND,
            Refusal::new(
                format!("incident {name}"),
                r.message,
                "pick one from the incidents list",
            ),
        ),
        Err(e) => bad_gateway("the incident", e),
    }
}

/// C5 (`homelab templates`): the golden templates and the OS templates.
async fn templates(State(c): State<ParityCtx>) -> Response {
    match ask(&c, Command::ListTemplates, 90).await {
        Ok(r) if r.ok => Json(serde_json::json!({
            "templates": crate::core::templates::parse(&r.message),
            "text": r.message,
            "measured_at": now_s(),
        }))
        .into_response(),
        Ok(r) => bad_gateway("the templates", r.message),
        Err(e) => bad_gateway("the templates", e),
    }
}

/// `homelab ping`: the round trip, the address and where it came from.
async fn ping(State(c): State<ParityCtx>) -> Response {
    let t = std::time::Instant::now();
    let r = ask(&c, Command::Ping, 15).await;
    let ms = t.elapsed().as_millis() as u64;
    let s = c.shared.read().await;
    let facts = serde_json::json!({
        "address": s.host_addr,
        "address_source": s.host_addr_source,
        "pin": homelab_client::repo_config::built_in_pin().map(|p| format!("SHA256:{p}")),
        "pin_source": "compiled into this build (the repository's config/client.toml pin)",
        "tls_fingerprint": s.fleet.as_ref().map(|f| f.host.tls_fingerprint.clone()),
        "host_version": s.host_version,
    });
    drop(s);
    match r {
        Ok(r) => Json(serde_json::json!({
            "ok": r.ok, "message": r.message, "ms": ms, "facts": facts, "measured_at": now_s(),
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({
            "ok": false, "message": e, "ms": ms, "facts": facts, "measured_at": now_s(),
        }))
        .into_response(),
    }
}

/// The versions and the two warnings (update available, dashboard older).
async fn versions(State(c): State<ParityCtx>) -> Json<serde_json::Value> {
    let s = c.shared.read().await;
    let mut v = crate::core::hostversion::release_view(
        s.latest_release.as_deref(),
        s.host_version.as_deref(),
    );
    v["checked_at"] = serde_json::json!(s.latest_checked_at);
    Json(v)
}

// ── drift, the plans ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Fresh {
    #[serde(default)]
    fresh: bool,
}

/// [CHANGED] per stack (fix-107's words). Computing it runs latch once per
/// stack (the host's hash covers the secrets), so it runs only when asked
/// (`fresh`, the pages' "Compare with the files"); otherwise the newest
/// reading is answered, or none, and nothing is computed behind a page view.
async fn drift(State(c): State<ParityCtx>, Query(q): Query<Fresh>) -> Response {
    let now = now_s();
    let cached = c
        .drift
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let reuse = cached
        .clone()
        .filter(|(at, _)| !q.fresh || now.saturating_sub(*at) < DRIFT_REUSE_S);
    let (at, local) = match (reuse, q.fresh) {
        (Some(x), _) => x,
        (None, false) => {
            return Json(serde_json::json!({ "stacks": {}, "measured_at": null })).into_response()
        }
        (None, true) => {
            let files = c.files.clone();
            let read = tokio::task::spawn_blocking(move || files.local_stacks(false)).await;
            match read {
                Ok(Ok(local)) => {
                    *c.drift.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some((now, local.clone()));
                    (now, local)
                }
                Ok(Err(r)) => return refused(StatusCode::CONFLICT, r),
                Err(e) => {
                    return refused(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Refusal::new("drift", e.to_string(), "report this"),
                    )
                }
            }
        }
    };
    let s = c.shared.read().await;
    let Some(fleet) = s.fleet.as_ref() else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            Refusal::new(
                "drift",
                "the dashboard has not read the host's fleet yet",
                "wait for the fleet page",
            ),
        );
    };
    let stacks: serde_json::Map<String, serde_json::Value> = fleet
        .stacks
        .iter()
        .map(|st| {
            let local_hash = local
                .hashes
                .iter()
                .find(|(n, _)| *n == st.name)
                .map(|(_, h)| h);
            let state = drift_state(&st.applied_hash, local_hash);
            let why = local_hash.and_then(|h| h.as_ref().err()).cloned();
            (
                st.name.clone(),
                serde_json::json!({ "state": state, "label": state.label(), "why": why }),
            )
        })
        .collect();
    Json(serde_json::json!({ "stacks": stacks, "measured_at": at })).into_response()
}

/// dash-apply: the plan against the host (no programs are downloaded; the
/// secrets are read through latch, since the host's hash covers them).
async fn apply_plan(State(c): State<ParityCtx>) -> Response {
    match c.actions.apply_plan(false).await {
        Ok((view, _)) => Json(serde_json::json!({
            "plan": view,
            "pending": view.pending(),
            "measured_at": now_s(),
        }))
        .into_response(),
        Err(r) => refused(StatusCode::CONFLICT, r),
    }
}

/// fix-100's per-file plan of one stack, as the Deploy review shows it.
async fn stack_plan(State(c): State<ParityCtx>, UrlPath(stack): UrlPath<String>) -> Response {
    if !valid_stack_name(&stack) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("the plan of {stack:?}"),
                "not a stack name",
                "use the name the fleet page shows",
            ),
        );
    }
    match c.actions.deploy_diff(&stack).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => refused(StatusCode::CONFLICT, r),
    }
}

// ── downloads ───────────────────────────────────────────────────────────

fn attachment(name: &str, mime: &str, body: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        body,
    )
        .into_response()
}

fn no_working_copy(what: &str) -> Response {
    refused(
        StatusCode::CONFLICT,
        Refusal::new(
            what,
            "the dashboard has no working copy of the homelab repository",
            "the working copy panel on the settings page says what is missing",
        ),
    )
}

/// E7 (`homelab runbook`): the disaster-recovery runbook, written from the
/// stack files, as a download.
async fn runbook(State(c): State<ParityCtx>) -> Response {
    let base = c.repo.join("stacks");
    if !base.is_dir() {
        return no_working_copy("the runbook");
    }
    let tmp = c.scratch.join(format!("runbook-{}.md", std::process::id()));
    let r = tokio::task::spawn_blocking(move || {
        let _ = std::fs::create_dir_all(tmp.parent().unwrap_or(&tmp));
        let out = homelab_client::spec::generate_runbook(&base, &tmp.display().to_string())
            .and_then(|_| std::fs::read_to_string(&tmp).map_err(|e| e.to_string()));
        let _ = std::fs::remove_file(&tmp);
        out
    })
    .await;
    match r {
        Ok(Ok(text)) => attachment("DR_RUNBOOK.md", "text/markdown; charset=utf-8", text),
        Ok(Err(why)) => refused(
            StatusCode::CONFLICT,
            Refusal::new("the runbook", why, "fix the stack files it names"),
        ),
        Err(e) => refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            Refusal::new("the runbook", e.to_string(), "report this"),
        ),
    }
}

/// D11 (`homelab export`): one stack as a single YAML bundle, never with
/// secrets, as a download.
async fn export(State(c): State<ParityCtx>, UrlPath(stack): UrlPath<String>) -> Response {
    if !valid_stack_name(&stack) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("export {stack:?}"),
                "not a stack name",
                "use the name the fleet page shows",
            ),
        );
    }
    let dir = c.repo.join("stacks").join(&stack);
    if !dir.is_dir() {
        return no_working_copy(&format!("export {stack}"));
    }
    match tokio::task::spawn_blocking(move || homelab_client::spec::bundle_text(&dir)).await {
        Ok(Ok((text, _))) => attachment(
            &format!("{stack}-bundle.yml"),
            "application/yaml; charset=utf-8",
            text,
        ),
        Ok(Err(why)) => refused(
            StatusCode::CONFLICT,
            Refusal::new(
                format!("export {stack}"),
                why,
                format!("fix stacks/{stack}"),
            ),
        ),
        Err(e) => refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            Refusal::new("export", e.to_string(), "report this"),
        ),
    }
}

#[derive(Deserialize)]
struct Apps {
    #[serde(default)]
    apps: Option<String>,
}

/// T2 (`homelab dashboard <stack> <app>…`): the Grafana dashboard a deploy
/// writes, as a download; the apps default to the stack's own.
async fn dashboard(
    State(c): State<ParityCtx>,
    UrlPath(stack): UrlPath<String>,
    Query(q): Query<Apps>,
) -> Response {
    if !valid_stack_name(&stack) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("dashboard {stack:?}"),
                "not a stack name",
                "use the name the fleet page shows",
            ),
        );
    }
    let mut apps: Vec<String> = q
        .apps
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .collect();
    if apps.is_empty() {
        let s = c.shared.read().await;
        apps = s
            .fleet
            .as_ref()
            .and_then(|f| f.stacks.iter().find(|x| x.name == stack))
            .map(|x| x.apps.iter().map(|a| a.name.clone()).collect())
            .unwrap_or_default();
    }
    if apps.is_empty() || apps.iter().any(|a| !valid_stack_name(a)) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("dashboard {stack}"),
                "a dashboard needs at least one app, each a plain name",
                "name the apps: ?apps=app-a,app-b",
            ),
        );
    }
    attachment(
        &format!("{stack}-dashboard.json"),
        "application/json",
        homelab_core::ops::dashboard::dashboard_json(&stack, &apps),
    )
}

/// Mounted with `dashboard_routes`: behind the login and both locks.
pub fn router(c: ParityCtx) -> Router {
    Router::new()
        .route("/data/today", get(today))
        .route("/data/fleet-check", get(fleet_check))
        .route("/data/incidents/{name}", get(incident))
        .route("/data/templates", get(templates))
        .route("/data/ping", get(ping))
        .route("/data/versions", get(versions))
        .route("/data/drift", get(drift))
        .route("/data/apply/plan", get(apply_plan))
        .route("/data/plan/{stack}", get(stack_plan))
        .route("/data/download/runbook", get(runbook))
        .route("/data/download/export/{stack}", get(export))
        .route("/data/download/dashboard/{stack}", get(dashboard))
        .with_state(c)
}
