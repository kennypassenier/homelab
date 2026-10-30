//! RPC handlers: snapshots.

use crate::*;

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
