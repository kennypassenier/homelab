//! The UI relay commands, which are the session's own business and never
//! queued behind a deploy (feat-platform-10).

use crate::*;

/// Answer one command with a plain response.
async fn reply(out_tx: &tokio::sync::mpsc::Sender<ServerMsg>, id: u64, ok: bool, message: String) {
    let resp = RpcResponse {
        id,
        ok,
        message,
        deferred: None,
    };
    let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
}

/// `true` when `req` was a UI command and has been answered (or handed to a
/// task that will answer it); `false` for every other command.
pub(super) async fn handled_here(
    state: &AppState,
    who: &Identity,
    session: u64,
    out_tx: &tokio::sync::mpsc::Sender<ServerMsg>,
    req: &RpcRequest,
) -> bool {
    match &req.command {
        Rpc::UiAttach => {
            state.ui.attach(session, &who.name, out_tx.clone());
            info!(token = %who.name, session, "the dashboard attached for UI steps");
            reply(
                out_tx,
                req.id,
                true,
                "attached: UI steps come to this session".into(),
            )
            .await;
        }
        Rpc::UiReply { relay, ok, message } => {
            let (ok, message) = match state.ui.reply(session, *relay, *ok, message.clone()) {
                Ok(()) => (true, "delivered".to_string()),
                Err(e) => (false, e),
            };
            reply(out_tx, req.id, ok, message).await;
        }
        Rpc::UiHold {
            relay,
            wait_s,
            note,
        } => {
            let (ok, message) = match state.ui.hold(session, *relay, *wait_s, note.clone()) {
                Ok(()) => (true, "held".to_string()),
                Err(e) => (false, e),
            };
            reply(out_tx, req.id, ok, message).await;
        }
        Rpc::Ui {
            step,
            client_version,
        } => {
            info!(token = %who.name, step = step.verb(), "UI step relayed to the dashboard");
            let (out_tx, ui, step) = (out_tx.clone(), state.ui.clone(), step.clone());
            let (by, scope, id) = (who.name.clone(), who.scope, req.id);
            let client_version = client_version.clone();
            tokio::spawn(async move {
                // Live view: a note from a held step ("paused by the viewer")
                // goes to this CLI alone.
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
                reply(&out_tx, id, ok, message).await;
            });
        }
        _ => return false,
    }
    true
}
