//! RPC handlers: what is applied, the live firewall and the retired stacks.

use crate::*;

/// `GetApplied`.
pub(crate) async fn get_applied(state: &AppState, req_id: u64, stack: String) -> RpcResponse {
    // D6: the applied intent lives in the host repo; secrets never
    // do (A5), so this is safe to return.
    if stack.is_empty()
        || !stack
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: "invalid stack name".into(),
            deferred: None,
        };
    }
    let dir = format!("{}/repo/stacks/{}", state.config.state_dir, stack);
    let mut files: Vec<homelab_proto::FileBlob> = Vec::new();
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<homelab_proto::FileBlob>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(base, &p, out);
                } else if let Ok(content) = std::fs::read_to_string(&p) {
                    out.push(homelab_proto::FileBlob {
                        path: p.strip_prefix(base).unwrap().to_string_lossy().into_owned(),
                        content,
                        mode: None,
                    });
                }
            }
        }
    }
    walk(
        std::path::Path::new(&dir),
        std::path::Path::new(&dir),
        &mut files,
    );
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::to_string(&files).unwrap_or_else(|_| "[]".into()),
        deferred: None,
    }
}

/// fix-207: the live firewall status alone — no pct exec, no
/// pools/owners/routes, no `pct list` — for a dashboard page that
/// only wants to know what pve is actually enforcing right now.
pub(crate) async fn get_firewall_live(
    state: &AppState,
    req_id: u64,
    stack_files: Vec<(String, u16)>,
) -> RpcResponse {
    let exec = RealExecutor;
    let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
    match store.load().await {
        Ok(snapshot) => {
            let facts = homelab_core::ops::facts::gather_firewall_facts(
                &exec,
                &snapshot,
                &state.config.safety.no_touch,
            )
            .await;
            let statuses = homelab_core::ops::fleetcheck::firewall_live_status(
                &snapshot,
                &facts,
                state.config.tile_watch_source.as_deref(),
                &state.config.trusted_sources,
            );
            // Only what the caller named: the host may track stacks
            // the dashboard's working copy does not (yet), and
            // answering about those would be noise it never asked
            // for.
            let wanted: std::collections::HashSet<&str> =
                stack_files.iter().map(|(s, _)| s.as_str()).collect();
            let statuses: std::collections::BTreeMap<_, _> = statuses
                .into_iter()
                .filter(|(stack, _)| wanted.contains(stack.as_str()))
                .collect();
            RpcResponse {
                id: req_id,
                ok: true,
                message: serde_json::json!({ "statuses": statuses }).to_string(),
                deferred: None,
            }
        }
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: format!("state unreadable: {}", e),
            deferred: None,
        },
    }
}

/// feat-retired-1: every retired entry for the dashboard's Retired
/// page, each repository's snapshot count/size/newest filled in from
/// the same snapshot cache `GetBackups` reads (fix-180) — never a
/// fresh restic call, so this never blocks on Google Drive.
pub(crate) async fn get_retired(state: &AppState, req_id: u64) -> RpcResponse {
    let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
    let snapshot = store.load().await.unwrap_or_default();
    let rows = homelab_core::ops::retired::retired_rows(&snapshot, &state.config.state_dir);
    let out: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let repos: Vec<serde_json::Value> = r
                .repos
                .iter()
                .map(|repo| {
                    let cached = state.snapshot_cache.get(repo);
                    serde_json::json!({
                        "name": repo,
                        "in_use": r.in_use.contains(repo),
                        "snapshot_count": cached.as_ref().map(|c| c.snapshots.len()),
                        "size_bytes": cached.as_ref().and_then(|c| c.size_bytes),
                        "newest_snapshot": cached.as_ref().and_then(|c| c.snapshots.first().cloned()),
                        "measured_at": cached.as_ref().map(|c| c.measured_at),
                    })
                })
                .collect();
            serde_json::json!({
                "key": r.key,
                "kind": r.kind,
                "stack": r.stack,
                "name": r.name,
                "vmid": r.vmid,
                "retired_at": r.retired_at,
                "repos": repos,
                "appdata": r.appdata,
                "vault": r.vault,
                "in_use": r.in_use,
                "refused": r.refused,
            })
        })
        .collect();
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::json!({ "retired": out }).to_string(),
        deferred: None,
    }
}
