//! feat-overview-7 (topology: which container talks to which) and
//! feat-stacks-9 (dependencies between stacks): one graph, derived from the
//! same firewall matrix feat-firewall-2 already computes
//! (`super::fwmatrix::matrix`), so the topology and the matrix can never
//! disagree about what is declared to reach what.
//!
//! The matrix already does the hard part (first-match evaluation of every
//! declared rule, Proxmox's own semantics, via `homelab_core::firewall`):
//! a cell's `state` is `"some"` when at least one declared flow from `from`
//! to `to` passes both ends' firewalls. A topology edge exists exactly
//! where a cell says `"some"`; its `allowed` list becomes the edge's detail
//! (shown as the hover/label). A stack with no firewall in force at all is
//! `"open"` — every topology reaches it, shown as a dashed edge rather than
//! a concrete declared flow, so an unguarded stack reads differently from
//! a stack whose rules were deliberately written to admit this neighbour.
//!
//! fix-203 (Kenny, 2026-10-02, verbatim: "Die topology van fleet view? hoe
//! moet ik daar dingen onderscheiden? waarom is bv almanac met niks
//! gelinked?"): the firewall-matrix edges alone starve a stack whose
//! firewall is DECLARED but not yet `enabled` (most of the fleet mid
//! rollout, `docs/deployment/REGISTER.md`'s "eleven stacks still without
//! one") of every specific relationship it already wrote down — `in_force`
//! only reads a firewall's rules when it is enforced, so a declared-but-
//! disabled stack falls straight into the generic "open to everyone"
//! branch, and the one real thing Kenny wanted to see (which neighbour that
//! inbound rule actually names) disappears into the crowd. Two more edge
//! kinds fix that without touching the firewall PAGE's strict enforcement
//! semantics (still exactly `super::fwmatrix::matrix`, used as-is):
//! `planned` (a specific ACCEPT rule names this peer, but the firewall
//! carrying it is not enabled yet — a real, written-down relationship nobody
//! is blocking but nobody is enforcing either) and `named` (fix-203's other
//! half: the same rule
//! `core/tests/firewall_declarations_tests.rs::every_flow_the_stack_files_name_passes_the_declared_firewalls`
//! already uses to catch an undeclared live flow — every `10.10.x.y:port`
//! a stack's OWN files mention, outside `routes/`, is a real edge whether
//! or not either end has a firewall object at all; a `routes/*` file is the
//! gateway's own route to that backend, so its edges start at the stack
//! named by the fleet's `gateway_route.gateway_vmid`, generically, never a
//! hand-picked name).
//!
//! Node and edge kinds are the only vocabulary for "what is this, really":
//! `docker` / `native` / `gateway` tell nodes apart (a native stack has
//! `natives`, a gateway is whichever vmid some OTHER stack's own
//! `gateway_route.gateway_vmid` names — both read straight off the
//! manifest, no app ever named in code), `external` is an address named in
//! a stack's own files that belongs to no fleet stack at all (Home
//! Assistant, pve, a house device) — shown as its own node, labelled by the
//! bare address, since nothing here is allowed to know what it is called.
//!
//! No app names: everything here is `stack`, `vmid`, `ip` and the protocol
//! words the firewall module already has.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use serde::Serialize;

