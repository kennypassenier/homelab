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
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::app::{App, LogLevel};
use crate::liveness;
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

async fn handle_client_heartbeat() -> Json<ApiResponse> {
    liveness::touch_client_heartbeat();
    Json(ApiResponse {
        status: "ok".to_string(),
        message: "CLIENT heartbeat accepted".to_string(),
    })
}

async fn handle_version() -> Json<VersionResponse> {
    Json(VersionResponse {
        component: "host-daemon".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
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
            let ram_pct = node
                .ram
                .as_deref()
                .and_then(parse_ram_pct)
                .unwrap_or(0);
            LxcMetric {
                vmid: node.id,
                name: node.name,
                status,
                cpu_pct: node.cpu.round().clamp(0.0, 100.0) as u8,
                ram_pct,
                uptime_secs: if is_running {
                    uptime_secs
                } else {
                    0
                },
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

async fn handle_update(State(app): State<Arc<Mutex<App>>>) -> Json<ApiResponse> {
    {
        let mut a = app.lock().unwrap();
        a.add_log(
            LogLevel::Info,
            "HOST update requested via HTTP API".to_string(),
        );
    }

    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        let result = self_update::check_and_apply_update();
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
                                    tokio::task::spawn_blocking(move || {
                                        let result = self_update::check_and_apply_update();
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
                                } else if kind == "client_heartbeat" {
                                    liveness::touch_client_heartbeat();
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

fn compact_snapshot(lines: &[String], max_lines: usize) -> Vec<String> {
    if lines.is_empty() || max_lines == 0 {
        return Vec::new();
    }

    let start = lines.len().saturating_sub(max_lines * 3);
    let mut compacted = Vec::new();
    let mut previous: Option<&str> = None;

    for line in &lines[start..] {
        let as_str = line.as_str();
        if previous == Some(as_str) {
            continue;
        }
        compacted.push(line.clone());
        previous = Some(as_str);
    }

    if compacted.len() > max_lines {
        compacted[compacted.len() - max_lines..].to_vec()
    } else {
        compacted
    }
}
