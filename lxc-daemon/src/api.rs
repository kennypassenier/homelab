use crate::app::{AppState, LogLevel};
use crate::restore::{self, RestoreRequest, RestoreStatus};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    message: String,
}

#[derive(Deserialize)]
struct WsExecRequest {
    kind: String,
    request_id: String,
    cmd: String,
    args: Option<Vec<String>>,
    stdin: Option<String>,
    timeout_secs: Option<u64>,
    token: Option<String>,
}

#[derive(Serialize)]
struct WsExecResponse {
    kind: String,
    request_id: String,
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct WsSyncRequest {
    kind: String,
    request_id: String,
    token: Option<String>,
}

#[derive(Serialize)]
struct WsSyncResponse {
    kind: String,
    request_id: String,
    ok: bool,
    message: String,
}

#[derive(Deserialize)]
struct WsHeartbeatRequest {
    kind: String,
    request_id: String,
    token: Option<String>,
}

#[derive(Serialize)]
struct WsHeartbeatResponse {
    kind: String,
    request_id: String,
    ok: bool,
    message: String,
}

#[derive(Deserialize)]
struct WsUpdateRequest {
    kind: String,
    request_id: String,
    token: Option<String>,
}

#[derive(Serialize)]
struct WsUpdateResponse {
    kind: String,
    request_id: String,
    ok: bool,
    message: String,
}

#[derive(Deserialize)]
struct WsRestoreRequest {
    kind: String,
    request_id: String,
    token: Option<String>,
    scope: String,
    stack_names: Vec<String>,
    backup_id: String,
    verify_only: bool,
    skip_post_hooks: bool,
}

#[derive(Serialize)]
struct WsRestoreResponse {
    kind: String,
    request_id: String,
    ok: bool,
    status: RestoreStatus,
    message: String,
}

#[derive(Deserialize)]
pub struct ExecRequest {
    pub cmd: String,
    pub args: Option<Vec<String>>,
    pub stdin: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Serialize)]
pub struct ExecResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Keyring slot status
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyringSlot {
    pub slot_name: String,
    pub has_value: bool,
    pub last_updated: Option<String>,
}

/// Keyring readiness status for latch clone
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KeyringStatus {
    pub latch_available: bool,
    pub latch_version: Option<String>,
    pub latch_last_update_secs: Option<u64>,
    pub keyring_available: bool,
    pub global_slots: Vec<KeyringSlot>,
    pub project_slots: Vec<KeyringSlot>,
    pub last_sync: Option<String>,
    pub message: String,
}

pub async fn run_server(state: Arc<Mutex<AppState>>) {
    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            "Axum HTTP server listening on 0.0.0.0:8080".to_string(),
        );
    }

    let app = Router::new()
        .route("/api/sync", post(handle_sync))
        .route("/api/heartbeat", post(handle_heartbeat))
        .route("/api/update", post(handle_update))
        .route("/api/backup/pause", post(handle_backup_pause))
        .route("/api/backup/resume", post(handle_backup_resume))
        .route("/api/restore", post(handle_restore))
        .route("/api/exec", post(handle_exec))
        .route("/api/secrets/keyring", get(handle_keyring_status))
        .route("/api/logs/ws", get(handle_ws))
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind("0.0.0.0:8080").await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind API server on :8080 — {}", e);
            return;
        }
    };

    axum::serve(listener, app).await.unwrap_or_else(|e| {
        eprintln!("API server error: {}", e);
    });
}

// ── Restore backend ───────────────────────────────────────────────────────

async fn handle_restore(
    headers: HeaderMap,
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<RestoreRequest>,
) -> (StatusCode, Json<RestoreStatus>) {
    if !is_authorized(&headers) {
        let mut denied = RestoreStatus::new("restore-unauthorized".to_string(), &req);
        denied.error_message = Some("Unauthorized".to_string());
        return (StatusCode::UNAUTHORIZED, Json(denied));
    }

    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!(
                "Restore requested: scope={:?} backup_id={} stacks={}",
                req.scope,
                req.backup_id,
                req.stack_names.join(",")
            ),
        );
    }

    let lxc_api_base =
        std::env::var("LXC_API_BASE").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    let backup_root = std::env::var("BACKUP_ROOT").unwrap_or_else(|_| "/backups".to_string());
    let host_appdata_root =
        std::env::var("HOST_APPDATA_ROOT").unwrap_or_else(|_| "/appdata".to_string());

    let status =
        restore::execute_restore(&req, &lxc_api_base, &backup_root, &host_appdata_root).await;

    {
        let mut s = state.lock().unwrap();
        if status.success {
            s.sync_requested = true;
            s.add_log(
                LogLevel::Ok,
                format!("Restore completed: operation_id={}", status.operation_id),
            );
        } else {
            s.add_log(
                LogLevel::Error,
                format!(
                    "Restore failed: operation_id={} error={}",
                    status.operation_id,
                    status
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string())
                ),
            );
        }
    }

    let code = if status.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };

    (code, Json(status))
}