use super::fwmatrix::{FleetFirewall, Matrix};
use super::stackedit::StackTexts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Docker,
    Native,
    Gateway,
    /// An address a stack's own files name that is not a fleet stack's own
    /// IP — a house device or an out-of-fleet machine, labelled by its bare
    /// address since nothing here knows (or is allowed to know) its name.
    External,
}
use homelab_core::ops::fleetcheck::FirewallLiveStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Node {
    pub stack: String,
    pub vmid: u16,
    pub ip: String,
    pub kind: NodeKind,
    /// fix-207: whether Proxmox is actually enforcing this stack's firewall
    /// right now — `None` when the dashboard could not ask the host (an
    /// older host, or the link down), so the page can tell "off" from
    /// "unknown" instead of quietly falling back to the repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_enforced: Option<bool>,
    /// fix-207: whether that live state matches what the repository
    /// declares. `None` alongside `live_enforced`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_matches_repo: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// `declared`: a specific rule permits it AND the firewall carrying it
    /// is enforced. `planned`: the same specific rule exists but its
    /// firewall is not enabled yet — written down, not yet enforced.
    /// `open`: `to` has no matching rule (or no firewall at all) and is not
    /// currently enforcing anything, so nothing declared stops `from` (or
    /// anyone else) reaching it. `named`: `from`'s own files name this
    /// address directly, independent of any firewall. `route`: a
    /// `routes/*` file under some stack names this address as the backend
    /// of a gateway route.
    pub kind: &'static str,
    /// What passes ("tcp 8080", "icmp", "everything"), or the file(s) that
    /// named it, for the label.
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Topology {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// One `10.10.x.y:port` a stack's own files name, outside `routes/`
/// (`kind: "named"`) or inside it (`kind: "route"`, the file sitting under
/// `stacks/<any>/routes/`) — gathered once per stack by the caller, which
/// already has every text file in hand reading the manifests
/// (`admin::shell::edit::fleet_firewall`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedFlow {
    /// The stack whose own files named this address (never used for a
    /// `route` flow, whose source is always the gateway — see
    /// [`merge_named_flows`]).
    pub from_stack: String,
    pub to_ip: Ipv4Addr,
    pub port: u16,
    pub file: String,
    pub route: bool,
}

/// The first port of every range in a `dport`-shaped value — same reading
/// as `core/tests/firewall_declarations_tests.rs`'s `sample_ports`/port
/// parsing, kept local since this module has no reason to depend on a test
/// helper.
fn house_addresses(line: &str) -> Vec<(Ipv4Addr, u16)> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(i) = rest.find("10.10.") {
        let tail = &rest[i..];
        let end = tail
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ':'))
            .unwrap_or(tail.len());
        let token = &tail[..end];
        let preceded_by_digit = rest[..i].chars().last().is_some_and(|c| c.is_ascii_digit());
        if let Some((ip, port)) = token.split_once(':')
            && !preceded_by_digit
            && let (Ok(ip), Ok(port)) = (
                ip.parse::<Ipv4Addr>(),
                port.trim_end_matches(':').parse::<u16>(),
            )
            && matches!(ip.octets()[2], 5 | 10)
        {
            out.push((ip, port));
        }
        rest = &tail[end.max(1)..];
    }
    out
}

/// Every `10.10.x.y:port` `stack`'s own files name (comment lines and
/// secrets already excluded by `read_texts`), split into plain mentions
/// (`route: false`) and `routes/*` files (`route: true`) — the two the
/// gateway-attribution rule in [`merge_named_flows`] treats differently.
pub fn addresses_named_in(stack: &str, texts: &StackTexts) -> Vec<NamedFlow> {
    let mut out = Vec::new();
    for (path, text) in texts {
        let route = path.split('/').next() == Some("routes");
        for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
            for (ip, port) in house_addresses(line) {
                out.push(NamedFlow {
                    from_stack: stack.to_string(),
                    to_ip: ip,
                    port,
                    file: format!("{stack}/{path}"),
                    route,
                });
            }
        }
    }
    out
}

/// Build the topology straight from the fleet's firewall declarations.
/// Pure given the matrix is already computed; kept separate from
/// [`from_fleet`] so a caller that already has a `Matrix` (the firewall
/// page) does not pay for computing it twice.
pub fn from_matrix(fleet: &[FleetFirewall], matrix: &Matrix) -> Topology {
    let nodes = nodes_of(fleet);
    let edges = matrix
        .cells
        .iter()
        .filter(|c| c.state != "none")
        .map(|c| Edge {
            from: c.from.clone(),
            to: c.to.clone(),
            kind: if c.state == "open" {
                "open"
            } else {
                "declared"
            },
            detail: c.allowed.clone(),
        })
        .collect();
    Topology { nodes, edges }
}

