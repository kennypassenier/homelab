//! Managed tools on the host: read what is installed, compare it with the
//! newest signed release, and install one with a roll-back. A tool is data
//! (`crate::tools::spec::ToolSpec`); nothing here knows any program by name.

pub mod check;
pub mod install;
