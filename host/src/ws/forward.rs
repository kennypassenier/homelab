//! The forwarder of one session: the task that carries the host's broadcasts
//! and the session's own answers out to the client.

use crate::*;
use futures_util::stream::SplitSink;

/// Spawn it. A client slower than the host lags the broadcast channel
/// (fix-127): say how much was missed and send every open question again,
/// since an operation waits on an answer to a question nobody saw.
pub(super) fn spawn_forwarder(
    mut tx: SplitSink<WebSocket, Message>,
    mut log_rx: broadcast::Receiver<ServerMsg>,
    mut out_rx: tokio::sync::mpsc::Receiver<ServerMsg>,
    state: &AppState,
) -> tokio::task::JoinHandle<()> {
    let asks = state.pending_asks.clone();
    tokio::spawn(async move {
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
    })
}
