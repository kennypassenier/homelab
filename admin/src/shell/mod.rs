//! Everything that touches the outside world (arch-crates).

pub mod actions;
pub mod actions_notify;
pub mod actions_state;
pub mod backups;
#[cfg(feature = "demo-host")]
pub mod demo;
pub mod drive;
pub mod edit;
pub mod guard;
pub mod host_link;
pub mod hostlog;
pub mod loki;
pub mod parity;
pub mod prometheus;
pub mod releases;
pub mod routes;
pub mod scheduler;
pub mod secrets;
pub mod slow;
pub mod watch;
pub mod workcopy;
