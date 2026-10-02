//! feat-retired-1: the Retired page's one read-only route. Kenny, 2026-10-02:
//! "is there a way, from the admin dashboard, to delete backups of services
//! we no longer use?" — before this the only reach to `homelab wipe <key>`
//! was the stack's own page (`ActionKind::Wipe` in the "Retire" group), which
//! is gone once the stack itself is destroyed. This route lists every
//! `HostState::retired` entry so the page can show one, with a "Wipe…"
//! button that opens the very same wipe dialog (`POST
//! /data/actions/{key}/wipe`, which already accepts a compound key like
//! `jellyfin/sonarr` — see `core::actions::valid_retired_key`).

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use homelab_proto::Command;

use super::actions::HostPort;

const ASK_TIMEOUT: Duration = Duration::from_secs(30);

fn refusal(status: StatusCode, r: impl ToString) -> Response {
    (status, Json(serde_json::json!({ "error": r.to_string() }))).into_response()
}

async fn list(State(host): State<Arc<dyn HostPort>>) -> Response {
    match host
        .ask_traced(Command::GetRetired, ASK_TIMEOUT, None)
        .await
    {
        Ok(r) if r.ok => match serde_json::from_str::<serde_json::Value>(&r.message) {
            Ok(v) => Json(v).into_response(),
            Err(_) => refusal(StatusCode::BAD_GATEWAY, "the host's answer did not read"),
        },
        Ok(r) => refusal(StatusCode::BAD_GATEWAY, r.message),
        Err(e) => refusal(StatusCode::BAD_GATEWAY, e),
    }
}

pub fn router(host: Arc<dyn HostPort>) -> Router {
    Router::new()
        .route("/data/retired", get(list))
        .with_state(host)
}
