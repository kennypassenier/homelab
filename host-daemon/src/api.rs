use axum::{
    Json, Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::process::Command;
use tokio::sync::broadcast;

static PROVISION_CYCLE_ACTIVE: AtomicBool = AtomicBool::new(false);

struct ProvisionCycleGuard;

impl Drop for ProvisionCycleGuard {
    fn drop(&mut self) {
        PROVISION_CYCLE_ACTIVE.store(false, Ordering::SeqCst);
    }
}

#[derive(serde::Deserialize)]
struct ClientHeartbeatPayload {
    #[serde(default)]
    active_stacks: Vec<String>,
}

#[derive(serde::Deserialize)]
struct DestroyStackPayload {
    stack_name: String,
}

#[derive(serde::Deserialize)]
struct ProvisionRequestBody {
    #[serde(default)]
    active_stacks: Vec<String>,
}

#[derive(serde::Deserialize)]
struct UpdateRequestBody {
    latch: Option<self_update::LatchPullRequest>,
    release_tag: Option<String>,
}

use crate::app::{App, BackupStatusLine, LogLevel};
use crate::liveness;
use crate::provision;
use crate::self_update;

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    message: String,
}

#[derive(Serialize)]
struct VersionResponse {
    component: String,
    version: String,
    latch_version: Option<String>,
}

/// Runtime metrics for the HOST (Proxmox node).
#[derive(Serialize, Clone, Debug)]
pub struct HostMetrics {
    pub hostname: String,
    pub ip: String,
    pub uptime_secs: u64,
    pub lxc_runtime: Vec<LxcMetric>,
}

/// Per-LXC runtime state visible to CLIENT.
#[derive(Serialize, Clone, Debug)]
pub struct LxcMetric {
    pub vmid: u32,
    pub name: String,
    pub status: String,
    pub cpu_pct: u8,
    pub ram_pct: u8,
    pub uptime_secs: u64,
}

pub async fn run_server(app: Arc<Mutex<App>>) {
    {
        let mut a = app.lock().unwrap();
        a.add_log(
            LogLevel::Info,
            "HOST API server listening on 0.0.0.0:8080".to_string(),
        );
    }

    let listener = match tokio::net::TcpListener::bind("0.0.0.0:8080").await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind HOST API server on :8080 — {}", e);
            return;
        }
    };

    let router = Router::new()
        .route("/api/health", get(handle_health))
        .route("/api/heartbeat", post(handle_client_heartbeat))
        .route("/api/version", get(handle_version))
        .route("/api/metrics", get(handle_metrics))
        .route("/api/update", post(handle_update))
        .route("/api/provision", post(handle_provision))
        .route("/api/provision/destroy", post(handle_destroy_stack))
        .route("/api/logs/ws", get(handle_ws))
        .with_state(app);

    axum::serve(listener, router)
        .await
        .unwrap_or_else(|e| eprintln!("HOST API server error: {}", e));
}

async fn handle_health() -> Json<ApiResponse> {
    Json(ApiResponse {
        status: "ok".to_string(),
        message: "HOST daemon is running".to_string(),
    })
}

async fn handle_client_heartbeat(
    payload: Option<Json<ClientHeartbeatPayload>>,
) -> Json<ApiResponse> {
    liveness::touch_client_heartbeat();
    if let Some(Json(body)) = payload {
        liveness::set_client_active_stacks(&body.active_stacks);
    }
    Json(ApiResponse {
        status: "ok".to_string(),
        message: "CLIENT heartbeat accepted".to_string(),
    })
}

async fn handle_version() -> Json<VersionResponse> {
    let latch_version = get_latch_version().ok();
    Json(VersionResponse {
        component: "host-daemon".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        latch_version,
    })
}

fn get_latch_version() -> Result<String, Box<dyn std::error::Error>> {
    use std::process::Command;
    let latch_bin = resolve_latch_binary().ok_or("latch not found")?;
    let output = Command::new(latch_bin).arg("--version").output()?;
    if output.status.success() {
        let version_str = String::from_utf8(output.stdout)?.trim().to_string();
        Ok(version_str)
    } else {
        Err("latch not found or failed".into())
    }
}

