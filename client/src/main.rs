//! Homelab CLIENT — thin CLI over the CLIENT↔HOST protocol.
//! (The cyberpunk TUI plugs into the same protocol; this is the scriptable
//! interface and the pilot-phase driver.)
//!
//! Usage:
//!   homelab ping
//!   homelab status
//!   homelab deploy stacks/<name>
//!
//! Config via env: HOMELAB_HOST (host:port), HOMELAB_TOKEN.

mod commands;

/// The env file is read before the tokio runtime starts: `set_var` while
/// worker threads may read the environment is a data race (and `unsafe` from
/// edition 2024 on, which is what kept the workspace on 2021).
/// rust-code-hygiene, expert panel 2026-09-27.
fn main() {
    // `explicit_host` is what was typed before the command, read before the
    // env file can add to it (feat-client-1, see `run`).
    let explicit_host = std::env::var("HOMELAB_HOST").ok();
    commands::load_config_env();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("start the tokio runtime")
        .block_on(commands::run(explicit_host));
}
