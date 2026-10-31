//! The websocket surface: upgrading a connection, reading and refusing
//! frames, and one session's serve loop. One file per part of a session.

mod forward;
mod frames;
mod refusals;
mod ui_commands;
mod upgrade;
mod worker;

pub(crate) use frames::unknown_field_refusal;
#[cfg(test)]
pub(crate) use frames::unparseable_frame_line;
pub(crate) use upgrade::ws_upgrade;

use crate::*;
use frames::{Frame, read_frame};

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
    let _ = tx
        .send(Message::Text(
            serde_json::to_string(&hello()).unwrap().into(),
        ))
        .await;

    let log_rx = state.log_tx.subscribe();
    let (out_tx, out_rx) = tokio::sync::mpsc::channel::<ServerMsg>(256);
    let forward = forward::spawn_forwarder(tx, log_rx, out_rx, &state);
    let (work_tx, work_rx) = tokio::sync::mpsc::unbounded_channel::<RpcRequest>();
    let worker = worker::spawn_worker(
        work_rx,
        out_tx.clone(),
        state.clone(),
        handler.clone(),
        who.name.clone(),
    );

    let mut reads_beside = false;
    while let Some(frame) = rx.next().await {
        let (value, req) = match read_frame(frame) {
            Frame::Request(value, req) => (value, req),
            Frame::Skip => continue,
            Frame::End => break,
        };
        if refusals::refused_before_queue(&state, &who, &value, &req, &out_tx).await {
            continue;
        }
        if ui_commands::handled_here(&state, &who, session, &out_tx, &req).await {
            continue;
        }
        // arch-host-link: a session that matches replies by id asks for its
        // reads to skip the queue; the CLI and TUI never ask.
        if let homelab_proto::Command::SessionOptions { reads_beside_queue } = req.command {
            reads_beside = reads_beside_queue;
            let message = format!("reads beside the queue: {}", reads_beside);
            let resp = RpcResponse {
                id: req.id,
                ok: true,
                message,
                deferred: None,
            };
            let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
            continue;
        }
        if runs_beside_the_queue(&req.command) || (reads_beside && req.command.is_read_only()) {
            worker::spawn_beside(
                req,
                out_tx.clone(),
                state.clone(),
                handler.clone(),
                who.name.clone(),
            );
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
