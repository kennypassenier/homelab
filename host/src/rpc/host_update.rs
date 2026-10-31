//! RPC handlers: updating and restarting the host daemon itself.

use crate::*;
use homelab_proto::*;

/// `SelfUpdateHost`.
pub(crate) async fn self_update_host(
    state: &AppState,
    req_id: u64,
    binary_b64: String,
    proof: Option<ReleaseProof>,
) -> RpcResponse {
    let exec = RealExecutor;
    use base64::Engine as _;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(&binary_b64) {
        Ok(b) => b,
        Err(e) => {
            return RpcResponse {
                id: req_id,
                ok: false,
                message: format!("bad binary payload: {}", e),
                deferred: None,
            };
        }
    };
    // redesign-host-4: a release's signature is verified here too
    // and recorded, so the new daemon can report itself a signed
    // release. One that does not verify is refused, never recorded;
    // a binary shipped without one installs as before and reports
    // itself unsigned.
    if let Some(p) = &proof
        && let Err(e) = homelab_core::release_sig::record_proof(
            std::path::Path::new(&state.config.state_dir),
            SELF_ASSET,
            &bytes,
            &p.sums,
            &p.sig,
        )
    {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("refused: the release signature sent with the binary: {e}"),
            deferred: None,
        };
    }
    let cfg = homelab_core::ops::selfupdate::SelfUpdateCfg::default();
    // Stage outside the op so the (large) write is done before the
    // op-lock is taken. Raw bytes, not write_file (which is text).
    if let Err(e) = std::fs::write(&cfg.staged, &bytes).and_then(|()| {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cfg.staged, std::fs::Permissions::from_mode(0o755))
    }) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("stage binary: {}", e),
            deferred: None,
        };
    }
    info!("self-update requested: staged {} bytes", bytes.len());
    run_mutating_op(state, &exec, req_id, "self-update", |ctx| {
        Box::pin(async move { homelab_core::ops::selfupdate::self_update(ctx, &cfg).await })
    })
    .await
}

/// Owner decision 2026-09-30 (item 2): "Save and restart the host".
/// Refused outright rather than queued: waiting behind a running
/// job and then restarting mid-nightly-round is not what "restart
/// the host" should ever mean.
pub(crate) async fn restart_host(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    if let Some(holder) = state.busy.lock().ok().and_then(|b| b.clone()) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "refused: an operation is running ({}); running jobs are refused while a job runs",
                holder.what
            ),
            deferred: None,
        };
    }
    run_mutating_op(state, &exec, req_id, "restart-host", |ctx| {
        Box::pin(async move { homelab_core::ops::restarthost::restart_host(ctx).await })
    })
    .await
}
