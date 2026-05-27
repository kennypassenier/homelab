use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json,
};
use futures::stream;
use serde::Serialize;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use crate::app::{AppState, LogLevel};

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    message: String,
}

pub async fn run_server(state: Arc<Mutex<AppState>>) {
    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, "Axum HTTP server listening on 0.0.0.0:8080".to_string());
    }

    let app = Router::new()
        .route("/api/sync",           post(handle_sync))
        .route("/api/backup/pause",   post(handle_backup_pause))
        .route("/api/backup/resume",  post(handle_backup_resume))
        .route("/api/logs/stream",    get(handle_sse))
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

// ── SSE log stream ─────────────────────────────────────────────────────────

async fn handle_sse(
    State(state): State<Arc<Mutex<AppState>>>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    // Subscribe BEFORE dropping the lock so we don't miss the first messages
    let rx: broadcast::Receiver<String> = state.lock().unwrap().log_tx.subscribe();

    let stream = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(msg) => return Some((Ok(Event::default().data(msg)), rx)),
                // Missed some messages due to slow consumer — skip and continue
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                // Sender dropped (daemon shutting down)
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── Sync trigger ───────────────────────────────────────────────────────────

async fn handle_sync(
    _headers: HeaderMap,
    State(state): State<Arc<Mutex<AppState>>>,
) -> (StatusCode, Json<ApiResponse>) {
    let mut s = state.lock().unwrap();
    s.sync_requested = true;
    s.add_log(LogLevel::Info, "Sync triggered via HTTP Push API".to_string());
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
