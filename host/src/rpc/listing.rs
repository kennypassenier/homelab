//! RPC handlers: the backup listing of a stack.

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
