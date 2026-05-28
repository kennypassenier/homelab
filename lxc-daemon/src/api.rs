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
    // Subscribe before dropping the lock so we don't miss the first messages.
    let mut rx: broadcast::Receiver<String> = { state.lock().unwrap().log_tx.subscribe() };
    drop(state);

    loop {
        tokio::select! {
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
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // ignore ping/pong/binary from client
                }
            }
        }
    }
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
) -> Result<ExecResponse, Box<dyn std::error::Error>> {
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
    // Check if latch CLI is available
    let latch_check = match execute_command("which", vec!["latch".to_string()], None).await {
        Ok(resp) => resp.exit_code == 0,
        Err(_) => false,
    };

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

    let message = match (latch_check, keyring_check) {
        (true, true) => "Ready for credential sync".to_string(),
        (true, false) => {
            "Latch available but keyring not detected; install pass or another keyring".to_string()
        }
        (false, _) => "Latch CLI not found; run setup-latch.sh in LXC to install".to_string(),
    };

    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!(
                "Keyring status check: latch={}, keyring={}",
                latch_check, keyring_check
            ),
        );
    }

    // TODO: Query actual keyring slots when latch provides a list command
    // For now, return empty lists to avoid blocking on keyring queries
    let status = KeyringStatus {
        latch_available: latch_check,
        keyring_available: keyring_check,
        global_slots: vec![],
        project_slots: vec![],
        last_sync: None,
        message,
    };

    (StatusCode::OK, Json(status))
}
