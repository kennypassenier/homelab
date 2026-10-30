//! RPC handlers: secrets.

use crate::*;
use homelab_proto::*;

/// `SealEnv`.
pub(crate) async fn seal_env(state: &AppState, req_id: u64, stack: String) -> RpcResponse {
    let exec = RealExecutor;
    let report = run_mutating_op(state, &exec, req_id, "seal-env", |ctx| {
        Box::pin(async move { homelab_core::ops::sealenv::seal_env(ctx, &stack).await })
    })
    .await;
    // The fleet's "Env sealed" verdict is read again at once rather
    // than at the next slow tick, so the hub's row goes away.
    refresh_env_sealed(state, &exec).await;
    report
}

/// feat-secrets-1: the value, read straight from the host's own
/// vault (never a process, never traced). Audited before the read,
/// naming what was asked for, never the value.
///
/// redesign-3.71 secrets: and one history line (history.jsonl, what
/// the dashboard's Activity reads) naming who revealed or copied
/// which secret — `audit.by` when the dashboard names the person or
/// Live view, else the session's token name. Never the value.
pub(crate) async fn reveal_secret(
    state: &AppState,
    req_id: u64,
    stack: String,
    secret: SecretRef,
    audit: Option<RevealAudit>,
) -> RpcResponse {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let purpose = audit.as_ref().map(|a| a.purpose).unwrap_or_default();
    let by = audit.map(|a| a.by).or_else(requested_by);
    let line = format!(
        "{} {} stack={} secret={:?} by={}\n",
        ts,
        purpose.label(),
        stack,
        secret,
        by.as_deref().unwrap_or("-")
    );
    if let Err(e) = append_audit(&format!("{}/audit.log", state.config.state_dir), &line) {
        tracing::warn!("audit.log: could not record the reveal :: {}", e);
    }
    let read = crate::secrets::reveal(&state.config.state_dir, &stack, &secret).await;
    record_history(
        state,
        &homelab_core::ops::secrets::reveal_history(
            ts,
            &stack,
            &secret,
            purpose,
            by,
            (req_id != 0).then_some(req_id),
            read.as_ref()
                .err()
                .map(|_| "no sealed copy on the host".to_string()),
        ),
    );
    match read {
        Ok(content) => RpcResponse {
            id: req_id,
            ok: true,
            message: content,
            deferred: None,
        },
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: e,
            deferred: None,
        },
    }
}

/// feat-secrets-2: through latch, exactly the one relative path a
/// deploy would read back — the rest of latch is untouched. Audited
/// before the write, never the value; `latch_put` itself never goes
/// through `Executor`/`Cmd` either (core::ops::secrets).
pub(crate) async fn set_secret(
    state: &AppState,
    req_id: u64,
    stack: String,
    secret: SecretRef,
    content: String,
) -> RpcResponse {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let rel = homelab_core::ops::secrets::latch_rel_path(&stack, &secret);
    let audit = format!(
        "{} set-secret stack={} secret={:?} rel={}\n",
        ts, stack, secret, rel
    );
    if let Err(e) = append_audit(&format!("{}/audit.log", state.config.state_dir), &audit) {
        tracing::warn!("audit.log: could not record the write :: {}", e);
    }
    let env = std::env::var("HOMELAB_LATCH_ENV").unwrap_or_default();
    let project_root = format!("{}/repo", state.config.state_dir);
    let result = tokio::task::spawn_blocking(move || {
        crate::secrets::latch_put(&project_root, &env, &rel, &content)
    })
    .await;
    match result {
        Ok(Ok(())) => RpcResponse {
            id: req_id,
            ok: true,
            message: format!(
                "{} written in latch; the other files latch holds for this stack are \
                 untouched; redeploy the stack for the running container to pick it up",
                homelab_core::ops::secrets::latch_rel_path(&stack, &secret)
            ),
            deferred: None,
        },
        Ok(Err(e)) => RpcResponse {
            id: req_id,
            ok: false,
            message: e,
            deferred: None,
        },
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: format!("internal: {e}"),
            deferred: None,
        },
    }
}
