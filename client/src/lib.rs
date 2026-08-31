//! homelab-client library: the TUI (AR6) and TLS pinning, shared by the
//! `homelab` binary and the test suite.

pub mod release;
pub mod scaffold;
pub mod spec;
pub mod tls;
pub mod tui;
pub mod version;

use std::path::PathBuf;

/// Path where the pinned TLS fingerprint is stored (A4, TOFU).
pub fn pin_path() -> PathBuf {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join(".config/homelab/pin")
}

pub fn load_pin() -> Option<String> {
    std::fs::read_to_string(pin_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn save_pin(fp: &str) {
    let path = pin_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, fp);
}

/// CLI exit rule for RPCs whose payload arrives as a separate broadcast
/// frame (GetConfig): RpcDone alone is not enough — the payload frame can
/// lose the race and would be dropped by an early exit. Regression guard
/// for the Config-race bug (fixed 2026-08-11).
pub fn rpc_can_exit(awaits_payload: bool, payload_seen: bool, rpc_done_ok: bool) -> bool {
    rpc_done_ok && (!awaits_payload || payload_seen)
}
