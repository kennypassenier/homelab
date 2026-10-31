//! Managed tools: programs homelab installs and keeps current beside the
//! stacks (latch first). Everything about one tool lives in `host.toml`;
//! this module only holds the pure rules, so the host and the dashboard
//! decide the same way.

pub mod policy;
pub mod spec;
pub mod version;