async fn handle_metrics(
    headers: HeaderMap,
    State(app): State<Arc<Mutex<App>>>,
) -> Result<Json<HostMetrics>, (StatusCode, Json<ApiResponse>)> {
    let expected_token = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    if !is_authorized_request(&headers, &expected_token) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse {
                status: "error".to_string(),
                message: "unauthorized".to_string(),
            }),
        ));
    }

    let mut a = app.lock().unwrap();
    let uptime_secs = a.started_at.elapsed().as_secs();
    let lxc_runtime = a
        .lxc_nodes()
        .into_iter()
        .map(|node| {
            let status = node.status;
            let is_running = status.eq_ignore_ascii_case("RUN");
            let ram_pct = node.ram.as_deref().and_then(parse_ram_pct).unwrap_or(0);
            LxcMetric {
                vmid: node.id,
                name: node.name,
                status,
                cpu_pct: node.cpu.round().clamp(0.0, 100.0) as u8,
                ram_pct,
                uptime_secs: if is_running { uptime_secs } else { 0 },
            }
        })
        .collect();

    a.current_metrics.uptime_secs = uptime_secs;
    a.current_metrics.lxc_runtime = lxc_runtime;
    Ok(Json(a.current_metrics.clone()))
}

fn is_authorized_request(headers: &HeaderMap, expected_token: &str) -> bool {
    let expected = expected_token.trim();
    if expected.is_empty() {
        return true;
    }

    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|token| token.trim() == expected)
        .unwrap_or(false)
}

fn parse_ram_pct(ram: &str) -> Option<u8> {
    let (used_str, total_str) = ram.split_once('/')?;
    let used = used_str.trim().parse::<f64>().ok()?;
    let total = total_str
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())?;
    if total <= 0.0 {
        return None;
    }
    Some(((used / total) * 100.0).round().clamp(0.0, 100.0) as u8)
}

fn resolve_latch_binary() -> Option<String> {
    if let Ok(value) = std::env::var("LATCH_BIN") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    ["/usr/local/bin/latch", "/usr/bin/latch", "/home/linuxbrew/.linuxbrew/bin/latch", "latch"]
        .iter()
        .find_map(|candidate| {
            if *candidate == "latch" {
                let output = Command::new(candidate).arg("--version").output().ok()?;
                if output.status.success() {
                    return Some(candidate.to_string());
                }
                return None;
            }

            if std::path::Path::new(candidate).exists() {
                Some(candidate.to_string())
            } else {
                None
            }
        })
}

async fn handle_update(
    State(app): State<Arc<Mutex<App>>>,
    payload: Option<Json<UpdateRequestBody>>,
) -> Json<ApiResponse> {
    {
        let mut a = app.lock().unwrap();
        a.add_log(
            LogLevel::Info,
            "HOST update requested via HTTP API".to_string(),
        );
    }

    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        let latch = payload.as_ref().and_then(|Json(body)| body.latch.as_ref());
        let release_tag = payload.as_ref().and_then(|Json(body)| body.release_tag.as_deref());
        let result = self_update::check_and_apply_update_with_latch_pull(latch, release_tag);
        let mut a = app_clone.lock().unwrap();
        match result {
            Ok(msg) => a.add_log(LogLevel::Info, format!("[update-http] {}", msg)),
            Err(err) => a.add_log(LogLevel::Error, format!("[update-http] {}", err)),
        }
    });

    Json(ApiResponse {
        status: "accepted".to_string(),
        message: "HOST update check started".to_string(),
    })
}

async fn handle_provision(
    State(app): State<Arc<Mutex<App>>>,
    payload: Option<Json<ProvisionRequestBody>>,
) -> Json<ApiResponse> {
    let active_stacks = payload
        .map(|Json(body)| body.active_stacks)
        .unwrap_or_default();
    {
        let mut a = app.lock().unwrap();
        a.add_log(
            LogLevel::Info,
            format!(
                "[provision] LXC provisioning requested via HTTP API active_stacks={}",
                active_stacks.join(",")
            ),
        );
    }

    if !active_stacks.is_empty() {
        liveness::set_client_active_stacks(&active_stacks);
    }

    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        run_provisioning_cycle(&app_clone, false);
    });

    Json(ApiResponse {
        status: "accepted".to_string(),
        message: "LXC provisioning cycle started".to_string(),
    })
}