pub fn from_fleet(fleet: &[FleetFirewall]) -> Topology {
    from_matrix(fleet, &super::fwmatrix::matrix(fleet))
}

/// `fleet`'s nodes, kind classified generically: `native` carries at least
/// one `natives` entry; `gateway` is whichever vmid some OTHER stack's own
/// `gateway_route.gateway_vmid` names (`gateway_vmids`, read by the caller
/// off the raw manifests — this module never parses YAML itself); anything
/// else is `docker`.
fn nodes_of(fleet: &[FleetFirewall]) -> Vec<Node> {
    fleet
        .iter()
        .map(|f| Node {
            stack: f.stack.clone(),
            vmid: f.vmid,
            ip: f.ip.to_string(),
            kind: NodeKind::Docker,
            live_enforced: None,
            live_matches_repo: None,
        })
        .collect()
}

/// fix-203: the firewall matrix PLUS every declared-but-not-enforced rule
/// (`planned`) and every address a stack's own files name (`named`/`route`)
/// — the fleet view's own, richer topology. The firewall page keeps using
/// [`from_fleet`]/[`from_matrix`] untouched: it cares what is actually
/// enforced right now, not what is merely written down.
///
/// `natives`: stacks that declare at least one native unit (`node_kind`
/// becomes `Native`). `gateway_vmids`: every vmid some stack's own
/// `gateway_route.gateway_vmid` names (`node_kind` becomes `Gateway`,
/// overriding `Native` — the gateway itself is a plain docker stack, never
/// a native one, but the override costs nothing to state explicitly).
/// `named`: every [`NamedFlow`] the caller already gathered off the raw
/// stack texts.
pub fn from_fleet_declared(
    fleet: &[FleetFirewall],
    natives: &[String],
    gateway_vmids: &[u16],
    named: &[NamedFlow],
) -> Topology {
    let matrix = super::fwmatrix::matrix(fleet);
    let by_ip: BTreeMap<Ipv4Addr, &FleetFirewall> = fleet.iter().map(|f| (f.ip, f)).collect();
    let by_stack: BTreeMap<&str, &FleetFirewall> =
        fleet.iter().map(|f| (f.stack.as_str(), f)).collect();

    let mut nodes = Vec::new();
    for f in fleet {
        let kind = if gateway_vmids.contains(&f.vmid) {
            NodeKind::Gateway
        } else if natives.contains(&f.stack) {
            NodeKind::Native
        } else {
            NodeKind::Docker
        };
        nodes.push(Node {
            stack: f.stack.clone(),
            vmid: f.vmid,
            ip: f.ip.to_string(),
            kind,
            live_enforced: None,
            live_matches_repo: None,
        });
    }

    let mut edges = Vec::new();
    for cell in &matrix.cells {
        if cell.state == "none" {
            continue;
        }
        // fix-203: `open` only when NOTHING specific names this peer — a
        // specific but not-yet-enforced rule is `planned`, strictly more
        // informative than the blanket "open" every other peer still gets.
        let Some(to) = by_stack.get(cell.to.as_str()) else {
            continue;
        };
        let declared_not_enforced = cell.state == "open"
            && to.firewall.as_ref().is_some_and(|fw| {
                !fw.enabled
                    && by_stack.get(cell.from.as_str()).is_some_and(|from| {
                        fw.rules.iter().any(|r| {
                            r.dir == homelab_core::manifest::FwDir::In
                                && r.action == homelab_core::manifest::FwAction::Accept
                                && r.source
                                    .as_deref()
                                    .is_none_or(|s| covers_simple(s, from.ip))
                        })
                    })
            });
        edges.push(Edge {
            from: cell.from.clone(),
            to: cell.to.clone(),
            kind: if declared_not_enforced {
                "planned"
            } else if cell.state == "open" {
                "open"
            } else {
                "declared"
            },
            detail: cell.allowed.clone(),
        });
    }

    // fix-203: external pseudo-nodes, added once per unknown address.
    let mut external_added: Vec<Ipv4Addr> = Vec::new();
    for n in named {
        let to_stack = match by_ip.get(&n.to_ip) {
            Some(f) => f.stack.clone(),
            None => {
                let label = n.to_ip.to_string();
                if !external_added.contains(&n.to_ip) {
                    external_added.push(n.to_ip);
                    nodes.push(Node {
                        stack: label.clone(),
                        vmid: 0,
                        ip: label.clone(),
                        kind: NodeKind::External,
                        live_enforced: None,
                        live_matches_repo: None,
                    });
                }
                label
            }
        };
        let from_stack = if n.route {
            match gateway_vmids
                .first()
                .and_then(|vmid| fleet.iter().find(|f| f.vmid == *vmid))
            {
                Some(g) => g.stack.clone(),
                None => continue,
            }
        } else {
            n.from_stack.clone()
        };
        if from_stack == to_stack {
            continue;
        }
        let kind = if n.route { "route" } else { "named" };
        let label = format!("{} (tcp {})", n.file, n.port);
        match edges
            .iter_mut()
            .find(|e| e.from == from_stack && e.to == to_stack && e.kind == kind)
        {
            Some(e) => {
                if !e.detail.contains(&label) {
                    e.detail.push(label);
                }
            }
            None => edges.push(Edge {
                from: from_stack,
                to: to_stack,
                kind,
                detail: vec![label],
            }),
        }
    }

    Topology { nodes, edges }
}

