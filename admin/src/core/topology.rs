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
//! No app names: everything here is `stack`, `vmid`, `ip` and the protocol
//! words the firewall module already has.

use serde::Serialize;

use super::fwmatrix::{FleetFirewall, Matrix};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Node {
    pub stack: String,
    pub vmid: u16,
    pub ip: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// `declared`: at least one specific rule permits it; `open`: `to` runs
    /// no firewall at all, so nothing declared stops `from` (or anyone
    /// else) reaching it.
    pub kind: &'static str,
    /// What passes ("tcp 8080", "icmp", "everything"), for the label.
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Topology {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Build the topology straight from the fleet's firewall declarations.
/// Pure given the matrix is already computed; kept separate from
/// [`from_fleet`] so a caller that already has a `Matrix` (the firewall
/// page) does not pay for computing it twice.
pub fn from_matrix(fleet: &[FleetFirewall], matrix: &Matrix) -> Topology {
    let nodes = fleet
        .iter()
        .map(|f| Node {
            stack: f.stack.clone(),
            vmid: f.vmid,
            ip: f.ip.to_string(),
        })
        .collect();
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

/// feat-stacks-9: what each stack reaches (its outbound dependencies) and
/// what reaches it (who depends on it), read off the same edges — a
/// dependency is simply a declared (or open) flow, directed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyRow {
    pub stack: String,
    /// Other stacks this one is declared to reach.
    pub depends_on: Vec<String>,
    /// Other stacks declared to reach this one.
    pub depended_on_by: Vec<String>,
}

pub fn dependencies(topo: &Topology) -> Vec<DependencyRow> {
    topo.nodes
        .iter()
        .map(|n| {
            let mut depends_on: Vec<String> = topo
                .edges
                .iter()
                .filter(|e| e.from == n.stack)
                .map(|e| e.to.clone())
                .collect();
            let mut depended_on_by: Vec<String> = topo
                .edges
                .iter()
                .filter(|e| e.to == n.stack)
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
        assert!(!topo
            .edges
            .iter()
            .any(|e| e.from == "stranger" && e.to == "app"));
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
}
