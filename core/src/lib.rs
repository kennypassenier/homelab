//! homelab-core — all domain logic, zero I/O (AR1).
//!
//! Everything that decides *what happens* lives here: manifest validation
//! (D10), safety gates (A1-A3), the operation pipeline (AR3), state handling
//! (AR4) and the operations themselves. All side effects go through the
//! [`executor::Executor`] trait (AR2), so every path in this crate is fully
//! testable with `executor::MockExecutor` (feature `test-support`) — no
//! Proxmox required.

pub mod ask;
pub mod charts;
pub mod checks;
pub mod compose;
pub mod diskgrowth;
pub mod doctor;
pub mod error;
pub mod executor;
pub mod firewall;
pub mod history;
pub mod hostconfig;
pub mod hostunits;
pub mod incidents;
pub mod logring;
pub mod manifest;
#[cfg(any(test, feature = "test-support"))]
pub mod mock;
pub mod native;
pub mod notify;
pub mod oplock;
pub mod ops;
pub mod release_sig;
pub mod retention;
pub mod routes;
pub mod runner;
pub mod safety;
pub mod sink;
pub mod state;
pub mod wire;