/// `covers` without `fwmatrix`'s private visibility — same CIDR check,
/// duplicated rather than exposed since it is three lines and not worth a
/// module boundary change for.
fn covers_simple(spec: &str, ip: Ipv4Addr) -> bool {
    let (net, bits) = match spec.split_once('/') {
        Some((n, b)) => (n, b.parse::<u32>().unwrap_or(32)),
        None => (spec, 32),
    };
    let Ok(net) = net.parse::<Ipv4Addr>() else {
        return false;
    };
    let mask: u32 = if bits == 0 {
        0
    } else {
        u32::MAX << (32 - bits.min(32))
    };
    u32::from(net) & mask == u32::from(ip) & mask
}

/// fix-207 (Kenny: "Firewalls: 5 van de 11 staan aan … waarom zie ik dat
/// dan niet op de topology van fleet view?"): stamp each node with what the
/// host actually enforces, read separately from the working copy's
/// declaration. Pure, so it is testable without a host: a topology built
/// from the repository, held against a map of live answers keyed by stack.
/// A stack missing from `live` (the host did not answer about it) keeps
/// `None` on both fields — unknown, not "off".
pub fn with_live(mut topo: Topology, live: &BTreeMap<String, FirewallLiveStatus>) -> Topology {
    for n in &mut topo.nodes {
        if let Some(s) = live.get(&n.stack) {
            n.live_enforced = Some(s.enforced);
            n.live_matches_repo = Some(s.matches_repo);
        }
    }
    topo
}

/// feat-stacks-9: what each stack reaches (its outbound dependencies) and
/// what reaches it (who depends on it), read off the same edges. Only a
/// declared flow counts: an open edge means nothing stops the traffic, not
/// that anything relies on it, and counting it would make every stack
/// depend on every unguarded one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyRow {
    pub stack: String,
    /// Other stacks this one is declared to reach.
    pub depends_on: Vec<String>,
    /// Other stacks declared to reach this one.
    pub depended_on_by: Vec<String>,
}

/// fix-203: an "open" edge means nothing stops the traffic, never that
/// anything relies on it — counting it would make every stack depend on
/// every unguarded one. Every other kind (`declared`, `planned`, `named`,
/// `route`) names a real, specific relationship and counts.
fn is_dependency_kind(kind: &str) -> bool {
    kind != "open"
}

