//! Gathering backup snapshot facts: who owns which snapshot, the cache of the newest one and the background refresh.

use crate::*;

/// fix-218: the repositories one stack actually owns — a native unit's own
/// unit name (D25), or a compose stack's `owner_groups` (fix-130: a native
/// stack backs up each service whole).
pub(crate) fn stack_snapshot_owners(st: &homelab_core::state::StackState) -> Vec<String> {
    // fix-218 round 2: one shared answer with `Rpc::GetBackups`.
    homelab_core::ops::backup::stack_repo_owners(st)
}

/// fix-218: the newest real snapshot across `owners`, read off the same
/// `SnapshotCache` `homelab snapshots` and the dashboard use — never
/// `StackState::last_backup`, which only records that a backup OPERATION
/// returned ok, even when it archived nothing (an empty native-service list
/// makes the nightly backup loop and the on-demand `backup-native` RPC both
/// vacuously "succeed"). A cache miss never blocks the caller: it kicks a
/// background refresh (the same bounded, coalescing path every other
/// snapshot-cache caller uses) and reports `read = false` instead of
/// guessing either "fresh" or "never backed up".
pub(crate) fn cached_newest_snapshot_unix(
    snapshot_cache: &Arc<homelab_core::ops::snapshot_cache::SnapshotCache>,
    backup_cfg: &homelab_core::ops::backup::BackupCfg,
    owners: &[String],
) -> (Option<u64>, bool) {
    let mut newest: Option<u64> = None;
    let mut read = true;
    for owner in owners {
        match snapshot_cache.get(owner) {
            Some(cached) => {
                if let Some(t) = cached.snapshots.iter().map(|s| s.time).max() {
                    newest = Some(newest.map_or(t, |n: u64| n.max(t)));
                }
            }
            None => {
                read = false;
                let cache = snapshot_cache.clone();
                let cfg = backup_cfg.clone();
                let owner = owner.clone();
                tokio::spawn(async move {
                    cache.refresh(&RealExecutor, &cfg, &owner, unix_now()).await;
                });
            }
        }
    }
    (newest, read)
}

/// fix-218: every recorded stack's real backup freshness, for the fleet
/// check / `homelab today` / the dashboard's Health — the same reading
/// `gather_probes` feeds `homelab doctor`, so the two never disagree about
/// whether a stack has actually been backed up.
pub(crate) async fn gather_backup_snapshot_facts(
    state_dir: &str,
    snapshot_cache: &Arc<homelab_core::ops::snapshot_cache::SnapshotCache>,
    backup_cfg: &homelab_core::ops::backup::BackupCfg,
) -> Vec<homelab_core::ops::fleetcheck::StackSnapshotFact> {
    use homelab_core::ops::fleetcheck::StackSnapshotFact;
    let store = homelab_core::state::StateStore::new(&RealExecutor, state_dir);
    let hs = store.load().await.unwrap_or_default();
    hs.stacks
        .iter()
        .map(|(name, st)| {
            let owners = stack_snapshot_owners(st);
            let (newest_snapshot_unix, snapshot_read) =
                cached_newest_snapshot_unix(snapshot_cache, backup_cfg, &owners);
            StackSnapshotFact {
                name: name.clone(),
                newest_snapshot_unix,
                snapshot_read,
                nothing_to_back_up: !st.is_native()
                    && st.manifest.as_ref().is_some_and(|m| m.backs_up_nothing()),
                is_native: st.is_native(),
                empty_owners: owners
                    .iter()
                    .filter(|o| {
                        snapshot_cache
                            .get(o.as_str())
                            .is_some_and(|c| c.snapshots.is_empty())
                    })
                    .cloned()
                    .collect(),
            }
        })
        .collect()
}

/// fix-180: refresh one repository's snapshot cache in the background — the
/// caller (an RPC handler whose cache missed, or who wants a forced
/// refresh, or the completion of that repository's own backup/prune/
/// restore) never waits for this; `SnapshotCache::refresh`'s own semaphore
/// bounds how many of these actually talk to restic at once, cache-wide, so
/// firing one more of these never by itself adds concurrency.
pub(crate) fn kick_snapshot_refresh(
    state: &AppState,
    cfg: homelab_core::ops::backup::BackupCfg,
    owner: String,
) {
    let cache = state.snapshot_cache.clone();
    tokio::spawn(async move {
        cache.refresh(&RealExecutor, &cfg, &owner, unix_now()).await;
    });
}

/// fix-180: the same, for every repository a stack's manifest (or a native
/// unit list) names — used right after that stack's own backup/restore
/// completes, so the cache the dashboard reads next already has today's
/// snapshot instead of waiting for the next periodic sweep.
pub(crate) fn kick_snapshot_refresh_many(
    state: &AppState,
    cfg: homelab_core::ops::backup::BackupCfg,
    owners: Vec<String>,
) {
    for owner in owners {
        kick_snapshot_refresh(state, cfg.clone(), owner);
    }
}

/// fix-180: every restic repository the fleet currently declares — compose
/// stacks' owning apps (D25) and native units alike, generic over both
/// since neither this nor `SnapshotCache` knows an app's name, only that it
/// owns a repository. Used by the periodic sweep and by
/// `SnapshotCache::forget_except` after a destroy, so a cache entry never
/// outlives the repository it answers for (rule 20).
pub(crate) async fn known_snapshot_owners(state: &AppState) -> std::collections::HashSet<String> {
    let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
    let st = store.load().await.unwrap_or_default();
    let mut owners = std::collections::HashSet::new();
    for entry in st.stacks.values() {
        if let Some(m) = &entry.manifest {
            for (owner, _paths) in homelab_core::ops::backup::owner_groups(m) {
                owners.insert(owner);
            }
        }
        for native in &entry.natives {
            owners.insert(native.unit.clone());
        }
    }
    owners
}

/// fix-180: keeps every repository's cache entry fresh within
/// `SNAPSHOT_SWEEP_INTERVAL_S`, even when nothing backed up, pruned or
/// restored it meanwhile, and prunes entries for a repository no stack
/// names any more (a destroy). Never blocks anything: each tick kicks
/// refreshes through the same bounded, coalescing path an RPC's cache miss
/// uses, and moves on.
pub(crate) async fn snapshot_cache_sweep_loop(state: AppState) {
    let mut tick = tokio::time::interval(Duration::from_secs(SNAPSHOT_SWEEP_INTERVAL_S));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let owners = known_snapshot_owners(&state).await;
        state.snapshot_cache.forget_except(&owners);
        let cfg = state.config.backup.clone();
        for owner in owners {
            // A fresh-enough entry is left alone — `refresh` always
            // overwrites, so re-reading it here would cost a restic call
            // for no new information.
            let stale = state.snapshot_cache.get(&owner).is_none_or(|c| {
                unix_now().saturating_sub(c.measured_at) >= SNAPSHOT_SWEEP_INTERVAL_S
            });
            if stale {
                kick_snapshot_refresh(&state, cfg.clone(), owner);
            }
        }
    }
}
