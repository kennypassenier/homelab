//! RPC handlers: views.

use crate::*;

/// `FleetCheck`.
pub(crate) async fn fleet_check(
    state: &AppState,
    req_id: u64,
    stack_files: Vec<(String, u16)>,
    digests: Vec<homelab_core::ops::fleetcheck::StackDigest>,
    json: bool,
    host_config: Option<std::collections::BTreeMap<String, serde_json::Value>>,
) -> RpcResponse {
    let exec = RealExecutor;
    let mut live = gather_live_facts(&exec, state, &stack_files, false).await;
    // fix-142: what the client's files say, for the repository comparison.
    live.digests = digests;
    // fix-110: as above, for config/host.toml against this host's own.
    live.declared_host_config = host_config;
    live.live_host_config = live_host_config_table(
        &std::fs::read_to_string(&state.config.config_path).unwrap_or_default(),
    );
    let snapshot =
        match homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir)
            .load()
            .await
        {
            Ok(s) => s,
            Err(e) => {
                return RpcResponse {
                    id: req_id,
                    ok: false,
                    message: format!("state unreadable: {}", e),
                    deferred: None,
                };
            }
        };
    let findings = homelab_core::ops::fleetcheck::evaluate(
        &snapshot,
        &live,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        state.config.backup_max_age_s,
        homelab_core::ops::fleetcheck::GrowthLimits::default(),
        state.config.tile_watch_source.as_deref(),
        &state.config.trusted_sources,
        state.config.patch_threshold_s,
        state.config.host_meta_max_age_s,
        state.config.capacity_thresholds,
    );
    RpcResponse {
        id: req_id,
        ok: homelab_core::ops::fleetcheck::check_passes(&findings),
        message: if json {
            serde_json::json!({
                "passes": homelab_core::ops::fleetcheck::check_passes(&findings),
                "findings": findings,
            })
            .to_string()
        } else {
            render_findings(&findings)
        },
        deferred: None,
    }
}

/// fix-68: always ok with a JSON body. A part that could not be read
/// travels inside it as `unread`, so the TUI can tell this reply from
/// any other by its shape and never mistakes a failure of it for the
/// end of an operation it has open.
pub(crate) async fn today(
    state: &AppState,
    req_id: u64,
    stack_files: Vec<(String, u16)>,
    digests: Vec<homelab_core::ops::fleetcheck::StackDigest>,
    host_config: Option<std::collections::BTreeMap<String, serde_json::Value>>,
) -> RpcResponse {
    let exec = RealExecutor;
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::to_string(
            &gather_today(&exec, state, &stack_files, digests, host_config).await,
        )
        .unwrap_or_default(),
        deferred: None,
    }
}

/// `Tiles`.
pub(crate) async fn tiles(state: &AppState, req_id: u64, bare: bool) -> RpcResponse {
    let exec = RealExecutor;
    // The plain executor: a reading may read a key on its way.
    let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
    let mut st = store.load().await.unwrap_or_default();
    if bare {
        for s in st.stacks.values_mut() {
            if let Some(m) = s.manifest.as_mut() {
                for t in m.tiles.values_mut() {
                    t.reading = None;
                }
            }
        }
    }
    let tiles = homelab_core::ops::tiles::read_tiles(&RealExecutor, &st).await;
    // Decision "deploys are known outages" (Kenny, 2026-09-30): AR12
    // holds the operation lock strictly one at a time, so at most
    // one stack is ever deploying from the host's own view.
    let deploying_stack = state
        .busy
        .lock()
        .ok()
        .and_then(|b| b.as_ref().and_then(|h| h.stack.clone()));
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::json!({
            "tiles": tiles,
            "deploying_stack": deploying_stack,
            "watch_interval_s": state.config.watch_interval_s,
            "watch_down_after_s": state.config.watch_down_after_s,
        })
        .to_string(),
        deferred: None,
    }
}