// ── WebSocket log stream ───────────────────────────────────────────────────

async fn handle_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<Mutex<AppState>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, state))
}

async fn handle_ws_client(mut socket: WebSocket, state: Arc<Mutex<AppState>>) {
    // Subscribe and capture current ring-buffer snapshot before entering stream loop.
    let (mut rx, snapshot, stack_name): (broadcast::Receiver<String>, Vec<String>, String) = {
        let guard = state.lock().unwrap();
        let stack_name = guard.stack_name.clone();
        let snapshot = guard
            .logs
            .iter()
            .map(|entry| {
                format!(
                    "ts={} level={} stack={} msg=\"{}\"",
                    entry.timestamp.format("%Y-%m-%dT%H:%M:%S"),
                    entry.level,
                    stack_name,
                    entry.msg.replace('"', "'")
                )
            })
            .collect::<Vec<_>>();
        (guard.log_tx.subscribe(), snapshot, stack_name)
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
                    "component": "lxc-daemon",
                    "stack": stack_name
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
                    // Missed messages due to slow consumer — skip and continue.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    // Sender dropped (daemon shutting down).
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(req) = serde_json::from_str::<WsSyncRequest>(&text) {
                            if req.kind == "sync_request" {
                                if !is_ws_authorized(req.token.as_deref()) {
                                    let _ = socket
                                        .send(Message::Text(
                                            serde_json::to_string(&WsSyncResponse {
                                                kind: "sync_response".to_string(),
                                                request_id: req.request_id,
                                                ok: false,
                                                message: "Unauthorized".to_string(),
                                            })
                                            .unwrap_or_else(|_| {
                                                "{\"kind\":\"sync_response\",\"ok\":false,\"message\":\"Unauthorized\"}".to_string()
                                            }),
                                        ))
                                        .await;
                                    continue;
                                }

                                {
                                    let mut s = state.lock().unwrap();
                                    s.sync_requested = true;
                                    s.add_log(
                                        LogLevel::Info,
                                        "Sync triggered via WebSocket RPC".to_string(),
                                    );
                                }

                                let _ = socket
                                    .send(Message::Text(
                                        serde_json::to_string(&WsSyncResponse {
                                            kind: "sync_response".to_string(),
                                            request_id: req.request_id,
                                            ok: true,
                                            message: "Sync queued".to_string(),
                                        })
                                        .unwrap_or_else(|_| {
                                            "{\"kind\":\"sync_response\",\"ok\":true,\"message\":\"Sync queued\"}".to_string()
                                        }),
                                    ))
                                    .await;
                                continue;
                            }
                        }

                        if let Ok(req) = serde_json::from_str::<WsHeartbeatRequest>(&text) {
                            if req.kind == "heartbeat_request" {
                                if !is_ws_authorized(req.token.as_deref()) {
                                    let _ = socket
                                        .send(Message::Text(
                                            serde_json::to_string(&WsHeartbeatResponse {
                                                kind: "heartbeat_response".to_string(),
                                                request_id: req.request_id,
                                                ok: false,
                                                message: "Unauthorized".to_string(),
                                            })
                                            .unwrap_or_else(|_| {
                                                "{\"kind\":\"heartbeat_response\",\"ok\":false,\"message\":\"Unauthorized\"}".to_string()
                                            }),
                                        ))
                                        .await;
                                    continue;
                                }

                                {
                                    let mut s = state.lock().unwrap();
                                    s.client_heartbeat_ts = Some(chrono::Utc::now().timestamp());
                                    s.add_log(
                                        LogLevel::Debug,
                                        "Heartbeat recorded via WebSocket RPC".to_string(),
                                    );
                                }

                                let _ = socket
                                    .send(Message::Text(
                                        serde_json::to_string(&WsHeartbeatResponse {
                                            kind: "heartbeat_response".to_string(),
                                            request_id: req.request_id,
                                            ok: true,
                                            message: "heartbeat recorded".to_string(),
                                        })
                                        .unwrap_or_else(|_| {
                                            "{\"kind\":\"heartbeat_response\",\"ok\":true,\"message\":\"heartbeat recorded\"}".to_string()
                                        }),
                                    ))
                                    .await;
                                continue;
                            }
                        }

                        if let Ok(req) = serde_json::from_str::<WsUpdateRequest>(&text) {
                            if req.kind == "update_request" {
                                if !is_ws_authorized(req.token.as_deref()) {
                                    let _ = socket
                                        .send(Message::Text(
                                            serde_json::to_string(&WsUpdateResponse {
                                                kind: "update_response".to_string(),
                                                request_id: req.request_id,
                                                ok: false,
                                                message: "Unauthorized".to_string(),
                                            })
                                            .unwrap_or_else(|_| {
                                                "{\"kind\":\"update_response\",\"ok\":false,\"message\":\"Unauthorized\"}".to_string()
                                            }),
                                        ))
                                        .await;
                                    continue;
                                }

                                let state_clone = state.clone();
                                tokio::spawn(async move {
                                    let _ = perform_lxc_self_update(state_clone).await;
                                });

                                let _ = socket
                                    .send(Message::Text(
                                        serde_json::to_string(&WsUpdateResponse {
                                            kind: "update_response".to_string(),
                                            request_id: req.request_id,
                                            ok: true,
                                            message: "LXC update check started".to_string(),
                                        })
                                        .unwrap_or_else(|_| {
                                            "{\"kind\":\"update_response\",\"ok\":false,\"message\":\"serialization error\"}".to_string()
                                        }),
                                    ))
                                    .await;
                                continue;
                            }
                        }

                        if let Ok(req) = serde_json::from_str::<WsRestoreRequest>(&text) {
                            if req.kind == "restore_request" {
                                if !is_ws_authorized(req.token.as_deref()) {
                                    let scope = match req.scope.as_str() {
                                        "Environment" => restore::RestoreScope::Environment,
                                        _ => restore::RestoreScope::Stack,
                                    };
                                    let denied_request = RestoreRequest {
                                        scope,
                                        stack_names: req.stack_names,
                                        backup_id: req.backup_id,
                                        verify_only: req.verify_only,
                                        skip_post_hooks: req.skip_post_hooks,
                                    };
                                    let mut denied = RestoreStatus::new(
                                        "restore-unauthorized".to_string(),
                                        &denied_request,
                                    );
                                    denied.error_message = Some("Unauthorized".to_string());

                                    let _ = socket
                                        .send(Message::Text(
                                            serde_json::to_string(&WsRestoreResponse {
                                                kind: "restore_response".to_string(),
                                                request_id: req.request_id,
                                                ok: false,
                                                status: denied,
                                                message: "Unauthorized".to_string(),
                                            })
                                            .unwrap_or_else(|_| {
                                                "{\"kind\":\"restore_response\",\"ok\":false,\"message\":\"Unauthorized\"}".to_string()
                                            }),
                                        ))
                                        .await;
                                    continue;
                                }

                                let scope = match req.scope.as_str() {
                                    "Environment" => restore::RestoreScope::Environment,
                                    _ => restore::RestoreScope::Stack,
                                };
                                let restore_req = RestoreRequest {
                                    scope,
                                    stack_names: req.stack_names,
                                    backup_id: req.backup_id,
                                    verify_only: req.verify_only,
                                    skip_post_hooks: req.skip_post_hooks,
                                };

                                {
                                    let mut s = state.lock().unwrap();
                                    s.add_log(
                                        LogLevel::Info,
                                        format!(
                                            "Restore requested via WebSocket RPC: backup_id={} stacks={}",
                                            restore_req.backup_id,
                                            restore_req.stack_names.join(",")
                                        ),
                                    );
                                }

                                let lxc_api_base = std::env::var("LXC_API_BASE")
                                    .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
                                let backup_root = std::env::var("BACKUP_ROOT")
                                    .unwrap_or_else(|_| "/backups".to_string());
                                let host_appdata_root = std::env::var("HOST_APPDATA_ROOT")
                                    .unwrap_or_else(|_| "/appdata".to_string());

                                let status = restore::execute_restore(
                                    &restore_req,
                                    &lxc_api_base,
                                    &backup_root,
                                    &host_appdata_root,
                                )
                                .await;

                                {
                                    let mut s = state.lock().unwrap();
                                    if status.success {
                                        s.sync_requested = true;
                                        s.add_log(
                                            LogLevel::Ok,
                                            format!(
                                                "Restore completed via WebSocket RPC: operation_id={}",
                                                status.operation_id
                                            ),
                                        );
                                    } else {
                                        s.add_log(
                                            LogLevel::Error,
                                            format!(
                                                "Restore failed via WebSocket RPC: operation_id={} error={}",
                                                status.operation_id,
                                                status
                                                    .error_message
                                                    .clone()
                                                    .unwrap_or_else(|| "unknown".to_string())
                                            ),
                                        );
                                    }
                                }

                                let _ = socket
                                    .send(Message::Text(
                                        serde_json::to_string(&WsRestoreResponse {
                                            kind: "restore_response".to_string(),
                                            request_id: req.request_id,
                                            ok: status.success,
                                            message: if status.success {
                                                "Restore completed".to_string()
                                            } else {
                                                status
                                                    .error_message
                                                    .clone()
                                                    .unwrap_or_else(|| "Restore failed".to_string())
                                            },
                                            status,
                                        })
                                        .unwrap_or_else(|_| {
                                            "{\"kind\":\"restore_response\",\"ok\":false,\"message\":\"serialization error\"}".to_string()
                                        }),
                                    ))
                                    .await;
                                continue;
                            }
                        }

                        if let Ok(req) = serde_json::from_str::<WsExecRequest>(&text) {
                            if req.kind != "exec_request" {
                                continue;
                            }

                            if !is_ws_authorized(req.token.as_deref()) {
                                let _ = socket.send(Message::Text(
                                    serde_json::to_string(&WsExecResponse {
                                        kind: "exec_response".to_string(),
                                        request_id: req.request_id,
                                        exit_code: None,
                                        stdout: None,
                                        stderr: None,
                                        error: Some("Unauthorized".to_string()),
                                    }).unwrap_or_else(|_| "{\"kind\":\"exec_response\",\"error\":\"Unauthorized\"}".to_string())
                                )).await;
                                continue;
                            }

                            let timeout_secs = req.timeout_secs.unwrap_or(30);
                            let result = tokio::time::timeout(
                                std::time::Duration::from_secs(timeout_secs),
                                execute_command(&req.cmd, req.args.unwrap_or_default(), req.stdin),
                            ).await;

                            let response = match result {
                                Ok(Ok(output)) => WsExecResponse {
                                    kind: "exec_response".to_string(),
                                    request_id: req.request_id.clone(),
                                    exit_code: Some(output.exit_code),
                                    stdout: Some(output.stdout),
                                    stderr: Some(output.stderr),
                                    error: None,
                                },
                                Ok(Err(err)) => WsExecResponse {
                                    kind: "exec_response".to_string(),
                                    request_id: req.request_id.clone(),
                                    exit_code: None,
                                    stdout: None,
                                    stderr: None,
                                    error: Some(err.to_string()),
                                },
                                Err(_) => WsExecResponse {
                                    kind: "exec_response".to_string(),
                                    request_id: req.request_id.clone(),
                                    exit_code: None,
                                    stdout: None,
                                    stderr: None,
                                    error: Some(format!("Command timed out after {}s", timeout_secs)),
                                },
                            };

                            if let Ok(payload) = serde_json::to_string(&response) {
                                if socket.send(Message::Text(payload)).await.is_err() {
                                    break;
                                }
                            }

                            let mut s = state.lock().unwrap();
                            s.add_log(
                                LogLevel::Info,
                                format!(
                                    "WebSocket exec handled cmd={} stack={}",
                                    req.cmd,
                                    stack_name
                                ),
                            );
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

fn is_ws_authorized(token: Option<&str>) -> bool {
    let expected = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    if expected.is_empty() {
        return true;
    }
    token.map(|t| t == expected).unwrap_or(false)
}

// ── Sync trigger ───────────────────────────────────────────────────────────

async fn handle_sync(
    _headers: HeaderMap,
    State(state): State<Arc<Mutex<AppState>>>,
) -> (StatusCode, Json<ApiResponse>) {
    let mut s = state.lock().unwrap();
    s.sync_requested = true;
    s.add_log(
        LogLevel::Info,
        "Sync triggered via HTTP Push API".to_string(),
    );
    (
        StatusCode::ACCEPTED,
        Json(ApiResponse {
            status: "accepted".to_string(),
            message: "Sync queued".to_string(),
        }),
    )
}

async fn handle_heartbeat(
    headers: HeaderMap,
    State(state): State<Arc<Mutex<AppState>>>,
) -> (StatusCode, Json<ApiResponse>) {
    if !is_authorized(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse {
                status: "unauthorized".to_string(),
                message: "Unauthorized".to_string(),
            }),
        );
    }

    let mut s = state.lock().unwrap();
    s.client_heartbeat_ts = Some(chrono::Utc::now().timestamp());
    s.add_log(
        LogLevel::Debug,
        "Heartbeat recorded via HTTP API".to_string(),
    );

    (
        StatusCode::OK,
        Json(ApiResponse {
            status: "ok".to_string(),
            message: "heartbeat recorded".to_string(),
        }),
    )
}

async fn handle_update(
    headers: HeaderMap,
    State(state): State<Arc<Mutex<AppState>>>,
) -> (StatusCode, Json<ApiResponse>) {
    if !is_authorized(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse {
                status: "unauthorized".to_string(),
                message: "Unauthorized".to_string(),
            }),
        );
    }

    tokio::spawn(async move {
        let _ = perform_lxc_self_update(state).await;
    });

    (
        StatusCode::ACCEPTED,
        Json(ApiResponse {
            status: "accepted".to_string(),
            message: "LXC update check started".to_string(),
        }),
    )
}

