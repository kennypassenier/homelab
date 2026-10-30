//! RPC handlers: native.

use crate::*;
use homelab_proto::*;

/// T85: a native binary arrives on its own, before the deploy that
/// installs it. Kept under the state directory, root-only, until the
/// deploy consumes it; a name that is not a plain stack or unit name
/// is refused before it can become a path.
pub(crate) async fn stage_native_binary(
    state: &AppState,
    req_id: u64,
    stack: String,
    unit: String,
    binary_b64: String,
) -> RpcResponse {
    let exec = RealExecutor;
    let plain = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    if !plain(&stack) || !plain(&unit) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "refusing to stage '{}' for '{}': stack and unit names are lowercase \
                 [a-z0-9-] and nothing else",
                unit, stack
            ),
            deferred: None,
        };
    }
    let dir = staged_binaries_dir(&state.config.state_dir, &stack);
    let path = format!("{}/{}.b64", dir, unit);
    // fix-51: through the executor's atomic write (own temp name,
    // 0600 from creation, off the async worker) rather than a plain
    // `std::fs::write` of tens of MB that a deploy reading the file at
    // the same moment could see half-written.
    let written = exec.write_file(&path, &binary_b64, 0o600).await;
    match written {
        Ok(()) => RpcResponse {
            id: req_id,
            ok: true,
            message: format!(
                "staged {} for {} ({} KiB of base64) — the next deploy of this stack \
                 installs it",
                unit,
                stack,
                binary_b64.len() / 1024
            ),
            deferred: None,
        },
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: format!("could not stage {} for {}: {}", unit, stack, e),
            deferred: None,
        },
    }
}

/// `AdoptService`.
pub(crate) async fn adopt_service(
    state: &AppState,
    req_id: u64,
    m: Box<NativeServiceManifest>,
) -> RpcResponse {
    let exec = RealExecutor;
    let stack_name = m.stack_name.clone();
    run_mutating_op_for_stack(state, &exec, req_id, "adopt", Some(&stack_name), |ctx| {
        Box::pin(async move { homelab_core::ops::native::adopt(ctx, &m).await })
    })
    .await
}

/// `InstallNative`.
pub(crate) async fn install_native(
    state: &AppState,
    req_id: u64,
    manifest: Box<NativeServiceManifest>,
    binary_b64: String,
    unit_file: String,
) -> RpcResponse {
    let exec = RealExecutor;
    let stack_name = manifest.stack_name.clone();
    run_mutating_op_for_stack(
        state,
        &exec,
        req_id,
        "install-native",
        Some(&stack_name),
        |ctx| {
            Box::pin(async move {
                homelab_core::ops::native::install_native(ctx, &manifest, &binary_b64, &unit_file)
                    .await
            })
        },
    )
    .await
}

/// TUI parity round: the dashboard's install-native; the host fetches
/// and verifies the release itself (CT 120 has no gh).
pub(crate) async fn install_native_release(
    state: &AppState,
    req_id: u64,
    manifest: Box<NativeServiceManifest>,
    unit_file: String,
    tag: String,
) -> RpcResponse {
    let exec = RealExecutor;
    if let Err(problems) = homelab_core::native::validate_native(&manifest) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("service.yml invalid: {}", problems.join("; ")),
            deferred: None,
        };
    }
    if manifest.release_repo.is_none() {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "{} declares no release_repo — there is no release to install; add \
                 release_repo to its service.yml",
                manifest.unit
            ),
            deferred: None,
        };
    }
    let stack_name = manifest.stack_name.clone();
    run_mutating_op_for_stack(
        state,
        &exec,
        req_id,
        "install-native",
        Some(&stack_name),
        |ctx| {
            Box::pin(async move {
                homelab_core::ops::native::install_release(ctx, &manifest, &tag, &unit_file).await
            })
        },
    )
    .await
}

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
