//! homelab-core — all domain logic, zero I/O (AR1).
//!
//! Everything that decides *what happens* lives here: manifest validation
//! (D10), safety gates (A1-A3), the operation pipeline (AR3), state handling
//! (AR4) and the operations themselves. All side effects go through the
//! [`executor::Executor`] trait (AR2), so every path in this crate is fully
//! testable with [`executor::MockExecutor`] — no Proxmox required.

pub mod error;
pub mod executor;
pub mod manifest;
pub mod ops;
pub mod runner;
pub mod safety;
pub mod sink;
pub mod state;
