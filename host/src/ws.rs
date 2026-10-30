//! The websocket surface: upgrading a connection, framing and refusing bad frames, the hello and one session's serve loop.

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

/// The journal line for a request frame this end could not parse.
///
/// fix-55 (expert panel, unparseable-frame-logged-whole, 2026-09-27): never
/// the body. A deploy frame carries every `.env` value of its stack and a
/// staging frame tens of MB of base64; both went into one journal line when
/// a version skew made a frame unreadable. serde's own message is left out
/// too, because it quotes the offending value ("invalid type: string
/// \"...\""), which can be a secret. The method, the size and where parsing
/// stopped are enough to tell which client sent what.
pub(crate) fn unparseable_frame_line(e: &serde_json::Error, text: &str) -> String {
    let method = text
        .find("\"cmd\"")
        .map(|at| {
            text[at + 5..]
                .trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '"')
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .take(40)
                .collect::<String>()
        })
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "?".into());
    format!(
        "unparseable request dropped :: cmd={} · {} bytes · {:?} error at line {} column {}",
        method,
        text.len(),
        e.classify(),
        e.line(),
        e.column()
    )
}

/// fix-211 (Kenny, 2026-10-02, decision "Host weigert enkel onbekende
/// velden"): why `req` should be refused before it reaches its handler, if
/// it should — a field `value` (the request exactly as the wire sent it)
/// carries that this build's own struct has no room for
/// (`homelab_core::wire::unknown_fields`), named by its path. `None` for a
/// read-only command (it changes nothing, so an unknown field in it cannot
/// cause the 2026-08-31 kind of harm) or for a request with no such field.
pub(crate) fn unknown_field_refusal(value: &serde_json::Value, req: &RpcRequest) -> Option<String> {
    if req.command.is_read_only() {
        return None;
    }
    let unknown = homelab_core::wire::unknown_fields(value, req);
    if unknown.is_empty() {
        return None;
    }
    Some(format!(
        "refused: this host's build does not know {} — update the host \
         ('homelab release-update') and try again, or drop the field if it \
         was sent by mistake",
        unknown.join(", ")
    ))
}

pub(crate) async fn ws_session(socket: WebSocket, state: AppState, who: Identity) {
    serve_ws(socket, state, who, |st, req| async move {
        handle_rpc(&st, req).await
    })
    .await
}

/// The first frame of every session: which host this is.
pub(crate) fn hello() -> ServerMsg {
    ServerMsg::Hello {
        version: VERSION.into(),
        proto: homelab_proto::PROTO_VERSION,
        build: Some(BUILD.into()),
    }
}

