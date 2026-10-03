//! feat-firewall-2: every stack's firewall in one place. The rules of the
//! whole fleet as one list (peers named by stack), and a matrix of which
//! container may open which port on which other one, evaluated with the
//! same first-match reading the deploy's tests use
//! (`homelab_core::firewall::permits`): the source's outbound rules and the
//! destination's inbound rules must both let the flow through. Edits go
//! through the per-stack editor (feat-firewall-1). Pure.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use homelab_core::firewall::permits;
use homelab_core::manifest::{FirewallSpec, FwAction, FwDir, FwProto};
use serde::Serialize;

use super::stackedit::action_word;

/// One stack as the matrix sees it.
#[derive(Debug, Clone)]
pub struct FleetFirewall {
    pub stack: String,
    pub vmid: u16,
    /// The container's address, without the prefix length.
    pub ip: Ipv4Addr,
    pub firewall: Option<FirewallSpec>,
}

/// One rule of one stack, for the fleet-wide table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleRow {
    pub stack: String,
    pub vmid: u16,
    /// 1-based, as the editor numbers them.
    pub n: usize,
    pub dir: &'static str,
    pub action: &'static str,
    /// The address or network on the other side, or "any".
    pub peer: String,
    /// The stacks that address covers, by name.
    pub peer_stacks: Vec<String>,
    pub proto: String,
    pub ports: String,
    pub note: String,
    /// Whether the stack's firewall is in force (`enabled`).
    pub enabled: bool,
    /// redesign-config-8: the rule is switched off (Proxmox skips it).
    pub disabled: bool,
}

/// One cell of the matrix: from `from` to `to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cell {
    pub from: String,
    pub to: String,
    /// `open`: `to` has no firewall in force, anything reaches it;
    /// `some`: the listed flows pass; `none`: nothing declared passes.
    pub state: &'static str,
    /// "tcp 8080", "icmp", …
    pub allowed: Vec<String>,
    /// Flows `to` lets in that `from`'s own outbound rules stop.
    pub stopped_at_source: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Matrix {
    pub stacks: Vec<String>,
    pub cells: Vec<Cell>,
    pub rules: Vec<RuleRow>,
    /// Stacks without a firewall in force, by name.
    pub unguarded: Vec<String>,
}

fn covers(spec: &str, ip: Ipv4Addr) -> bool {
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

fn proto_word(p: FwProto) -> &'static str {
    match p {
        FwProto::Tcp => "tcp",
        FwProto::Udp => "udp",
        FwProto::Icmp => "icmp",
    }
}

fn in_force(f: &Option<FirewallSpec>) -> Option<&FirewallSpec> {
    f.as_ref().filter(|f| f.enabled)
}

/// The first port of every range in a `dport` value.
fn sample_ports(v: &str) -> Vec<u16> {
    v.split(',')
        .filter_map(|p| p.trim().split(':').next()?.parse::<u16>().ok())
        .collect()
}

pub fn matrix(fleet: &[FleetFirewall]) -> Matrix {
    let mut rules = Vec::new();
    for s in fleet {
        let Some(fw) = &s.firewall else { continue };
        for (i, r) in fw.rules.iter().enumerate() {
            let peer = match r.dir {
                FwDir::In => r.source.clone(),
                FwDir::Out => r.dest.clone(),
            };
            let peer_stacks = peer
                .as_deref()
                .map(|p| {
                    fleet
                        .iter()
                        .filter(|o| o.stack != s.stack && covers(p, o.ip))
                        .map(|o| o.stack.clone())
                        .collect()
                })
                .unwrap_or_default();
            rules.push(RuleRow {
                stack: s.stack.clone(),
                vmid: s.vmid,
                n: i + 1,
                dir: if r.dir == FwDir::In { "in" } else { "out" },
                action: action_word(r.action),
                peer: peer.unwrap_or_else(|| "any".into()),
                peer_stacks,
                proto: r.proto.map(proto_word).unwrap_or("any").to_string(),
                ports: r.dport.clone().unwrap_or_default(),
                note: r
                    .note
                    .clone()
                    .or_else(|| {
                        r.comment
                            .clone()
                            .map(|c| c.lines().next().unwrap_or("").to_string())
                    })
                    .unwrap_or_default(),
                enabled: fw.enabled,
                disabled: r.disabled,
            });
        }
    }
    let mut cells = Vec::new();
    for from in fleet {
        for to in fleet {
            if from.stack == to.stack {
                continue;
            }
            let Some(dst) = in_force(&to.firewall) else {
                cells.push(Cell {
                    from: from.stack.clone(),
                    to: to.stack.clone(),
                    state: "open",
                    allowed: vec!["everything".into()],
                    stopped_at_source: Vec::new(),
                });
                continue;
            };
            // The flows `to` names for `from`: its inbound ACCEPT rules
            // whose source covers `from`, one probe per declared port.
            let mut probes: BTreeSet<(u8, &'static str, Option<u16>)> = BTreeSet::new();
            for r in dst.rules.iter().filter(|r| {
                !r.disabled
                    && r.dir == FwDir::In
                    && r.action == FwAction::Accept
                    && r.source.as_deref().is_none_or(|s| covers(s, from.ip))
            }) {
                let protos: Vec<FwProto> = match r.proto {
                    Some(p) => vec![p],
                    None => vec![FwProto::Tcp, FwProto::Udp, FwProto::Icmp],
                };
                for p in protos {
                    let order = match p {
                        FwProto::Tcp => 0,
                        FwProto::Udp => 1,
                        FwProto::Icmp => 2,
                    };
                    match (&r.dport, p) {
                        (_, FwProto::Icmp) => {
                            probes.insert((order, "icmp", None));
                        }
                        (Some(d), _) => {
                            for port in sample_ports(d) {
                                probes.insert((order, proto_word(p), Some(port)));
                            }
                        }
                        (None, _) => {
                            probes.insert((order, proto_word(p), None));
                        }
                    }
                }
            }
            let mut allowed = Vec::new();
            let mut stopped = Vec::new();
            for (_, word, port) in probes {
                let p = match word {
                    "tcp" => FwProto::Tcp,
                    "udp" => FwProto::Udp,
                    _ => FwProto::Icmp,
                };
                let label = match port {
                    Some(n) => format!("{word} {n}"),
                    None if p == FwProto::Icmp => "icmp".to_string(),
                    None => format!("{word} any port"),
                };
                // A port-less rule is probed at an unlikely port.
                let probe_port = match p {
                    FwProto::Icmp => None,
                    _ => Some(port.unwrap_or(40_000)),
                };
                if !permits(dst, to.ip, FwDir::In, from.ip, p, probe_port) {
                    continue;
                }
                let out_ok = in_force(&from.firewall)
                    .map(|src| permits(src, from.ip, FwDir::Out, to.ip, p, probe_port))
                    .unwrap_or(true);
                if out_ok {
                    allowed.push(label);
                } else {
                    stopped.push(label);
                }
            }
            cells.push(Cell {
                from: from.stack.clone(),
                to: to.stack.clone(),
                state: if allowed.is_empty() { "none" } else { "some" },
                allowed,
                stopped_at_source: stopped,
            });
        }
    }
    Matrix {
        stacks: fleet.iter().map(|s| s.stack.clone()).collect(),
        cells,
        rules,
        unguarded: fleet
            .iter()
            .filter(|s| in_force(&s.firewall).is_none())
            .map(|s| s.stack.clone())
            .collect(),
    }
}
