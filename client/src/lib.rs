//! homelab-client library: the TUI (AR6) and TLS pinning, shared by the
//! `homelab` binary and the test suite.

pub mod apply;
pub mod edge;
pub mod link;
pub mod release;
pub mod repo_config;
pub mod scaffold;
pub mod spec;
pub mod testplan;
pub mod tls;
pub mod tui;
pub mod updatepolicy;
pub mod version;

use std::path::PathBuf;

/// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): `git
/// describe --dirty` of the tree this client was built from (build.rs), or
/// `unknown` outside a git tree. Printed by `ping` and `status` and sent
/// with every deploy.
pub const BUILD: &str = env!("HOMELAB_BUILD");

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
