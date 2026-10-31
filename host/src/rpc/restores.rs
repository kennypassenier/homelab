//! RPC handlers: restoring a compose stack, and (below) a native service.

use crate::*;
use homelab_proto::*;

/// `RestoreStack`. One parameter per field of the wire request, so the
/// dispatcher hands the variant over as it is.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn restore_stack(
    state: &AppState,
    req_id: u64,
    manifest: Box<StackManifest>,
    snapshot: String,
    confirm: Option<String>,
    skip_safety_copy: bool,
    app: Option<String>,
    source: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    // fix-64: no typed name, no restore — whoever sent the request.
    if let Err(e) =
        homelab_core::ops::backup::restore_confirmed(&manifest.stack_name, confirm.as_deref())
    {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: e.to_string(),
            deferred: None,
        };
    }
    // The configured target and timeout, not the compiled defaults
    // (F38, see REGISTER.md); restore-two-sources-1: from the set the
    // operator picked.
    let cfg =
        match backup_cfg_for_source(&exec, state.config.backup.clone(), source.as_deref()).await {
            Ok(c) => c,
            Err(e) => {
                return RpcResponse {
                    id: req_id,
                    ok: false,
                    message: e,
                    deferred: None,
                };
            }
        };
    let stack_name = manifest.stack_name.clone();
    // fix-180: a restore may take a safety-copy backup first (unless
    // `skip_safety_copy`) and always re-tags a `restic forget` pass
    // over the repositories it touches, so the same owners are
    // refreshed on success as on an ordinary backup.
    let owners: Vec<String> = homelab_core::ops::backup::owner_groups(&manifest)
        .into_iter()
        .map(|(owner, _paths)| owner)
        .collect();
    let resp =
        run_mutating_op_for_stack(state, &exec, req_id, "restore", Some(&stack_name), |ctx| {
            Box::pin(async move {
                homelab_core::ops::backup::restore_app(
                    ctx,
                    &manifest,
                    &cfg,
                    &snapshot,
                    !skip_safety_copy,
                    app.as_deref(),
                )
                .await
            })
        })
        .await;
    if resp.ok {
        kick_snapshot_refresh_many(state, state.config.backup.clone(), owners);
    }
    resp
}

/// `RestoreNative`.
pub(crate) async fn restore_native(
    state: &AppState,
    req_id: u64,
    stack: String,
    snapshot: String,
    confirm: Option<String>,
    unit: Option<String>,
    source: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    match native_from_state(&state.config.state_dir, &stack).await {
        Ok((services, _)) => {
            // fix-223: a named unit restricts the restore to that one
            // unit's own repository; `None` (and an older client, which
            // never sends the field) restores every unit of the stack
            // from the same snapshot, exactly as before.
            let services = match &unit {
                Some(u) => {
                    let matched: Vec<_> = services.into_iter().filter(|m| &m.unit == u).collect();
                    if matched.is_empty() {
                        return RpcResponse {
                            id: req_id,
                            ok: false,
                            message: format!("stack '{}' has no unit named '{}'", stack, u),
                            deferred: None,
                        };
                    }
                    matched
                }
                None => services,
            };
            let cfg =
                match backup_cfg_for_source(&exec, state.config.backup.clone(), source.as_deref())
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        return RpcResponse {
                            id: req_id,
                            ok: false,
                            message: e,
                            deferred: None,
                        };
                    }
                };
            let mut resp = RpcResponse {
                id: req_id,
                ok: true,
                message: format!("no services on stack '{}'", stack),
                deferred: None,
            };
            for m in services {
                let cfg = cfg.clone();
                let snap = snapshot.clone();
                let confirm = confirm.clone();
                let owner = m.unit.clone();
                let r = run_mutating_op_for_stack(
                    state,
                    &exec,
                    req_id,
                    "restore-native",
                    Some(&stack),
                    |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::native::restore_native(
                                ctx,
                                &m,
                                &cfg,
                                &snap,
                                confirm.as_deref(),
                            )
                            .await
                        })
                    },
                )
                .await;
                let failed = !r.ok;
                if r.ok {
                    kick_snapshot_refresh(state, state.config.backup.clone(), owner);
                }
                if resp.ok || failed {
                    resp = r;
                }
                if failed {
                    break;
                }
            }
            resp
        }
        Err(msg) => RpcResponse {
            id: req_id,
            ok: false,
            message: msg,
            deferred: None,
        },
    }
}
