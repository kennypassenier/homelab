//! The dashboard's own JSON routes, mounted with `dashboard_routes`, so the
//! chassis login (and its Origin/Sec-Fetch-Site guard) sits in front of
//! every one of them.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use super::host_link::Shared;

/// The page's first paint: the newest snapshot, then SSE carries the rest.
async fn fleet(State(shared): State<Shared>) -> Json<serde_json::Value> {
    let s = shared.read().await;
    Json(serde_json::json!({
        "fleet": s.fleet,
        "host_version": s.host_version,
        "link_error": s.link_error,
    }))
}

pub fn router(shared: Shared) -> Router {
    Router::new()
        .route("/data/fleet", get(fleet))
        .with_state(shared)
}
