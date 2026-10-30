//! Upgrading an HTTP request to a session: the token check and the frame ceiling.

use crate::*;

/// The largest single message the link carries, in bytes. Shared with the
/// client so a payload that cannot arrive is refused before it is built.
pub const MAX_WS_FRAME: usize = 256 * 1024 * 1024;

// Raised from 64 MiB on 2026-09-09 (F303, see REGISTER.md). Headroom, not a
// solution: the honest fix is to send a stack's programs one at a time
// instead of in one message — recorded as T85.

pub(crate) async fn ws_upgrade(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let Some(who) = identify(
        headers.get("authorization").and_then(|v| v.to_str().ok()),
        &state.config.token,
        &state.live_tokens(),
    ) else {
        log_refused(&state, peer, "/api/ws");
        return (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response();
    };
    // H5 self-update ships the whole host binary in one message, and the
    // default frame ceiling is 16 MiB. The binary crossed it between v3.19.0
    // (16 729 464 bytes of base64) and v3.20.0 (16 861 920) — 132 KB over —
    // and the failure said "Connection reset by peer", which points at the
    // network rather than at a limit nobody had ever named. The rollout had
    // to be done by hand to get a host that could accept the next one.
    //
    // 64 MiB is not a considered capacity figure, it is distance: five times
    // the current binary, so the ceiling is not reachable by growth alone.
    // The client refuses to send more than this and says so in words.
    ws.max_frame_size(MAX_WS_FRAME)
        .max_message_size(MAX_WS_FRAME)
        .on_upgrade(move |socket| ws_session(socket, state, who))
        .into_response()
}
