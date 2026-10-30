//! Running a session's requests: the worker that runs them one at a time in
//! arrival order, and the tasks for the ones that run beside it.

use crate::*;
use tracing::Instrument as _;

/// fix-66 (see REGISTER.md): requests are READ continuously and RUN by a
/// worker, one at a time and in arrival order, so an answer sent over the
/// same connection as the deploy that asked for it is never stuck behind it.
/// Everything except an answer keeps its order: the TUI tells replies apart
/// by the order it sent the requests in, not by id.
pub(super) fn spawn_worker<H, Fut>(
    mut work_rx: tokio::sync::mpsc::UnboundedReceiver<RpcRequest>,
    out_tx: tokio::sync::mpsc::Sender<ServerMsg>,
    state: AppState,
    handler: H,
    by: String,
) -> tokio::task::JoinHandle<()>
where
    H: Fn(AppState, RpcRequest) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    tokio::spawn(async move {
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
}

/// One request run on its own task, beside the queue (an answer to a
/// question, a read for a session that asked for it).
pub(super) fn spawn_beside<H, Fut>(
    req: RpcRequest,
    out_tx: tokio::sync::mpsc::Sender<ServerMsg>,
    state: AppState,
    handler: H,
    by: String,
) where
    H: Fn(AppState, RpcRequest) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    tokio::spawn(async move {
        let span = rpc_span(&req);
        let resp = REQUESTED_BY
            .scope(by, handler(state.clone(), req).instrument(span))
            .await;
        let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
        accept_pending_update(&state.config.state_dir, state.started_at);
    });
}
