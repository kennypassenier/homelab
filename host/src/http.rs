//! The HTTP surface besides the websocket: the router, the version endpoint, the plaintext-bearer route check and accepting a pending self-update.

use crate::*;

/// fix-121 (see REGISTER.md): accept a pending self-update once this daemon
/// has answered an authenticated request.
///
/// Until a request is answered the marker stays armed, so a daemon that dies
/// or crash-loops meanwhile is rolled back by the OnFailure unit as before.
/// Only a marker armed before this daemon started is its own: the daemon that
/// armed it answers the self-update request itself, and that answer must not
/// accept its successor.
pub(crate) fn accept_pending_update(state_dir: &str, started_at: u64) {
    let marker = format!("{}/selfupdate.pending", state_dir);
    let Ok(raw) = std::fs::read_to_string(&marker) else {
        return;
    };
    let armed_at = serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|v| v.get("armed_at").and_then(|a| a.as_u64()))
        .unwrap_or(0);
    if armed_at >= started_at {
        return;
    }
    match std::fs::remove_file(&marker) {
        Ok(()) => info!(
            "self-update accepted — v{} answered an authenticated request",
            VERSION
        ),
        Err(e) => tracing::warn!("self-update: could not clear {} :: {}", marker, e),
    }
}

/// fix-126 (expert panel, unauth-version-plaintext-kyu, 2026-09-27):
/// notification routes that send a bearer token over plain HTTP to another
/// machine, shown as `route_for_log` shows them. The kyu publish token
/// crossed VLAN 10 in clear text, where any container that can ARP-spoof
/// reads it. Moving the route to TLS is a change on the machines, so the
/// daemon says it at start rather than refusing.
pub(crate) fn plaintext_bearer_routes(
    primary: Option<&str>,
    primary_bearer: Option<&str>,
    fallback: Option<&str>,
    fallback_bearer: Option<&str>,
) -> Vec<String> {
    let loopback = |url: &str| {
        let host = homelab_core::notify::route_for_log(url);
        let host = host.trim_start_matches("http://");
        host.starts_with("127.") || host.starts_with("localhost") || host.starts_with("[::1]")
    };
    [(primary, primary_bearer), (fallback, fallback_bearer)]
        .into_iter()
        .filter_map(|(url, bearer)| Some((url?, bearer?)))
        .filter(|(url, _)| url.starts_with("http://") && !loopback(url))
        .map(|(url, _)| homelab_core::notify::route_for_log(url))
        .collect()
}

/// The daemon's routes. One function for `main` and the tests, so a test of
/// the 401 path runs the real one.
pub(crate) fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/version", get(version_endpoint))
        .route("/api/ws", get(ws_upgrade))
        .with_state(state)
}

/// fix-126: the version for a caller holding the token. It was open, and it
/// told any neighbour on the VLAN which release runs.
pub(crate) async fn version_endpoint(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> axum::response::Response {
    // arch-tokens: any known token may read the version, scope read included.
    if identify(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        &state.config.token,
        &state.live_tokens(),
    )
    .is_some()
    {
        VERSION.into_response()
    } else {
        log_refused(&state, peer, "/api/version");
        (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response()
    }
}
