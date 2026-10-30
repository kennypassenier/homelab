//! RPC handlers: native.

use crate::*;

/// `BackupNative`.
pub(crate) async fn backup_native(state: &AppState, req_id: u64, stack: String) -> RpcResponse {
    let exec = RealExecutor;
    match native_from_state(&state.config.state_dir, &stack).await {
        Ok((services, _)) => {
            let tiers = state
                .settings
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .retention
                .clone();
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers,
                // fix-223: an operator asked for this on demand.
                trigger: homelab_core::ops::backup::BackupTrigger::Manual,
                ..state.config.backup.clone()
            };
            // T5: the stack may hold several services; back up each,
            // and report the first failure rather than the last.
            let mut resp = RpcResponse {
                id: req_id,
                ok: true,
                message: format!("no services on stack '{}'", stack),
                deferred: None,
            };
            for m in services {
                let cfg = cfg.clone();
                // fix-180: the unit name IS the repository's owner
                // for a native service (no `owner_groups` — that is
                // compose-only, D25).
                let owner = m.unit.clone();
                let r = run_mutating_op_for_stack(
                    state,
                    &exec,
                    req_id,
                    "backup-native",
                    Some(&stack),
                    |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::native::backup_native(ctx, &m, &cfg).await
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
            if resp.ok {
                record_backup_time(state, &stack).await;
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

/// `UpdateNative`.
pub(crate) async fn update_native(state: &AppState, req_id: u64, stack: String) -> RpcResponse {
    let exec = RealExecutor;
    match native_from_state(&state.config.state_dir, &stack).await {
        Ok((services, applied_at)) => {
            let stored_at = Some(applied_at);
            let mut resp = RpcResponse {
                id: req_id,
                ok: true,
                message: format!("no services on stack '{}'", stack),
                deferred: None,
            };
            for m in services {
                let r = run_mutating_op_for_stack(
                    state,
                    &exec,
                    req_id,
                    "update-native",
                    Some(&stack),
                    |ctx| {
                        Box::pin(async move {
                            homelab_core::ops::native::update_native(ctx, &m, stored_at).await
                        })
                    },
                )
                .await;
                let failed = !r.ok;
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

/// `RollbackNative`.
pub(crate) async fn rollback_native(
    state: &AppState,
    req_id: u64,
    stack: String,
    unit: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    match native_from_state(&state.config.state_dir, &stack)
        .await
        .and_then(|(services, _)| {
            homelab_core::ops::native::select_unit(&services, unit.as_deref())
        }) {
        Ok(m) => {
            run_mutating_op_for_stack(
                state,
                &exec,
                req_id,
                "rollback-native",
                Some(&stack),
                |ctx| {
                    Box::pin(
                        async move { homelab_core::ops::native::rollback_native(ctx, &m).await },
                    )
                },
            )
            .await
        }
        Err(msg) => RpcResponse {
            id: req_id,
            ok: false,
            message: msg,
            deferred: None,
        },
    }
}

/// `ReleaseUpdateNative`.
pub(crate) async fn release_update_native(
    state: &AppState,
    req_id: u64,
    stack: String,
) -> RpcResponse {
    let exec = RealExecutor;
    match native_from_state(&state.config.state_dir, &stack).await {
        Ok((services, _)) => {
            let mut resp = RpcResponse {
                id: req_id,
                ok: true,
                message: format!("no services on stack '{}'", stack),
                deferred: None,
            };
            for m in services {
                let r = run_mutating_op_for_stack(
                    state,
                    &exec,
                    req_id,
                    "release-update-native",
                    Some(&stack),
                    |ctx| {
                        Box::pin(
                            async move { homelab_core::ops::native::release_update(ctx, &m).await },
                        )
                    },
                )
                .await;
                let failed = !r.ok;
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