pub fn dependencies(topo: &Topology) -> Vec<DependencyRow> {
    topo.nodes
        .iter()
        .filter(|n| n.kind != NodeKind::External)
        .map(|n| {
            let mut depends_on: Vec<String> = topo
                .edges
                .iter()
                .filter(|e| is_dependency_kind(e.kind) && e.from == n.stack)
                .map(|e| e.to.clone())
                .collect();
            let mut depended_on_by: Vec<String> = topo
                .edges
                .iter()
                .filter(|e| is_dependency_kind(e.kind) && e.to == n.stack)
                .map(|e| e.from.clone())
                .collect();
            depends_on.sort();
            depends_on.dedup();
            depended_on_by.sort();
            depended_on_by.dedup();
            DependencyRow {
                stack: n.stack.clone(),
                depends_on,
                depended_on_by,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use homelab_core::manifest::{FirewallRule, FirewallSpec, FwAction, FwDir, FwProto};

    fn fw_in_from(src: &str, port: u16) -> FirewallSpec {
        FirewallSpec {
            enabled: true,
            comment: None,
            policy_in: FwAction::Drop,
            policy_out: FwAction::Accept,
            management_open: None,
            rules: vec![FirewallRule {
                disabled: false,
                dir: FwDir::In,
                action: FwAction::Accept,
                source: Some(src.into()),
                dest: None,
                proto: Some(FwProto::Tcp),
                dport: Some(port.to_string()),
                comment: None,
                note: None,
            }],
        }
    }

    fn stack(name: &str, vmid: u16, ip: &str, fw: Option<FirewallSpec>) -> FleetFirewall {
        FleetFirewall {
            stack: name.into(),
            vmid,
            ip: ip.parse().unwrap(),
            firewall: fw,
        }
    }

    #[test]
    fn declared_flow_becomes_a_declared_edge_with_its_ports() {
        let fleet = vec![
            stack("gateway", 104, "10.10.10.4", None),
            stack(
                "app",
                110,
                "10.10.10.10",
                Some(fw_in_from("10.10.10.4", 8080)),
            ),
        ];
        let topo = from_fleet(&fleet);
        assert_eq!(topo.nodes.len(), 2);
        let edge = topo
            .edges
            .iter()
            .find(|e| e.from == "gateway" && e.to == "app")
            .expect("gateway -> app edge");
        assert_eq!(edge.kind, "declared");
        assert!(edge.detail.iter().any(|d| d.contains("8080")));
    }

    #[test]
    fn a_stack_with_no_firewall_in_force_is_an_open_edge() {
        let fleet = vec![
            stack("gateway", 104, "10.10.10.4", None),
            stack("open-app", 111, "10.10.10.11", None),
        ];
        let topo = from_fleet(&fleet);
        let edge = topo
            .edges
            .iter()
            .find(|e| e.from == "gateway" && e.to == "open-app")
            .expect("edge exists");
        assert_eq!(edge.kind, "open");
    }

    #[test]
    fn a_guarded_stack_with_no_matching_rule_gets_no_edge() {
        let fleet = vec![
            stack("stranger", 199, "10.10.10.199", None),
            stack(
                "app",
                110,
                "10.10.10.10",
                Some(fw_in_from("10.10.10.4", 8080)),
            ),
        ];
        let topo = from_fleet(&fleet);
        assert!(
            !topo
                .edges
                .iter()
                .any(|e| e.from == "stranger" && e.to == "app")
        );
    }

    #[test]
    fn with_live_stamps_only_the_stacks_the_host_answered_about() {
        let fleet = vec![
            stack("gateway", 104, "10.10.10.4", None),
            stack(
                "app",
                110,
                "10.10.10.10",
                Some(fw_in_from("10.10.10.4", 8080)),
            ),
        ];
        let topo = from_fleet(&fleet);
        let mut live = std::collections::BTreeMap::new();
        live.insert(
            "app".to_string(),
            FirewallLiveStatus {
                enforced: true,
                matches_repo: false,
            },
        );
        let topo = with_live(topo, &live);
        let app = topo.nodes.iter().find(|n| n.stack == "app").unwrap();
        assert_eq!(app.live_enforced, Some(true));
        assert_eq!(app.live_matches_repo, Some(false));
        // The host said nothing about "gateway": unknown, not "off".
        let gateway = topo.nodes.iter().find(|n| n.stack == "gateway").unwrap();
        assert_eq!(gateway.live_enforced, None);
        assert_eq!(gateway.live_matches_repo, None);
    }

    #[test]
    fn dependencies_are_read_directly_off_the_edges() {
        let fleet = vec![
            stack("gateway", 104, "10.10.10.4", None),
            stack(
                "app",
                110,
                "10.10.10.10",
                Some(fw_in_from("10.10.10.4", 8080)),
            ),
            stack(
                "db",
                111,
                "10.10.10.11",
                Some(fw_in_from("10.10.10.10", 5432)),
            ),
        ];
        let topo = from_fleet(&fleet);
        let deps = dependencies(&topo);
        let app = deps.iter().find(|d| d.stack == "app").unwrap();
        assert!(app.depends_on.contains(&"db".to_string()));
        assert!(app.depended_on_by.contains(&"gateway".to_string()));
        let db = deps.iter().find(|d| d.stack == "db").unwrap();
        assert!(db.depended_on_by.contains(&"app".to_string()));
        assert!(db.depends_on.is_empty());
    }

    // ── fix-203: the fleet view's richer topology ───────────────────────

    fn disabled_fw_in_from(src: &str, port: u16) -> FirewallSpec {
        FirewallSpec {
            enabled: false,
            ..fw_in_from(src, port)
        }
    }

    fn named(from_stack: &str, to_ip: &str, port: u16, route: bool) -> NamedFlow {
        NamedFlow {
            from_stack: from_stack.into(),
            to_ip: to_ip.parse().unwrap(),
            port,
            file: format!("{from_stack}/lxc-compose.yml"),
            route,
        }
    }

    #[test]
    fn a_declared_but_disabled_firewall_is_planned_not_a_blank_open() {
        // fix-203 (Kenny, verbatim): "waarom is bv almanac met niks
        // gelinked?" — almanac's own firewall names kp-soft's address, but
        // is not enabled yet (the fleet's phased rollout). The matrix alone
        // (`from_fleet`) hides that behind a blanket "open"; the fleet
        // view's own builder must not.
        let fleet = vec![
            stack("kp-soft", 116, "10.10.10.16", None),
            stack(
                "almanac",
                112,
                "10.10.10.12",
                Some(disabled_fw_in_from("10.10.10.16", 8080)),
            ),
        ];
        let topo = from_fleet_declared(&fleet, &[], &[], &[]);
        let edge = topo
            .edges
            .iter()
            .find(|e| e.from == "kp-soft" && e.to == "almanac")
            .expect("kp-soft -> almanac edge");
        assert_eq!(edge.kind, "planned");
    }

    #[test]
    fn a_peer_the_disabled_firewall_never_named_is_still_open() {
        let fleet = vec![
            stack("kp-soft", 116, "10.10.10.16", None),
            stack("stranger", 199, "10.10.10.199", None),
            stack(
                "almanac",
                112,
                "10.10.10.12",
                Some(disabled_fw_in_from("10.10.10.16", 8080)),
            ),
        ];
        let topo = from_fleet_declared(&fleet, &[], &[], &[]);
        let edge = topo
            .edges
            .iter()
            .find(|e| e.from == "stranger" && e.to == "almanac")
            .expect("stranger -> almanac edge");
        assert_eq!(edge.kind, "open");
    }

    #[test]
    fn an_address_a_stacks_own_files_name_is_a_named_edge_even_unfirewalled() {
        let fleet = vec![
            stack("kp-soft", 116, "10.10.10.16", None),
            stack("almanac", 112, "10.10.10.12", None),
        ];
        let flows = vec![named("kp-soft", "10.10.10.12", 8080, false)];
        let topo = from_fleet_declared(&fleet, &[], &[], &flows);
        let edge = topo
            .edges
            .iter()
            .find(|e| e.from == "kp-soft" && e.to == "almanac" && e.kind == "named")
            .expect("named kp-soft -> almanac edge");
        assert!(edge.detail[0].contains("8080"));
    }

    #[test]
    fn an_address_named_outside_the_fleet_becomes_an_external_node() {
        let fleet = vec![stack("kp-soft", 116, "10.10.10.16", None)];
        let flows = vec![named("kp-soft", "10.10.5.1", 8123, false)];
        let topo = from_fleet_declared(&fleet, &[], &[], &flows);
        let ext = topo
            .nodes
            .iter()
            .find(|n| n.stack == "10.10.5.1")
            .expect("an external node for the unknown address");
        assert_eq!(ext.kind, NodeKind::External);
        assert!(
            topo.edges
                .iter()
                .any(|e| e.from == "kp-soft" && e.to == "10.10.5.1" && e.kind == "named")
        );
    }

    #[test]
    fn a_routes_file_is_a_flow_from_the_gateway_not_from_the_directory_it_sits_in() {
        let fleet = vec![
            stack("gateway", 104, "10.10.10.4", None),
            stack("kyu", 109, "10.10.10.9", None),
        ];
        let flows = vec![named("kyu", "10.10.10.9", 8080, true)];
        let topo = from_fleet_declared(&fleet, &[], &[104], &flows);
        // kyu names itself in its own routes/ file; the flow is from the
        // gateway (vmid 104) to kyu, not kyu to itself.
        assert!(
            topo.edges
                .iter()
                .any(|e| e.from == "gateway" && e.to == "kyu" && e.kind == "route")
        );
        assert!(!topo.edges.iter().any(|e| e.from == "kyu" && e.to == "kyu"));
    }

    #[test]
    fn node_kind_is_read_generically_off_natives_and_gateway_vmids() {
        let fleet = vec![
            stack("gateway", 104, "10.10.10.4", None),
            stack("almanac", 112, "10.10.10.12", None),
            stack("kp-soft", 116, "10.10.10.16", None),
        ];
        let topo = from_fleet_declared(&fleet, &["almanac".to_string()], &[104], &[]);
        let kind_of = |s: &str| topo.nodes.iter().find(|n| n.stack == s).unwrap().kind;
        assert_eq!(kind_of("gateway"), NodeKind::Gateway);
        assert_eq!(kind_of("almanac"), NodeKind::Native);
        assert_eq!(kind_of("kp-soft"), NodeKind::Docker);
    }

    #[test]
    fn house_addresses_reads_named_ips_and_ignores_version_numbers() {
        assert_eq!(
            house_addresses("    - url: \"http://10.10.10.9:8080\""),
            vec![("10.10.10.9".parse().unwrap(), 8080)]
        );
        // Not preceded by a digit boundary and not a house network: ignored.
        assert_eq!(
            house_addresses("version: 2.10.10.9:8080 (not a flow)"),
            vec![]
        );
        assert_eq!(house_addresses("no address here at all"), vec![]);
    }

    #[test]
    fn addresses_named_in_splits_routes_from_plain_mentions() {
        let mut texts = StackTexts::new();
        texts.insert(
            "lxc-compose.yml".into(),
            "# comment with 10.10.10.9:8080 is ignored\ndest: 10.10.10.16:8080\n".into(),
        );
        texts.insert(
            "routes/manual.yml".into(),
            "url: \"http://10.10.10.9:8080\"\n".into(),
        );
        let flows = addresses_named_in("kyu", &texts);
        assert_eq!(flows.len(), 2);
        assert!(
            flows
                .iter()
                .any(|f| !f.route && f.to_ip.to_string() == "10.10.10.16")
        );
        assert!(
            flows
                .iter()
                .any(|f| f.route && f.to_ip.to_string() == "10.10.10.9")
        );
    }
}
