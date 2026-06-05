use axum::{
    Json, Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::app::{App, LogLevel};

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    message: String,
}

/// Runtime metrics for the HOST (Proxmox node).
#[derive(Serialize, Clone, Debug)]
pub struct HostMetrics {
    pub hostname: String,
    pub ip: String,
    pub uptime_secs: u64,
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
        .route("/api/metrics", get(handle_metrics))
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

async fn handle_metrics(State(app): State<Arc<Mutex<App>>>) -> Json<HostMetrics> {
    let a = app.lock().unwrap();
    Json(a.current_metrics.clone())
}

async fn handle_ws(ws: WebSocketUpgrade, State(app): State<Arc<Mutex<App>>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, app))
}

async fn handle_ws_client(mut socket: WebSocket, app: Arc<Mutex<App>>) {
    // Subscribe and snapshot existing logs so reconnecting clients don't miss history.
    let (mut rx, snapshot): (broadcast::Receiver<String>, Vec<String>) = {
        let guard = app.lock().unwrap();
        (guard.log_tx.subscribe(), guard.backup_status.clone())
    };

    for line in snapshot {
        if socket.send(Message::Text(line)).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
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
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {} // ignore ping/pong/binary from client
                }
            }
        }
    }
}
