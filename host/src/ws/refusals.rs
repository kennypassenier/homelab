//! Refusing a request before it reaches the queue: a field this build does
//! not know, and a command above the token's scope.

use crate::*;

/// The refusal as the answer to the request.
async fn refuse(out_tx: &tokio::sync::mpsc::Sender<ServerMsg>, id: u64, message: String) {
    let resp = RpcResponse {
        id,
        ok: false,
        message,
        deferred: None,
    };
    let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
}

/// One line in audit.log; a failed write is said, never silent.
fn audit(state: &AppState, who: &Identity, req: &RpcRequest, refused: bool) {
    let path = format!("{}/audit.log", state.config.state_dir);
    if let Err(e) = append_audit(
        &path,
        &scope_audit_line(unix_now(), who, &req.command, refused),
    ) {
        tracing::warn!("audit.log :: {}", e);
    }
}

/// `true` when the request was refused and answered here, so the caller
/// stops; `false` when it may go on to the queue. The scope-all audit line
/// for an allowed request is written here too.
pub(super) async fn refused_before_queue(
    state: &AppState,
    who: &Identity,
    value: &serde_json::Value,
    req: &RpcRequest,
    out_tx: &tokio::sync::mpsc::Sender<ServerMsg>,
) -> bool {
    // fix-211 (Kenny, 2026-10-02: "Host weigert enkel onbekende velden"
    // — the host refuses a command only for a field it does not know,
    // naming it, never for merely being older than the client): a
    // field this host's own build has no struct field for is dropped
    // by serde without a word, which is how a host one release behind
    // silently ignored `data_mounts` on 2026-08-31 and came up without
    // a container's disks.
    if let Some(why) = unknown_field_refusal(value, req) {
        tracing::warn!(
            cmd = req.command.name(),
            "refused an unknown field: {}",
            why
        );
        refuse(out_tx, req.id, why).await;
        return true;
    }
    // arch-tokens: a command above the token's scope is refused here,
    // before it reaches the queue, and the refusal is audited.
    if req.command.scope() > who.scope {
        audit(state, who, req, true);
        tracing::warn!(token = %who.name, cmd = req.command.name(), "refused: outside the token's scope");
        refuse(
            out_tx,
            req.id,
            format!(
                "refused: {} needs scope {:?}; token \"{}\" has scope {:?}",
                req.command.name(),
                req.command.scope(),
                who.name,
                who.scope
            ),
        )
        .await;
        return true;
    }
    if req.command.scope() == homelab_proto::Scope::All && who.name != "legacy" {
        audit(state, who, req, false);
    }
    false
}
