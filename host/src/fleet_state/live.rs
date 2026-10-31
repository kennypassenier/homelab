//! What the status loop read about each app, and the refresh of the sealed-env facts.

use crate::*;

/// feat-platform-2: one app's (running, restarts) from a reading. No
/// reading yet, or its guest was not probed: the old answer (running, 0),
/// said as such by `FleetState.status_measured_at` being None. Probed and
/// absent: the app has no container at all, which is not running.
pub(crate) fn live_app(
    reading: Option<&homelab_core::ops::livestatus::LiveStatus>,
    vmid: u16,
    app: &str,
) -> (bool, u32) {
    let Some(apps) = reading.and_then(|r| r.apps.get(&vmid)) else {
        return (true, 0);
    };
    match apps.get(app) {
        Some(a) => (a.running, a.restarts),
        None => (false, 0),
    }
}

/// redesign-stackhub-2: an app's own health check from the newest reading
/// (docker's healthcheck, or `systemctl is-active` for a native unit); None
/// when the app declares none or was not read.
pub(crate) fn live_health(
    reading: Option<&homelab_core::ops::livestatus::LiveStatus>,
    vmid: u16,
    app: &str,
) -> Option<String> {
    reading?.apps.get(&vmid)?.get(app)?.health.clone()
}

/// redesign-stackhub-1: how often the status loop reads every stack's
/// "Env sealed" verdict. It asks each container per secret file, so it runs
/// at a slower cadence than the guest status; a seal-env action reads it
/// again at once.
pub(crate) const ENV_SEALED_INTERVAL_S: u64 = 600;

/// redesign-stackhub-1: read every stack's "Env sealed" verdict now and
/// keep it for the fleet state.
pub(crate) async fn refresh_env_sealed(state: &AppState, exec: &RealExecutor) {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let hs = store.load().await.unwrap_or_default();
    let verdicts =
        homelab_core::ops::facts::sealed_verdicts(exec, &state.config.state_dir, &hs).await;
    if let Ok(mut slot) = state.env_sealed.write() {
        *slot = verdicts;
    }
}
