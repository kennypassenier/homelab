//! The gateway's routes: files nobody owns, routes nothing answers, and the
//! address a route probe dials.

use super::{Finding, Severity};
use crate::state::HostState;

/// fix-92 / fix-130: every name in the gateway's routes directory that no
/// stack's deploy recorded. Pure so `homelab doctor` (which wants the bare
/// names, not a `Finding`) and the fleet check (which wants one) can share a
/// single judgement.
pub fn unowned_route_files(state: &HostState, on_disk: &[String]) -> Vec<String> {
    let owned: std::collections::BTreeSet<&str> = state
        .stacks
        .values()
        .flat_map(|s| s.route_file.iter().chain(s.extra_route_files.iter()))
        .map(String::as_str)
        .collect();
    on_disk
        .iter()
        .filter(|f| !owned.contains(f.as_str()))
        .cloned()
        .collect()
}

/// fix-92 (routes-outside-repo-unvalidated, 2026-09-27): a file in the
/// gateway's routes directory that no stack's deploy recorded.
///
/// Which hostnames the house publishes is decided by that directory, so a
/// file there that no stack declares is a hostname the repository cannot
/// account for — the four hand-written files fix-91 brought in were exactly
/// that. Judged against what deploys recorded (`route_file`,
/// `extra_route_files`) rather than against the stack files, because the
/// nightly round runs on the host, which has no working copy of them. Every
/// name counts, not only `*.yml`: a `.bak` Traefik ignores today is still a
/// file somebody put there and nobody owns. Drift, not Broken: nothing fails
/// because of it. Homelab never removes such a file itself (fix-41).
pub fn evaluate_route_owners(state: &HostState, on_disk: &[String]) -> Vec<Finding> {
    unowned_route_files(state, on_disk)
        .into_iter()
        .map(|f| Finding {
            severity: Severity::Drift,
            subject: format!("gateway route file {}", f),
            what: "is in the gateway's routes directory but no stack declares it — the \
                   repository cannot say what it publishes"
                .into(),
            remedy: "declare it in the stack it belongs to (gateway_route, or extra_routes to \
                     keep its name) and deploy that stack, or delete it by hand on the gateway; \
                     homelab never removes a route file no deploy wrote"
                .into(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFact {
    pub file: String,
    pub target: String,
    pub answered: bool,
}

/// A route whose target did not answer is a Broken finding.
pub fn dead_route_findings(routes: &[RouteFact]) -> Vec<Finding> {
    routes
        .iter()
        .filter(|r| !r.answered)
        .map(|r| Finding {
            severity: Severity::Broken,
            subject: r.file.clone(),
            what: format!("routes to {}, where nothing answers", r.target),
            remedy: "fix the address or delete the route; a dead route is only found when someone needs it".into(),
        })
        .collect()
}

/// The address a route forwards to, as `host/port` for a `/dev/tcp` probe.
///
/// Extracted from the shell so it can be tested: the first live run reported
/// every route in the house dead, and one of the two reasons was here —
/// `https://10.10.5.1` carries no port, so probing it as written asks for a
/// path rather than a socket. The other reason was the shell itself (dash has
/// no /dev/tcp), which no amount of testing this function would have caught.
/// Both were invisible until the check ran against the real fleet once.
pub fn probe_hostport(target: &str) -> String {
    let (scheme, rest) = match target.split_once("://") {
        Some((s, r)) => (s, r),
        None => ("", target),
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    match authority.rsplit_once(':') {
        // An IPv6 literal has colons of its own; only a numeric tail is a port.
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
            format!("{}/{}", host, port)
        }
        _ => format!("{}/{}", authority, if scheme == "https" { 443 } else { 80 }),
    }
}
