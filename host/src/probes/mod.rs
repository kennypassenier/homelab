//! The probes behind the doctor, one file per kind: the basic reads, the
//! security reads and the context they need from the configuration.

mod basic;
mod context;
mod security;

pub(crate) use basic::gather_probes;
pub(crate) use context::ProbeContext;
pub(crate) use security::gather_security_probes;

#[cfg(test)]
mod disk_tests;
