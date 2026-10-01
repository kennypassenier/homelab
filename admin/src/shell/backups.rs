//! feat-backup-1/3: the Backups page's two read-only routes. Restore itself
//! is `ActionKind::Restore`/`RestoreNative` (`shell::actions`), so it gets
//! the job/progress machinery and Live-view driving every other write
//! action gets; these two routes only ever show data.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use homelab_proto::Command;

use super::actions::HostPort;

const ASK_TIMEOUT: Duration = Duration::from_secs(30);

fn refusal(status: StatusCode, r: impl ToString) -> Response {
    (status, Json(serde_json::json!({ "error": r.to_string() }))).into_response()
}

async fn status(
    State(host): State<Arc<dyn HostPort>>,
    UrlPath(stack): UrlPath<String>,
) -> Response {
    if !crate::core::actions::valid_stack_name(&stack) {
        return refusal(StatusCode::BAD_REQUEST, "not a stack name");
    }
    match host
        .ask_traced(Command::GetBackups { stack }, ASK_TIMEOUT, None)
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

#[derive(Deserialize)]
struct LsQuery {
    #[serde(default)]
    path: String,
}

/// feat-backup-3: list one snapshot's files under `?path=`, read-only.
async fn ls(
    State(host): State<Arc<dyn HostPort>>,
    UrlPath((owner, snapshot)): UrlPath<(String, String)>,
    Query(q): Query<LsQuery>,
) -> Response {
    match host
        .ask_traced(
            Command::BrowseSnapshot {
                owner,
                snapshot,
                path: q.path,
            },
            ASK_TIMEOUT,
            None,
        )
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
        .route("/data/backups/{stack}", get(status))
        .route("/data/backups/{owner}/{snapshot}/ls", get(ls))
        .with_state(host)
}