async fn handle_destroy_stack(
    State(app): State<Arc<Mutex<App>>>,
    Json(payload): Json<DestroyStackPayload>,
) -> Json<ApiResponse> {
    {
        let mut a = app.lock().unwrap();
        a.add_log(
            LogLevel::Warn,
            format!(
                "[provision] destroy requested via HTTP API stack={}",
                payload.stack_name
            ),
        );
    }

    let app_clone = app.clone();
    let stack_name = payload.stack_name;
    tokio::task::spawn_blocking(move || {
        run_destroy_stack_cycle(&app_clone, &stack_name);
    });

    Json(ApiResponse {
        status: "accepted".to_string(),
        message: "Stack container destroy started".to_string(),
    })
}

async fn handle_ws(ws: WebSocketUpgrade, State(app): State<Arc<Mutex<App>>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, app))
}

async fn handle_ws_client(mut socket: WebSocket, app: Arc<Mutex<App>>) {
    // Subscribe and snapshot existing logs so reconnecting clients don't miss history.
    let (mut rx, snapshot): (broadcast::Receiver<String>, Vec<String>) = {
        let guard = app.lock().unwrap();
        (
            guard.log_tx.subscribe(),
            compact_snapshot(&guard.backup_status, 120),
        )
    };

    for line in snapshot {
        if socket.send(Message::Text(line)).await.is_err() {
            return;
        }
    }

    let mut keepalive_tick = tokio::time::interval(std::time::Duration::from_secs(20));

    loop {
        tokio::select! {
            _ = keepalive_tick.tick() => {
                let keepalive = serde_json::json!({
                    "kind": "ws_keepalive",
                    "component": "host-daemon"
                });
                if socket.send(Message::Text(keepalive.to_string())).await.is_err() {
                    break;
                }
            }
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg)).await.is_err() {
                            break; // client disconnected
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Handle RPC messages from CLIENT (update_request, etc.)
                        if let Ok(req) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(kind) = req.get("kind").and_then(|v| v.as_str()) {
                                let request_id = req.get("request_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                                if kind == "update_request" {
                                    {
                                        let mut guard = app.lock().unwrap();
                                        guard.add_log(
                                            LogLevel::Info,
                                            "HOST update requested via WebSocket RPC".to_string(),
                                        );
                                    }

                                    let app_clone = app.clone();
                                    let latch = req
                                        .get("latch")
                                        .and_then(|v| {
                                            serde_json::from_value::<self_update::LatchPullRequest>(
                                                v.clone(),
                                            )
                                            .ok()
                                        });
                                    let release_tag = req
                                        .get("release_tag")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string());
                                    tokio::task::spawn_blocking(move || {
                                        let result = self_update::check_and_apply_update_with_latch_pull(
                                            latch.as_ref(),
                                            release_tag.as_deref(),
                                        );
                                        let mut guard = app_clone.lock().unwrap();
                                        match result {
                                            Ok(msg) => guard.add_log(LogLevel::Info, format!("[update-rpc] {}", msg)),
                                            Err(err) => guard.add_log(LogLevel::Error, format!("[update-rpc] {}", err)),
                                        }
                                    });

                                    let response = serde_json::json!({
                                        "kind": "update_response",
                                        "request_id": request_id,
                                        "ok": true,
                                        "message": "HOST update check started"
                                    });
                                    let _ = socket.send(Message::Text(response.to_string())).await;
                                } else if kind == "provision_request" {
                                    let active_stacks = req
                                        .get("active_stacks")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                                .collect::<Vec<String>>()
                                        })
                                        .unwrap_or_default();
                                    {
                                        let mut guard = app.lock().unwrap();
                                        guard.add_log(
                                            LogLevel::Info,
                                            format!(
                                                "[provision] LXC provisioning requested via WebSocket RPC active_stacks={}",
                                                active_stacks.join(",")
                                            ),
                                        );
                                    }

                                    if !active_stacks.is_empty() {
                                        liveness::set_client_active_stacks(&active_stacks);
                                    }

                                    let app_clone = app.clone();
                                    tokio::task::spawn_blocking(move || {
                                        run_provisioning_cycle(&app_clone, false);
                                    });

                                    let response = serde_json::json!({
                                        "kind": "provision_response",
                                        "request_id": request_id,
                                        "ok": true,
                                        "message": "LXC provisioning cycle started"
                                    });
                                    let _ = socket.send(Message::Text(response.to_string())).await;
                                } else if kind == "destroy_stack_request" {
                                    let stack_name = req
                                        .get("stack_name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .trim()
                                        .to_string();

                                    if stack_name.is_empty() {
                                        let response = serde_json::json!({
                                            "kind": "destroy_stack_response",
                                            "request_id": request_id,
                                            "ok": false,
                                            "message": "stack_name is required"
                                        });
                                        let _ = socket.send(Message::Text(response.to_string())).await;
                                    } else {
                                        {
                                            let mut guard = app.lock().unwrap();
                                            guard.add_log(
                                                LogLevel::Warn,
                                                format!(
                                                    "[provision] destroy requested via WebSocket RPC stack={}",
                                                    stack_name
                                                ),
                                            );
                                        }

                                        let app_clone = app.clone();
                                        let stack_for_worker = stack_name.clone();
                                        tokio::task::spawn_blocking(move || {
                                            run_destroy_stack_cycle(&app_clone, &stack_for_worker);
                                        });

                                        let response = serde_json::json!({
                                            "kind": "destroy_stack_response",
                                            "request_id": request_id,
                                            "ok": true,
                                            "message": "Stack container destroy started"
                                        });
                                        let _ = socket.send(Message::Text(response.to_string())).await;
                                    }
                                } else if kind == "client_heartbeat" {
                                    let active_stacks = req
                                        .get("active_stacks")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                                .collect::<Vec<String>>()
                                        })
                                        .unwrap_or_default();
                                    liveness::touch_client_heartbeat();
                                    liveness::set_client_active_stacks(&active_stacks);
                                    // Cache any latch credentials CLIENT included with this heartbeat.
                                    if let Some(latch_val) = req.get("latch") {
                                        if let Ok(latch) = serde_json::from_value::<self_update::LatchPullRequest>(latch_val.clone()) {
                                            let mut guard = app.lock().unwrap();
                                            guard.latch_credentials = Some(latch);
                                        }
                                    }
                                    let response = serde_json::json!({
                                        "kind": "client_heartbeat_response",
                                        "request_id": request_id,
                                        "ok": true,
                                        "message": "CLIENT heartbeat accepted"
                                    });
                                    let _ = socket.send(Message::Text(response.to_string())).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // ignore ping/pong/binary from client
                }
            }
        }
    }
}

