use crate::app::{AppState, LogLevel};
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
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    message: String,
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
