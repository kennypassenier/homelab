//! Shared client liveness registry for HOST failsafe decisions.
//!
//! CLIENT heartbeat pulses update this state via websocket RPC or HTTP API.
//! HOST failsafe checks this state before triggering emergency self-update windows.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LAST_CLIENT_HEARTBEAT_TS: AtomicU64 = AtomicU64::new(0);

/// Records a CLIENT heartbeat and returns the unix timestamp used.
pub fn touch_client_heartbeat() -> u64 {
    let now = now_secs();
    LAST_CLIENT_HEARTBEAT_TS.store(now, Ordering::Relaxed);
    now
}

/// Returns age in seconds of the last observed CLIENT heartbeat.
/// Returns `None` when no heartbeat was observed since HOST start.
pub fn heartbeat_age_secs() -> Option<u64> {
    let ts = LAST_CLIENT_HEARTBEAT_TS.load(Ordering::Relaxed);
    if ts == 0 {
        return None;
    }

    Some(now_secs().saturating_sub(ts))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