fn compact_snapshot(lines: &VecDeque<BackupStatusLine>, max_lines: usize) -> Vec<String> {
    if lines.is_empty() || max_lines == 0 {
        return Vec::new();
    }

    let start = lines.len().saturating_sub(max_lines * 3);
    let mut compacted = Vec::new();
    let mut previous: Option<&str> = None;

    for line in lines.iter().skip(start) {
        let as_str = line.message.as_str();
        if previous == Some(as_str) {
            continue;
        }
        compacted.push(line.message.clone());
        previous = Some(as_str);
    }

    if compacted.len() > max_lines {
        compacted[compacted.len() - max_lines..].to_vec()
    } else {
        compacted
    }
}

/// Run one provisioning reconcile cycle (called from HTTP and WS RPC handlers).
/// Logs all results back into the app log buffer so CLIENT can see them via WS.
pub fn run_provisioning_cycle(app: &Arc<Mutex<App>>, dry_run: bool) {
    use std::path::Path;
    use std::process::Command;

    if PROVISION_CYCLE_ACTIVE.swap(true, Ordering::SeqCst) {
        app.lock().unwrap().add_log(
            LogLevel::Info,
            "[provision] cycle already running; skipping duplicate request".to_string(),
        );
        return;
    }
    let _provision_cycle_guard = ProvisionCycleGuard;

    let gitops_root = std::env::var("GITOPS_REPO").unwrap_or_else(|_| {
        std::env::var("HOME")
            .map(|home| format!("{}/homelab", home))
            .unwrap_or_else(|_| "/root/homelab".to_string())
    });

    // Force git pull to ensure we have latest stacks before provisioning.
    // Use reset --hard to discard local changes (build artifacts, etc).
    {
        let mut a = app.lock().unwrap();
        a.add_log(
            LogLevel::Info,
            format!(
                "[provision] force pulling latest changes from git repo: {}",
                gitops_root
            ),
        );
    }

    let reset_output = Command::new("git")
        .args(["reset", "--hard", "origin/main"])
        .current_dir(&gitops_root)
        .output();

    match reset_output {
        Ok(out) if out.status.success() => {
            let mut a = app.lock().unwrap();
            a.add_log(
                LogLevel::Info,
                "[provision] git reset --hard origin/main succeeded".to_string(),
            );
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let mut a = app.lock().unwrap();
            a.add_log(
                LogLevel::Error,
                format!("[provision] git reset failed: {}", stderr),
            );
            return;
        }
        Err(e) => {
            let mut a = app.lock().unwrap();
            a.add_log(
                LogLevel::Error,
                format!("[provision] git reset command failed: {}", e),
            );
            return;
        }
    }

    let pull_output = Command::new("git")
        .args(["pull"])
        .current_dir(&gitops_root)
        .output();

    match pull_output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut a = app.lock().unwrap();
            a.add_log(
                LogLevel::Info,
                format!(
                    "[provision] git pull succeeded: {}",
                    stdout.lines().next().unwrap_or("up to date")
                ),
            );
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let mut a = app.lock().unwrap();
            a.add_log(
                LogLevel::Error,
                format!("[provision] git pull failed: {}", stderr),
            );
            return;
        }
        Err(e) => {
            let mut a = app.lock().unwrap();
            a.add_log(
                LogLevel::Error,
                format!("[provision] git pull command failed: {}", e),
            );
            return;
        }
    }

    let app_for_log = app.clone();
    let log = move |level: &str, msg: &str| {
        let log_level = match level {
            "error" => LogLevel::Error,
            "warn" => LogLevel::Warn,
            "ok" => LogLevel::Ok,
            _ => LogLevel::Info,
        };
        app_for_log
            .lock()
            .unwrap()
            .add_log(log_level, msg.to_string());
    };

    let actions =
        match provision::apply_provisioning_changes(Path::new(&gitops_root), dry_run, &log) {
            Ok(actions) => actions,
            Err(e) => {
                app.lock()
                    .unwrap()
                    .add_log(LogLevel::Error, format!("[provision] failed: {}", e));
                return;
            }
        };

    let summary = provision::format_provision_summary(&actions);
    let mut a = app.lock().unwrap();
    for line in summary {
        if line.is_empty() {
            continue;
        }
        let level = if line.contains("SKIP") || line.contains("Summary") {
            LogLevel::Info
        } else if line.contains("CREATE")
            || line.contains("RECREATE")
            || line.contains("UPDATE")
            || line.contains("RESUME_BOOTSTRAP")
        {
            LogLevel::Ok
        } else {
            LogLevel::Info
        };
        a.add_log(level, format!("[provision] {}", line));
    }
}