async fn perform_lxc_self_update(state: Arc<Mutex<AppState>>) -> Result<String, String> {
    let update_cmd = std::env::var("LXC_SELF_UPDATE_CMD")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            let image = std::env::var("LXC_DAEMON_IMAGE")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "ghcr.io/kennypassenier/homelab-lxc-daemon:latest".to_string());
            let compose_dir = std::env::var("LXC_DAEMON_COMPOSE_DIR")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "/opt/lxc-daemon".to_string());
            let service = std::env::var("LXC_DAEMON_COMPOSE_SERVICE")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "lxc-daemon".to_string());

            format!(
                "docker pull {image} && cd {compose_dir} && docker compose up -d --force-recreate --no-deps {service}",
            )
        });

    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!("LXC self-update requested via API (cmd={})", update_cmd),
        );
    }

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("sh")
            .args(["-lc", &update_cmd])
            .output()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stdout.is_empty() {
            "LXC update applied (image pull + service recreate)".to_string()
        } else {
            format!("LXC update applied: {}", stdout)
        };
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Ok, message.clone());
        Ok(message)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "LXC self-update command failed".to_string()
        } else {
            format!("LXC self-update failed: {}", stderr)
        };
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Error, message.clone());
        Err(message)
    }
}

// ── Backup pause / resume ──────────────────────────────────────────────────

