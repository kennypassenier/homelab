//! RPC handlers: snapshots.

use crate::*;

/// feat-backup-1: per-repository status (D25's owning-app repos for
/// a compose stack, one per unit for a native stack), joined with
/// the restore-drill verdict the nightly drill already recorded.
/// fix-180: answered from `state.snapshot_cache`, same owner-keyed
/// cache `BackupCalendar` reads — a repository the two pages both
/// show is read from restic once, not once per page. A repository
/// never read before answers `repo_status_unread` ("not read yet")
/// at once, and a background refresh fills the cache for next time.
pub(crate) async fn get_backups(
    state: &AppState,
    req_id: u64,
    stack: String,
    force: bool,
    source: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
    let snapshot = store.load().await.unwrap_or_default();
    let Some(st) = snapshot.stacks.get(&stack) else {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("no such stack '{}'", stack),
            deferred: None,
        };
    };
    let tiers = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .retention
        .clone();
    let cfg = homelab_core::ops::backup::BackupCfg {
        tiers,
        ..state.config.backup.clone()
    };
    // fix-218 round 2: the same owners doctor and the fleet check
    // read. Taking the manifest's mounts whenever one was recorded
    // dropped a native stack's unit repositories (inbox: manifest
    // with `storage: []` since fix-145 → "no repositories").
    let owners = homelab_core::ops::backup::stack_repo_owners(st);
    let local_available = state.config.backup.local_dataset.is_some();
    // restore-two-sources-1: the local set is read directly — it is
    // a disk away, so it needs none of the remote's cache.
    if source.as_deref() == Some("local") {
        let cfg = match backup_cfg_for_source(&exec, cfg, Some("local")).await {
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
        let mut statuses = Vec::with_capacity(owners.len());
        for owner in owners {
            statuses.push(
                homelab_core::ops::backup::repo_status_of(
                    &exec,
                    &cfg,
                    &snapshot.restore_drills,
                    owner,
                )
                .await,
            );
        }
        return RpcResponse {
            id: req_id,
            ok: true,
            message: serde_json::json!({
                "native": st.is_native(),
                "repos": statuses,
                "source": "local",
                "local_available": local_available,
            })
            .to_string(),
            deferred: None,
        };
    }
    let mut statuses = Vec::with_capacity(owners.len());
    for owner in owners {
        match state.snapshot_cache.get(&owner) {
            Some(cached) => {
                if force {
                    kick_snapshot_refresh(state, cfg.clone(), owner.clone());
                }
                statuses.push(homelab_core::ops::backup::repo_status_from_cache(
                    &cached,
                    &snapshot.restore_drills,
                    owner,
                ));
            }
            None => {
                kick_snapshot_refresh(state, cfg.clone(), owner.clone());
                statuses.push(homelab_core::ops::backup::repo_status_unread(
                    &snapshot.restore_drills,
                    owner,
                ));
            }
        }
    }
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::json!({
            "native": st.is_native(),
            "repos": statuses,
            "source": "remote",
            "local_available": local_available,
        })
        .to_string(),
        deferred: None,
    }
}

/// feat-backup-3: read-only, never stops anything.
pub(crate) async fn browse_snapshot(
    state: &AppState,
    req_id: u64,
    owner: String,
    snapshot: String,
    path: String,
    source: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    let tiers = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .retention
        .clone();
    let base = homelab_core::ops::backup::BackupCfg {
        tiers,
        ..state.config.backup.clone()
    };
    let cfg = match backup_cfg_for_source(&exec, base, source.as_deref()).await {
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
    match homelab_core::ops::backup::browse_snapshot(&exec, &cfg, &owner, &snapshot, &path).await {
        Ok(entries) => RpcResponse {
            id: req_id,
            ok: true,
            message: serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into()),
            deferred: None,
        },
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: e.to_string(),
            deferred: None,
        },
    }
}

/// fix-241: one file from a snapshot, read-only: `restic dump` to
/// stdout, capped; nothing restored, nothing written, nothing stopped.
pub(crate) async fn read_snapshot_file(
    state: &AppState,
    req_id: u64,
    stack: String,
    owner: String,
    snapshot: String,
    path: String,
    source: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
    let target = match store.load().await {
        Ok(hs) => homelab_core::ops::backup::snapshot_file_target(&hs, &stack, &owner, &path),
        Err(e) => Err(format!("state unreadable: {}", e)),
    };
    let path = match target {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse {
                id: req_id,
                ok: false,
                message: e,
                deferred: None,
            };
        }
    };
    let tiers = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .retention
        .clone();
    let base = homelab_core::ops::backup::BackupCfg {
        tiers,
        ..state.config.backup.clone()
    };
    let cfg = match backup_cfg_for_source(&exec, base, source.as_deref()).await {
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
    match homelab_core::ops::backup::read_snapshot_file(&exec, &cfg, &owner, &snapshot, &path).await
    {
        Ok(file) => RpcResponse {
            id: req_id,
            ok: true,
            message: serde_json::to_string(&file).unwrap_or_else(|_| "{}".into()),
            deferred: None,
        },
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: e.to_string(),
            deferred: None,
        },
    }
}

/// fix-237: one snapshot restored into the drill's scratch directory
/// with the drill's own restore-and-judge, counted, and emptied again;
/// the live data is never a target.
pub(crate) async fn verify_restore(
    state: &AppState,
    req_id: u64,
    stack: String,
    app: Option<String>,
    snapshot: String,
    source: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
    let owners = match store.load().await {
        Ok(hs) => homelab_core::ops::restoredrill::verify_restore_owners(
            &hs,
            &stack,
            app.as_deref(),
            &snapshot,
        ),
        Err(e) => Err(format!("state unreadable: {}", e)),
    };
    let owners = match owners {
        Ok(o) => o,
        Err(e) => {
            return RpcResponse {
                id: req_id,
                ok: false,
                message: e,
                deferred: None,
            };
        }
    };
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
    let scratch = state.config.restore_drill_scratch_dir.clone();
    run_mutating_op_for_stack(
        state,
        &exec,
        req_id,
        "verify-restore",
        Some(&stack),
        |ctx| {
            Box::pin(async move {
                homelab_core::ops::restoredrill::verify_restore(
                    ctx, &cfg, &scratch, &owners, &snapshot,
                )
                .await
            })
        },
    )
    .await
}
