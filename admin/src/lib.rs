//! homelab-admin: the browser front for the homelab HOST daemon.
//!
//! arch-crates: `core` holds everything that can be decided without I/O
//! (view models, plans, chart data) and is tested without a network;
//! `shell` holds the chassis app, the link to the host and, later, git and
//! latch. The binary in `main.rs` only wires the two together.

pub mod core;
pub mod shell;