/// One client's session: the Hello, the forwarder that carries broadcasts and
/// answers out, and the loop that reads requests. The request handler is a
/// parameter so a test can drive the real session over a real socket with a
/// handler that asks a question (fix-66).
pub(crate) async fn serve_ws<H, Fut>(socket: WebSocket, state: AppState, who: Identity, handler: H)
where
    H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    let (mut tx, mut rx) = socket.split();
    // feat-platform-10: which session this is, for the UI relay.
    static SESSIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let session = SESSIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let hello = hello();
    let _ = tx
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await;

    let mut log_rx = state.log_tx.subscribe();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ServerMsg>(256);
    let asks = state.pending_asks.clone();
    let forward = tokio::spawn(async move {
        loop {
            tokio::select! {
                received = log_rx.recv() => {
                    let msgs = match received {
                        Ok(msg) => vec![msg],
                        // fix-127 (websocket-edge-cases, 2026-09-27): a client
                        // slower than the host lags the channel. The skipped
                        // messages were dropped in silence, questions
                        // included, so an operation waited on an answer to a
                        // question nobody saw. Say how much was missed, and
                        // send every open question again.
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("a client lagged: {} message(s) to it dropped", n);
                            lag_catch_up(n, &asks)
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    };
                    let mut gone = false;
                    for msg in msgs {
                        if tx.send(Message::Text(serde_json::to_string(&msg).unwrap().into())).await.is_err() {
                            gone = true;
                            break;
                        }
                    }
                    if gone { break; }
                }
                Some(msg) = out_rx.recv() => {
                    if tx.send(Message::Text(serde_json::to_string(&msg).unwrap().into())).await.is_err() { break; }
                }
                else => break,
            }
        }
    });

    // fix-66 (see REGISTER.md): requests are READ continuously and RUN by a
    // worker, one at a time and in arrival order, so an answer sent over the
    // same connection as the deploy that asked for it is never stuck behind
    // it.
    //
    // Everything except an answer keeps its order: the TUI tells replies
    // apart by the order it sent the requests in, not by id.
    let (work_tx, mut work_rx) = tokio::sync::mpsc::unbounded_channel::<RpcRequest>();
    let worker = {
        let out_tx = out_tx.clone();
        let state = state.clone();
        let handler = handler.clone();
        let by = who.name.clone();
        tokio::spawn(async move {
            use tracing::Instrument as _;
            while let Some(req) = work_rx.recv().await {
                let span = rpc_span(&req);
                let resp = REQUESTED_BY
                    .scope(by.clone(), handler(state.clone(), req).instrument(span))
                    .await;
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                // fix-121: an answered request is what accepts an update.
                accept_pending_update(&state.config.state_dir, state.started_at);
            }
        })
    };

    let mut reads_beside = false;
    while let Some(frame) = rx.next().await {
        // fix-127 (websocket-edge-cases, 2026-09-27): only Close and a read
        // error end the session. The loop matched Text alone, so the first
        // Ping (a client or proxy keepalive), Pong or Binary frame ended it
        // without a word. Pings are answered by the WebSocket layer itself.
        let text = match frame {
            Ok(Message::Text(text)) => text,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            Ok(Message::Binary(b)) => {
                tracing::warn!(
                    "a binary frame of {} bytes was ignored — requests are JSON text",
                    b.len()
                );
                continue;
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                info!("session ended on a read error :: {}", e);
                break;
            }
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                // Silence here is why `homelab checks answer` looked like a
                // hang rather than a bug: the request was dropped and the
                // client waited for a reply that was never coming. A frame
                // this end cannot understand is a fault on this end.
                tracing::error!("{}", unparseable_frame_line(&e, &text));
                continue;
            }
        };
        let req = match serde_json::from_value::<RpcRequest>(value.clone()) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("{}", unparseable_frame_line(&e, &text));
                continue;
            }
        };
        // fix-211 (Kenny, 2026-10-02: "Host weigert enkel onbekende velden"
        // — the host refuses a command only for a field it does not know,
        // naming it, never for merely being older than the client): a
        // field this host's own build has no struct field for is dropped
        // by serde without a word, which is how a host one release behind
        // silently ignored `data_mounts` on 2026-08-31 and came up without
        // a container's disks.
        if let Some(why) = unknown_field_refusal(&value, &req) {
            tracing::warn!(
                cmd = req.command.name(),
                "refused an unknown field: {}",
                why
            );
            let resp = RpcResponse {
                id: req.id,
                ok: false,
                message: why,
                deferred: None,
            };
            let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
            continue;
        }
        // arch-tokens: a command above the token's scope is refused here,
        // before it reaches the queue, and the refusal is audited.
        if req.command.scope() > who.scope {
            let audit = format!("{}/audit.log", state.config.state_dir);
            if let Err(e) = append_audit(
                &audit,
                &scope_audit_line(unix_now(), &who, &req.command, true),
            ) {
                tracing::warn!("audit.log :: {}", e);
            }
            tracing::warn!(token = %who.name, cmd = req.command.name(), "refused: outside the token's scope");
            let resp = RpcResponse {
                id: req.id,
                ok: false,
                message: format!(
                    "refused: {} needs scope {:?}; token \"{}\" has scope {:?}",
                    req.command.name(),
                    req.command.scope(),
                    who.name,
                    who.scope
                ),
                deferred: None,
            };
            let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
            continue;
        }
        if req.command.scope() == homelab_proto::Scope::All && who.name != "legacy" {
            let audit = format!("{}/audit.log", state.config.state_dir);
            if let Err(e) = append_audit(
                &audit,
                &scope_audit_line(unix_now(), &who, &req.command, false),
            ) {
                tracing::warn!("audit.log :: {}", e);
            }
        }
        // feat-platform-10: the UI relay is the session's own business, like
        // the session options: never queued behind a deploy.
        match req.command {
            Rpc::UiAttach => {
                state.ui.attach(session, &who.name, out_tx.clone());
                info!(token = %who.name, session, "the dashboard attached for UI steps");
                let resp = RpcResponse {
                    id: req.id,
                    ok: true,
                    message: "attached: UI steps come to this session".into(),
                    deferred: None,
                };
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                continue;
            }
            Rpc::UiReply {
                relay,
                ok,
                ref message,
            } => {
                let (ok, message) = match state.ui.reply(session, relay, ok, message.clone()) {
                    Ok(()) => (true, "delivered".to_string()),
                    Err(e) => (false, e),
                };
                let resp = RpcResponse {
                    id: req.id,
                    ok,
                    message,
                    deferred: None,
                };
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                continue;
            }
            Rpc::UiHold {
                relay,
                wait_s,
                ref note,
            } => {
                let (ok, message) = match state.ui.hold(session, relay, wait_s, note.clone()) {
                    Ok(()) => (true, "held".to_string()),
                    Err(e) => (false, e),
                };
                let resp = RpcResponse {
                    id: req.id,
                    ok,
                    message,
                    deferred: None,
                };
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                continue;
            }
            Rpc::Ui {
                ref step,
                ref client_version,
            } => {
                info!(token = %who.name, step = step.verb(), "UI step relayed to the dashboard");
                let (out_tx, ui, step) = (out_tx.clone(), state.ui.clone(), step.clone());
                let (by, scope, id) = (who.name.clone(), who.scope, req.id);
                let client_version = client_version.clone();
                tokio::spawn(async move {
                    // Live view: a note from a held step ("paused by the
                    // viewer …") goes to this CLI alone.
                    let notes = out_tx.clone();
                    let note = move |note: String| {
                        let _ = notes.try_send(ServerMsg::UiNote { note });
                    };
                    let (ok, message) = ui
                        .relay(
                            &by,
                            scope,
                            step,
                            &client_version,
                            ui_relay::RELAY_WAIT,
                            &note,
                        )
                        .await;
                    let resp = RpcResponse {
                        id,
                        ok,
                        message,
                        deferred: None,
                    };
                    let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                });
                continue;
            }
            _ => {}
        }
        // arch-host-link: a session that matches replies by id asks for its
        // reads to skip the queue; the CLI and TUI never ask.
        if let homelab_proto::Command::SessionOptions { reads_beside_queue } = req.command {
            reads_beside = reads_beside_queue;
            let resp = RpcResponse {
                id: req.id,
                ok: true,
                message: format!("reads beside the queue: {}", reads_beside),
                deferred: None,
            };
            let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
            continue;
        }
        if runs_beside_the_queue(&req.command) || (reads_beside && req.command.is_read_only()) {
            let (out_tx, state, handler) = (out_tx.clone(), state.clone(), handler.clone());
            let by = who.name.clone();
            tokio::spawn(async move {
                use tracing::Instrument as _;
                let span = rpc_span(&req);
                let resp = REQUESTED_BY
                    .scope(by, handler(state.clone(), req).instrument(span))
                    .await;
                let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
                accept_pending_update(&state.config.state_dir, state.started_at);
            });
        } else if work_tx.send(req).is_err() {
            break;
        }
    }
    // The client went away. What it already asked for still runs to the end,
    // as it did when the loop ran requests inline: a deploy is not abandoned
    // halfway because a laptop lid closed.
    drop(work_tx);
    // fix-158: the attached dashboard's end hands the steps back to the
    // most recent earlier dashboard still connected, and says which.
    match state.ui.detach(session) {
        ui_relay::Detached::NotAttached => {}
        ui_relay::Detached::FellBackTo(back, token) => info!(
            session,
            back,
            token = %token,
            "the attached dashboard's session ended; UI steps go to the earlier dashboard session {back} again"
        ),
        ui_relay::Detached::NoneLeft => info!(
            session,
            "the attached dashboard's session ended; no dashboard is attached for UI steps"
        ),
    }
    let _ = worker.await;
    forward.abort();
}
