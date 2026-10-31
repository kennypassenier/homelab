//! RPC handlers: backups.

use crate::*;
use homelab_proto::*;

/// `BackupStack`.
pub(crate) async fn backup_stack(
    state: &AppState,
    req_id: u64,
    manifest: Box<StackManifest>,
) -> RpcResponse {
    let exec = RealExecutor;
    let cfg = homelab_core::ops::backup::BackupCfg {
        tiers: state
            .settings
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .retention
            .clone(),
        // fix-223: an operator asked for this on demand.
        trigger: homelab_core::ops::backup::BackupTrigger::Manual,
        ..state.config.backup.clone()
    };
    let stack = manifest.stack_name.clone();
    // fix-180: the owners the cache refreshes on success — read
    // before `manifest` moves into the closure below.
    let owners: Vec<String> = homelab_core::ops::backup::owner_groups(&manifest)
        .into_iter()
        .map(|(owner, _paths)| owner)
        .collect();
    let resp = run_mutating_op_for_stack(state, &exec, req_id, "backup", Some(&stack), |ctx| {
        Box::pin(async move { homelab_core::ops::backup::backup(ctx, &manifest, &cfg).await })
    })
    .await;
    if resp.ok {
        record_backup_time(state, &stack).await;
        kick_snapshot_refresh_many(state, state.config.backup.clone(), owners);
    }
    resp
}

/// `BackupHostMeta`.
pub(crate) async fn backup_host_meta(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
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
    let resp = run_mutating_op(state, &exec, req_id, "host-meta-backup", |ctx| {
        Box::pin(async move { homelab_core::ops::backup::backup_host_meta(ctx, &cfg).await })
    })
    .await;
    if resp.ok {
        let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        record_state(&store, "last_host_meta", |s| s.last_host_meta = now).await;
    }
    resp
}

/// `ZfsReplicate`.
pub(crate) async fn zfs_replicate(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    let tiers = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .retention
        .clone();
    let jobs = state.config.zfs_jobs.clone();
    let resp = run_mutating_op(state, &exec, req_id, "zfs-replicate", |ctx| {
        Box::pin(async move {
            homelab_core::ops::zfs::replicate(
                ctx,
                &jobs,
                &tiers,
                homelab_core::ops::backup::BackupTrigger::Manual,
            )
            .await
        })
    })
    .await;
    if resp.ok {
        let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        record_state(&store, "last_zfs", |s| s.last_zfs = now).await;
    }
    resp
}

/// `BackupDevices`.
pub(crate) async fn backup_devices(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    let devices = state.config.device_backups.clone();
    if devices.is_empty() {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: "no device_backups configured in host.toml".into(),
            deferred: None,
        };
    }
    let tiers = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .retention
        .clone();
    let mut lines = Vec::new();
    let mut ok = true;
    for dev in devices {
        let cfg = homelab_core::ops::backup::BackupCfg {
            tiers: tiers.clone(),
            ..state.config.backup.clone()
        };
        let name = dev.name.clone();
        let report = run_mutating_op(state, &exec, req_id, "device-backup", |ctx| {
            Box::pin(async move {
                homelab_core::ops::devicebackup::backup_device(ctx, &dev, &cfg).await
            })
        })
        .await;
        ok &= report.ok;
        if report.ok {
            record_device_backup(state, &name).await;
        }
        lines.push(format!(
            "{}: {}",
            name,
            if report.ok { "ok" } else { &report.message }
        ));
    }
    RpcResponse {
        id: req_id,
        ok,
        message: lines.join("\n"),
        deferred: None,
    }
}

