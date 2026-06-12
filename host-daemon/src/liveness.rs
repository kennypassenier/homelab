//! Shared client liveness registry for HOST failsafe decisions.
//!
//! CLIENT heartbeat pulses update this state via websocket RPC or HTTP API.
//! HOST failsafe checks this state before triggering emergency self-update windows.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static CLIENT_ACTIVE_STACKS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

static LAST_CLIENT_HEARTBEAT_TS: AtomicU64 = AtomicU64::new(0);

/// Records a CLIENT heartbeat and returns the unix timestamp used.
pub fn touch_client_heartbeat() -> u64 {
    let now = now_secs();
    LAST_CLIENT_HEARTBEAT_TS.store(now, Ordering::Relaxed);
    now
}

/// Updates the most recent CLIENT runtime-active stack list.
pub fn set_client_active_stacks(stacks: &[String]) {
    let lock = CLIENT_ACTIVE_STACKS.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = lock.lock().unwrap();
    guard.clear();
    guard.extend_from_slice(stacks);
}

/// Returns the latest CLIENT runtime-active stacks received via heartbeat.
pub fn client_active_stacks() -> Vec<String> {
    let lock = CLIENT_ACTIVE_STACKS.get_or_init(|| Mutex::new(Vec::new()));
    lock.lock().unwrap().clone()
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

/// True when a CLIENT heartbeat is recent enough to trust runtime activation state.
pub fn heartbeat_is_fresh(ttl_secs: u64) -> bool {
    heartbeat_age_secs().map(|age| age <= ttl_secs).unwrap_or(false)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
