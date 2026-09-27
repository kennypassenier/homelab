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

/// fix-92: what one route file claims and where it sends it, read out of its
/// content by the client. Core parses no YAML itself (a deliberate choice,
/// F200), so the check below judges facts rather than text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RouteFacts {
    pub stack: String,
    pub filename: String,
    /// Every hostname its routers match, lower-cased.
    pub hosts: Vec<String>,
    /// Every backend it forwards to, exactly as the file writes it.
    pub backends: Vec<String>,
    /// As in [`RouteDecl::external`].
    pub external: Vec<String>,
}

/// The host part of a backend: `https://10.10.5.250:8006/x` → `10.10.5.250`.
fn backend_host(target: &str) -> &str {
    let rest = target.split_once("://").map(|(_, r)| r).unwrap_or(target);
    let authority = rest.split('/').next().unwrap_or(rest);
    authority
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(authority)
}

/// fix-92 (routes-outside-repo-unvalidated, 2026-09-27): hold every route
/// file the repository declares against every other, before anything is
/// sent.
///
/// * one stack per file name, or two deploys overwrite each other on the
///   gateway;
/// * one file per hostname — two files claiming one hostname is how F115
///   happened, and which of them Traefik serves is decided nowhere;
/// * every backend is a managed stack's address or declared `external` by
///   its file, so a route to the management network (pve, OPNsense) cannot
///   appear without a line in a stack file saying so;
/// * every `external` entry is a backend the file really has — a stale one
///   would wave through an address nobody meant.
///
/// `stacks` is (stack, address without the prefix length) for every stack
/// in the repository. Returns every problem, not the first.
pub fn fleet_route_problems(stacks: &[(String, String)], routes: &[RouteFacts]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut file_owner: std::collections::BTreeMap<&str, &str> = Default::default();
    let mut host_owner: std::collections::BTreeMap<&str, &str> = Default::default();
    for r in routes {
        match file_owner.get(r.filename.as_str()) {
            Some(other) if *other != r.stack => problems.push(format!(
                "route file {} is declared by both stack {} and stack {} — each deploy would \
                 overwrite the other's; give one of them another name",
                r.filename, other, r.stack
            )),
            _ => {
                file_owner.insert(&r.filename, &r.stack);
            }
        }
        // A file may name its own host in several routers (almanac blocks
        // /metrics with a second one), so repeats inside one file are fine.
        let mut seen_here: Vec<&str> = Vec::new();
        for h in &r.hosts {
            if seen_here.contains(&h.as_str()) {
                continue;
            }
            seen_here.push(h);
            match host_owner.get(h.as_str()) {
                Some(other) => problems.push(format!(
                    "{} is routed by both {} and {} (stack {}) — which one Traefik serves is \
                     not decided anywhere; keep the hostname in one file",
                    h, other, r.filename, r.stack
                )),
                None => {
                    host_owner.insert(h, &r.filename);
                }
            }
        }
        for t in &r.backends {
            let host = backend_host(t);
            let managed = stacks.iter().any(|(_, ip)| ip == host);
            if !managed && !r.external.iter().any(|e| e == t) {
                problems.push(format!(
                    "{} (stack {}) routes to {}, which is no stack's address — point it at the \
                     stack's own container, or declare it under external: in the stack file \
                     if it is meant to leave homelab",
                    r.filename, r.stack, t
                ));
            }
        }
        for e in &r.external {
            if !r.backends.iter().any(|t| t == e) {
                problems.push(format!(
                    "{} (stack {}) declares {} external but does not route there — remove \
                     the stale entry",
                    r.filename, r.stack, e
                ));
            }
        }
    }
    problems
}