async fn handle_backup_pause(
    State(state): State<Arc<Mutex<AppState>>>,
) -> (StatusCode, Json<ApiResponse>) {
    let mut s = state.lock().unwrap();
    s.backup_paused = true;
    s.add_log(LogLevel::Info, "Backup pause requested by HOST".to_string());
    (
        StatusCode::OK,
        Json(ApiResponse {
            status: "ok".to_string(),
            message: "Backup paused".to_string(),
        }),
    )
}

async fn handle_backup_resume(
    State(state): State<Arc<Mutex<AppState>>>,
) -> (StatusCode, Json<ApiResponse>) {
    let mut s = state.lock().unwrap();
    s.backup_paused = false;
    s.add_log(LogLevel::Info, "Backup resumed".to_string());
    (
        StatusCode::OK,
        Json(ApiResponse {
            status: "ok".to_string(),
            message: "Backup resumed".to_string(),
        }),
    )
}

// ── Command execution ──────────────────────────────────────────────────────
//
// Execute arbitrary shell commands inside the LXC container.
// Security note: This endpoint should only be available in trusted networks.
// Consider requiring authentication in production.

async fn handle_exec(
    headers: HeaderMap,
    State(state): State<Arc<Mutex<AppState>>>,
    Json(req): Json<ExecRequest>,
) -> (StatusCode, Json<ExecResponse>) {
    if !is_authorized(&headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ExecResponse {
                exit_code: -1,
                stdout: String::new(),
                stderr: "Unauthorized".to_string(),
            }),
        );
    }

    let timeout_secs = req.timeout_secs.unwrap_or(30);
    let args = req.args.unwrap_or_default();

    // Log the command execution request
    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!("Executing command: {} {:?}", req.cmd, args),
        );
    }

    // Run the command with optional stdin and timeout
    match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        execute_command(&req.cmd, args, req.stdin),
    )
    .await
    {
        Ok(Ok(output)) => {
            let status_code = if output.exit_code == 0 {
                StatusCode::OK
            } else {
                StatusCode::BAD_REQUEST
            };

            // Log the command result
            {
                let mut s = state.lock().unwrap();
                s.add_log(
                    if output.exit_code == 0 {
                        LogLevel::Info
                    } else {
                        LogLevel::Error
                    },
                    format!("Command {} exited with code {}", req.cmd, output.exit_code),
                );
            }

            (status_code, Json(output))
        }
        Ok(Err(e)) => {
            {
                let mut s = state.lock().unwrap();
                s.add_log(LogLevel::Error, format!("Command execution error: {}", e));
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ExecResponse {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: format!("Execution error: {}", e),
                }),
            )
        }
        Err(_) => {
            {
                let mut s = state.lock().unwrap();
                s.add_log(
                    LogLevel::Error,
                    format!("Command {} timed out ({}s)", req.cmd, timeout_secs),
                );
            }
            (
                StatusCode::REQUEST_TIMEOUT,
                Json(ExecResponse {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: format!("Command timed out after {}s", timeout_secs),
                }),
            )
        }
    }
}