/// feat-overview-10 (homelab-admin, 2026-10-01): the dashboard's
/// backup calendar, its own query reading restic directly (not
/// through state.json's cached `last_backup`, which only ever holds
/// the newest night — a calendar needs every one of them).
///
/// fix-180: answered from `state.snapshot_cache`, never restic
/// directly — a whole-fleet read used to run every owner's
/// `restic snapshots` one after another and, live on CT 120
/// (2026-10-02), ran past the dashboard's 180 s budget and answered
/// 502 with nothing to say which stack it was on. A repository never
/// read before answers empty at once ("not read yet") while a
/// background read (bounded, coalesced — see `SnapshotCache`) fills
/// it in for the next call.
pub(crate) async fn backup_calendar(
    state: &AppState,
    req_id: u64,
    stacks: Vec<String>,
    force: bool,
) -> RpcResponse {
    let exec = RealExecutor;
    let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
    let st = store.load().await.unwrap_or_default();
    let cfg = state.config.backup.clone();
    let wanted: Vec<&String> = if stacks.is_empty() {
        st.stacks.keys().collect()
    } else {
        stacks.iter().collect()
    };
    let mut by_stack = serde_json::Map::new();
    let mut measured_at = serde_json::Map::new();
    let mut skipped = Vec::new();
    // fix-202: a name that will NEVER get a restic read (no manifest
    // on record yet, not a known stack, or it backs up nothing by
    // design) used to fall straight through `continue` with no
    // `measured_at` entry at all — indistinguishable, client-side,
    // from an owner whose cache is simply still warming up. The
    // dashboard's per-stack read (fix-177) then retried it for the
    // full `UNREAD_GIVE_UP_MS` (3 min) before giving up with a
    // generic "press Refresh" message, which is exactly the "stuck
    // at 11 of 13 for minutes" symptom Kenny measured live on
    // 2026-10-02. Every such name now gets `measured_at` set at once
    // (so the client never waits on it) plus its own entry here:
    // `no_backup` for "nothing to keep, by design" (shown as such,
    // not as a failure), `reasons` for the rest (a real finding,
    // shown with its own text instead of a generic one).
    let mut no_backup = Vec::new();
    let mut reasons = serde_json::Map::new();
    let now = unix_now();
    for name in wanted {
        let Some(entry) = st.stacks.get(name) else {
            reasons.insert(name.clone(), serde_json::json!("not a known stack"));
            measured_at.insert(name.clone(), serde_json::json!(now));
            skipped.push(format!("{name}: not a known stack"));
            continue;
        };
        let Some(m) = entry.manifest.as_ref() else {
            reasons.insert(name.clone(), serde_json::json!("no manifest on record"));
            measured_at.insert(name.clone(), serde_json::json!(now));
            skipped.push(format!("{name}: no manifest on record"));
            continue;
        };
        if m.backs_up_nothing() {
            no_backup.push(name.clone());
            measured_at.insert(name.clone(), serde_json::json!(now));
            continue;
        }
        let mut nights = Vec::new();
        let mut oldest: Option<u64> = None;
        let mut any_unread = false;
        for (owner, _paths) in homelab_core::ops::backup::owner_groups(m) {
            match state.snapshot_cache.get(&owner) {
                Some(cached) => {
                    nights.extend(cached.snapshots.iter().map(|s| s.time));
                    oldest =
                        Some(oldest.map_or(cached.measured_at, |o: u64| o.min(cached.measured_at)));
                    if force {
                        kick_snapshot_refresh(state, cfg.clone(), owner);
                    }
                }
                None => {
                    any_unread = true;
                    kick_snapshot_refresh(state, cfg.clone(), owner);
                }
            }
        }
        if any_unread {
            skipped.push(format!("{name}: not read yet"));
        }
        by_stack.insert(name.clone(), serde_json::json!(nights));
        measured_at.insert(
            name.clone(),
            oldest.map_or(serde_json::Value::Null, |t| serde_json::json!(t)),
        );
    }
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::json!({
            "stacks": by_stack,
            "measured_at": measured_at,
            "skipped": skipped,
            // fix-202: see above — always present (possibly empty),
            // so the dashboard never has to tell "not answered yet"
            // apart from "will never be answered".
            "no_backup": no_backup,
            "reasons": reasons,
        })
        .to_string(),
        deferred: None,
    }
}
