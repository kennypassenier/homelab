//! feat-backup-1/3, fix-241: the Backups page's read-only routes. Restore itself
//! is `ActionKind::Restore`/`RestoreNative` (`shell::actions`), so it gets
//! the job/progress machinery and Live-view driving every other write
//! action gets; these routes only ever show data.

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

/// fix-180: `?refresh=1` asks the host to read restic again for this
/// stack's repositories in the background, even though this call itself
/// still answers at once from whatever is cached.
#[derive(Deserialize, Default)]
struct StatusQuery {
    #[serde(default, deserialize_with = "crate::shell::queryflag::flag")]
    refresh: bool,
}

async fn status(
    State(host): State<Arc<dyn HostPort>>,
    UrlPath(stack): UrlPath<String>,
    Query(q): Query<StatusQuery>,
) -> Response {
    if !crate::core::actions::valid_stack_name(&stack) {
        return refusal(StatusCode::BAD_REQUEST, "not a stack name");
    }
    match host
        .ask_traced(
            Command::GetBackups {
                stack,
                force: q.refresh,
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

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

/// fix-241: one file of one snapshot, read-only (`restic dump`, capped by
/// the host). `?path=` is relative to the app's backed-up directory or
/// absolute under it; the host resolves it against the stack's manifest.
async fn file(
    State(host): State<Arc<dyn HostPort>>,
    UrlPath((stack, owner, snapshot)): UrlPath<(String, String, String)>,
    Query(q): Query<FileQuery>,
) -> Response {
    if !crate::core::actions::valid_stack_name(&stack) {
        return refusal(StatusCode::BAD_REQUEST, "not a stack name");
    }
    match host
        .ask_traced(
            Command::ReadSnapshotFile {
                stack,
                owner,
                snapshot,
                path: q.path,
            },
            // The chassis request guard cuts every request at 30 s; a dump
            // that needs longer is `homelab snapshot-file`'s, which waits.
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
        .route("/data/backups/{stack}/{owner}/{snapshot}/file", get(file))
        .with_state(host)
}