fn is_authorized(headers: &HeaderMap) -> bool {
    let expected = std::env::var("LXC_API_TOKEN").unwrap_or_default();
    if expected.is_empty() {
        return true;
    }

    let Some(auth_header) = headers.get("authorization") else {
        return false;
    };

    let Ok(auth_value) = auth_header.to_str() else {
        return false;
    };

    let Some(token) = auth_value.strip_prefix("Bearer ") else {
        return false;
    };

    token == expected
}

// Execute a command with optional stdin input
async fn execute_command(
    cmd: &str,
    args: Vec<String>,
    stdin: Option<String>,
) -> Result<ExecResponse, Box<dyn std::error::Error + Send + Sync>> {
    let mut child = tokio::process::Command::new(cmd)
        .args(args)
        .stdin(if stdin.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // Write stdin if provided
    if let Some(stdin_data) = stdin {
        if let Some(mut stdin_handle) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin_handle.write_all(stdin_data.as_bytes()).await?;
        }
    }

    let output = child.wait_with_output().await?;

    Ok(ExecResponse {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

// ── Keyring/Secrets status ─────────────────────────────────────────────────
//
// Provides status about latch CLI, OS keyring, and credential slots for
// latch clone operations and CLIENT monitoring.

async fn handle_keyring_status(
    State(state): State<Arc<Mutex<AppState>>>,
) -> (StatusCode, Json<KeyringStatus>) {
    // Check if latch CLI is available and get version
    let (latch_check, latch_version) =
        match execute_command("latch", vec!["--version".to_string()], None).await {
            Ok(resp) if resp.exit_code == 0 => (true, Some(resp.stdout.trim().to_string())),
            _ => (false, None),
        };

    // Get last latch update check timestamp
    let latch_last_update_secs = std::fs::metadata("/var/lib/homelab/latch-update.last")
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.elapsed().ok())
        .map(|elapsed| elapsed.as_secs());

    // Check if keyring is available (common on Linux)
    let keyring_check = match execute_command(
        "sh",
        vec![
            "-c".to_string(),
            "which pass || which secret-tool || which kwallet-query".to_string(),
        ],
        None,
    )
    .await
    {
        Ok(resp) => resp.exit_code == 0,
        Err(_) => false,
    };

    let env_check = std::env::var("LATCH_PAT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some()
        && std::env::var("LATCH_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .is_some();

    let message = match (latch_check, keyring_check, env_check) {
        (true, true, _) => "Ready for credential sync (keyring backend available)".to_string(),
        (true, false, true) => {
            "Ready for headless latch operation via persistent LATCH_PAT/LATCH_KEY".to_string()
        }
        (true, false, false) => {
            "Latch available but no keyring backend or LATCH_PAT/LATCH_KEY detected".to_string()
        }
        (false, _, _) => "Latch CLI not found; run setup-latch.sh in LXC to install".to_string(),
    };

    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!(
                "Keyring status check: latch={}, keyring={}, env_fallback={}",
                latch_check, keyring_check, env_check
            ),
        );
    }

    // TODO: Query actual keyring slots when latch provides a list command
    // For now, return empty lists to avoid blocking on keyring queries
    let status = KeyringStatus {
        latch_available: latch_check,
        latch_version,
        latch_last_update_secs,
        keyring_available: keyring_check,
        global_slots: vec![],
        project_slots: vec![],
        last_sync: None,
        message,
    };

    (StatusCode::OK, Json(status))
}
