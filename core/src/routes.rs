//! Gateway route files as the repository declares them (fix-91, expert panel
//! routes-outside-repo-unvalidated, 2026-09-27).
//!
//! Which hostnames of the house are on the internet is decided by the route
//! files in the gateway's routes directory. Until fix-91 four of them were
//! written by hand on CT 104 and named nowhere in the repository, so the
//! repository could not answer that question. Every route file is now
//! declared by a stack, and this is the shape the client reads them into.

/// One route file a stack declares: its `gateway_route` or one of its
/// `extra_routes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecl {
    /// The stack whose file declares it.
    pub stack: String,
    /// The name the file has in the gateway's routes directory.
    pub filename: String,
    /// The file's content, exactly as it is written to the gateway.
    pub content: String,
    /// Backends this file routes to on purpose that are not a stack homelab
    /// manages (Home Assistant, OPNsense, Proxmox), each exactly as the file
    /// names it. Empty for a route to the stack's own container.
    pub external: Vec<String>,
}
