//! redesign-final (B, coordinator 2026-10-04): the dashboard's one clock.
//! A whole-screen run injects one moment (`HOMELAB_ADMIN_DEMO_CLOCK`, unix
//! seconds; scripts/invariants-run.sh, the same as the browser's and the
//! cases' admin/web/test-e2e/clock.js) and a demo-host build counts from it,
//! so its made-up history, its "last night" and every "N s ago" read the
//! same at any time of day. Every other build reads the real clock — here
//! and nowhere else (scripts/check-test-clock.mjs refuses a clock in the
//! demo host or a test).

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// The injected moment and when this process started counting from it.
fn injected() -> Option<&'static (u64, Instant)> {
    static AT: OnceLock<Option<(u64, Instant)>> = OnceLock::new();
    AT.get_or_init(|| {
        if !cfg!(feature = "demo-host") {
            return None;
        }
        std::env::var("HOMELAB_ADMIN_DEMO_CLOCK")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|s| (s, Instant::now()))
    })
    .as_ref()
}

/// Now, unix seconds: the injected clock in a demo run, else the real one.
pub fn now_s() -> u64 {
    match injected() {
        Some((at, since)) => at + since.elapsed().as_secs(),
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    }
}

/// Now, unix milliseconds.
pub fn now_ms() -> u128 {
    match injected() {
        Some((at, since)) => u128::from(*at) * 1000 + since.elapsed().as_millis(),
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    }
}