/// Destroy one stack container by stack name (called from HTTP and WS RPC handlers).
pub fn run_destroy_stack_cycle(app: &Arc<Mutex<App>>, stack_name: &str) {
    use std::path::Path;

    let gitops_root = std::env::var("GITOPS_REPO").unwrap_or_else(|_| {
        std::env::var("HOME")
            .map(|home| format!("{}/homelab", home))
            .unwrap_or_else(|_| "/root/homelab".to_string())
    });

    let stack = stack_name.to_string();
    let app_for_log = app.clone();
    let log = move |level: &str, msg: &str| {
        let log_level = match level {
            "error" => LogLevel::Error,
            "warn" => LogLevel::Warn,
            "ok" => LogLevel::Ok,
            _ => LogLevel::Info,
        };
        app_for_log
            .lock()
            .unwrap()
            .add_log(log_level, msg.to_string());
    };

    match provision::destroy_stack_container(Path::new(&gitops_root), &stack, false, &log) {
        Ok(()) => app.lock().unwrap().add_log(
            LogLevel::Ok,
            format!("[provision] destroy complete stack={}", stack),
        ),
        Err(e) => app.lock().unwrap().add_log(
            LogLevel::Error,
            format!("[provision] destroy failed stack={} error={}", stack, e),
        ),
    }
}
