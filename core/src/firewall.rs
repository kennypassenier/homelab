//! fix-88: a container's Proxmox firewall, rendered from its stack file.
//!
//! Expert panel 2026-09-27, flat-vlan-no-east-west-control and
//! traefik-lan-host-header-bypass. Every container sat on one flat VLAN with
//! nothing between them, and the only per-container ruleset (CT 116's) was
//! written by hand on pve. Kenny's answer the same day: a firewall per
//! container, declared in the stack file and written by the deploy — nothing
//! on the machines that the repository does not know.
//!
//! Everything here is pure: the deploy writes what [`render`] returns, and
//! the fleet check compares pve's file with the same function's output, so
//! the two can never disagree about what "declared" means.

use std::net::Ipv4Addr;

use crate::manifest::{FirewallRule, FirewallSpec, FwAction, FwDir, FwProto};

/// Where Proxmox reads a guest's firewall: `<dir>/<vmid>.fw` on pmxcfs.
pub const PVE_FIREWALL_DIR: &str = "/etc/pve/firewall";

/// The management network: pve (10.10.5.250) and OPNsense (10.10.5.1).
/// Measured 2026-09-27 from CT 107: both logins answered 200 directly.
pub const MANAGEMENT_NET: &str = "10.10.5.0/24";

/// The one thing every container needs on the management network: DNS at
/// the router.
pub const ROUTER_DNS: &str = "10.10.5.1";

pub fn fw_path(vmid: u16) -> String {
    format!("{}/{}.fw", PVE_FIREWALL_DIR, vmid)
}

fn comment_lines(out: &mut String, text: &str) {
    for line in text.trim_end_matches('\n').lines() {
        if line.is_empty() {
            out.push_str("#\n");
        } else {
            out.push_str("# ");
            out.push_str(line);
            out.push('\n');
        }
    }
}

fn action(a: FwAction) -> &'static str {
    match a {
        FwAction::Accept => "ACCEPT",
        FwAction::Drop => "DROP",
        FwAction::Reject => "REJECT",
    }
}

fn proto(p: FwProto) -> &'static str {
    match p {
        FwProto::Tcp => "tcp",
        FwProto::Udp => "udp",
        FwProto::Icmp => "icmp",
    }
}

fn dir(d: FwDir) -> &'static str {
    match d {
        FwDir::In => "IN",
        FwDir::Out => "OUT",
    }
}

/// One `[RULES]` line, in the option order Proxmox itself writes.
pub fn rule_line(r: &FirewallRule) -> String {
    let mut s = format!("{} {}", dir(r.dir), action(r.action));
    if let Some(v) = &r.source {
        s.push_str(&format!(" -source {}", v));
    }
    if let Some(v) = &r.dest {
        s.push_str(&format!(" -dest {}", v));
    }
    if let Some(p) = r.proto {
        s.push_str(&format!(" -p {}", proto(p)));
    }
    if let Some(v) = &r.dport {
        s.push_str(&format!(" -dport {}", v));
    }
    if let Some(n) = &r.note {
        s.push_str(&format!(" # {}", n));
    }
    s
}

/// The whole `/etc/pve/firewall/<vmid>.fw` for a declaration.
///
/// The first line says where the file comes from, so whoever opens it on pve
/// learns that a hand edit is undone by the next deploy and reported by the
/// check before it is. The rest is the declaration, in order; the management
/// guard comes last so a declared ACCEPT towards the management network is
/// matched first.
pub fn render(stack: &str, fw: &FirewallSpec) -> String {
    let mut s = format!(
        "# Written by homelab from stacks/{}/lxc-compose.yml (fix-88): edit that file, not this one\n",
        stack
    );
    if let Some(c) = &fw.comment {
        comment_lines(&mut s, c);
    }
    s.push_str("[OPTIONS]\n");
    s.push_str(&format!("enable: {}\n", if fw.enabled { 1 } else { 0 }));
    s.push_str(&format!("policy_in: {}\n", action(fw.policy_in)));
    s.push_str(&format!("policy_out: {}\n", action(fw.policy_out)));
    s.push_str("log_level_in: nolog\nlog_level_out: nolog\n\n[RULES]\n");
    for r in &fw.rules {
        if let Some(c) = &r.comment {
            comment_lines(&mut s, c);
        }
        s.push_str(&rule_line(r));
        s.push('\n');
    }
    if fw.management_open.is_none() {
        comment_lines(
            &mut s,
            "management network: DNS to the router, nothing else (flat-vlan-no-east-west-control,\n\
             traefik-lan-host-header-bypass, 2026-09-27)",
        );
        s.push_str(&format!(
            "OUT ACCEPT -dest {} -p udp -dport 53\n",
            ROUTER_DNS
        ));
        s.push_str(&format!(
            "OUT ACCEPT -dest {} -p tcp -dport 53\n",
            ROUTER_DNS
        ));
        s.push_str(&format!("OUT DROP -dest {}\n", MANAGEMENT_NET));
    }
    s
}

/// `10.10.10.4` or `10.10.10.0/24`. A network written with host bits set is
/// refused: `10.10.10.4/24` means the whole VLAN to Proxmox, and whoever
/// wrote it almost certainly meant one address.
fn addr_problem(v: &str) -> Option<String> {
    let (ip, prefix) = match v.split_once('/') {
        Some((ip, p)) => (ip, Some(p)),
        None => (v, None),
    };
    let Ok(ip) = ip.parse::<Ipv4Addr>() else {
        return Some(format!(
            "'{}' is not an IPv4 address or a CIDR network — write 10.10.10.4 or 10.10.10.0/24",
            v
        ));
    };
    // A single address needs no further check.
    let p = prefix?;
    let Ok(bits) = p.parse::<u8>() else {
        return Some(format!("'{}' has no usable prefix length", v));
    };
    if bits > 32 {
        return Some(format!("'{}' has a prefix longer than 32", v));
    }
    let mask: u32 = if bits == 0 {
        0
    } else {
        u32::MAX << (32 - bits)
    };
    let net = Ipv4Addr::from(u32::from(ip) & mask);
    if net != ip {
        return Some(format!(
            "'{}' has host bits set, so Proxmox reads it as the whole network {}/{} — write \
             {} for the one address or {}/{} for the network",
            v, net, bits, ip, net, bits
        ));
    }
    None
}

/// Every port in a `dport` value, as (low, high) ranges; Err names every
/// part that is not a port or a forward range.
fn port_ranges(v: &str) -> Result<Vec<(u32, u32)>, String> {
    let mut out = Vec::new();
    let mut bad = Vec::new();
    let parse =
        |s: &str| -> Option<u32> { s.parse::<u32>().ok().filter(|n| (1..=65535).contains(n)) };
    for part in v.split(',') {
        let part = part.trim();
        match part.split_once(':') {
            Some((a, b)) => match (parse(a), parse(b)) {
                (Some(a), Some(b)) if a < b => out.push((a, b)),
                (Some(_), Some(_)) => bad.push(format!(
                    "'{}' is a range that runs backwards or holds one port",
                    part
                )),
                _ => bad.push(format!("'{}' is not a range of ports 1 to 65535", part)),
            },
            None => match parse(part) {
                Some(n) => out.push((n, n)),
                None => bad.push(format!("'{}' is not a port between 1 and 65535", part)),
            },
        }
    }
    if bad.is_empty() {
        Ok(out)
    } else {
        Err(bad.join(", "))
    }
}

fn rule_label(i: usize, r: &FirewallRule) -> String {
    let peer = match r.dir {
        FwDir::In => r.source.as_deref().map(|s| format!(" from {}", s)),
        FwDir::Out => r.dest.as_deref().map(|d| format!(" to {}", d)),
    }
    .unwrap_or_default();
    format!(
        "firewall rule {} ({} {}{})",
        i + 1,
        dir(r.dir),
        action(r.action),
        peer
    )
}

/// Everything in a declaration that Proxmox would reject or misread. Called
/// by the manifest validator, so the client refuses it before a deploy and
/// the host refuses it again at its trust boundary.
pub fn problems(fw: &FirewallSpec) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(reason) = &fw.management_open {
        if reason.trim().len() < 10 {
            out.push(format!(
                "firewall management_open '{}' is no usable reason — the guard that keeps the \
                 container off the management network may only be dropped with a reason a \
                 person can read a year from now",
                reason.trim()
            ));
        }
    }
    for (i, r) in fw.rules.iter().enumerate() {
        let label = rule_label(i, r);
        for (field, v) in [("source", &r.source), ("dest", &r.dest)] {
            if let Some(v) = v {
                if let Some(why) = addr_problem(v) {
                    out.push(format!("{}: {} {}", label, field, why));
                }
            }
        }
        if let Some(ports) = &r.dport {
            match r.proto {
                None => out.push(format!(
                    "{}: dport {} without proto — Proxmox needs `proto: tcp` or `proto: udp` \
                     to match a port",
                    label, ports
                )),
                Some(FwProto::Icmp) => out.push(format!(
                    "{}: dport {} with proto icmp — icmp has no ports; drop the dport",
                    label, ports
                )),
                _ => {}
            }
            if let Err(why) = port_ranges(ports) {
                out.push(format!("{}: dport {}", label, why));
            }
        }
        if r.note.as_deref().is_some_and(|n| n.contains('\n')) {
            out.push(format!(
                "{}: note must be one line — it is written after the rule on the same line; \
                 use `comment:` for more",
                label
            ));
        }
    }
    out
}

/// traefik-lan-host-header-bypass (2026-09-27): on the gateway, nothing may
/// open port 80. Traefik routes by the Host header, which any sender fills
/// in, and two hand-written routes forward to the Proxmox and OPNsense
/// logins. The tunnel reaches Traefik over CT 104's own docker network,
/// which never crosses the container's veth, so no neighbour needs port 80.
pub fn gateway_problems(fw: &FirewallSpec) -> Vec<String> {
    let why = "Traefik answers a forged Host header with the Proxmox and OPNsense logins \
               (traefik-lan-host-header-bypass); the tunnel reaches port 80 over the \
               gateway's own docker network, which this firewall never sees, so no neighbour \
               needs it";
    let mut out = Vec::new();
    if fw.policy_in == FwAction::Accept {
        out.push(format!(
            "firewall policy_in ACCEPT opens port 80 on the gateway to every neighbour — {}",
            why
        ));
    }
    for (i, r) in fw.rules.iter().enumerate() {
        if r.dir != FwDir::In || r.action != FwAction::Accept {
            continue;
        }
        let covers_80 = match r.proto {
            None => true,
            Some(FwProto::Tcp) => match &r.dport {
                None => true,
                Some(p) => port_ranges(p)
                    .map(|rs| rs.iter().any(|(a, b)| (*a..=*b).contains(&80)))
                    .unwrap_or(false),
            },
            Some(_) => false,
        };
        if covers_80 {
            out.push(format!(
                "{} opens port 80 on the gateway — {}",
                rule_label(i, r),
                why
            ));
        }
    }
    out
}

/// Lines only in `new` (added) and only in `old` (removed), each in the order
/// of its own file. A line that occurs twice counts twice.
pub fn line_changes(old: &str, new: &str) -> (Vec<String>, Vec<String>) {
    fn minus(a: &str, b: &str) -> Vec<String> {
        let mut pool: Vec<&str> = b.lines().collect();
        let mut out = Vec::new();
        for l in a.lines() {
            match pool.iter().position(|x| *x == l) {
                Some(i) => {
                    pool.remove(i);
                }
                None => out.push(l.to_string()),
            }
        }
        out
    }
    (minus(new, old), minus(old, new))
}

/// `net0` with `firewall=1`, every other part kept as it was (the MAC above
/// all, or the container comes back with a new one); None when the flag is
/// already on. Without the flag Proxmox applies none of the file's rules —
/// CT 116's was set by hand on 2026-09-20, and a deploy creates NICs with
/// `firewall=0`.
pub fn net0_with_firewall(net0: &str) -> Option<String> {
    let parts: Vec<&str> = net0.split(',').map(str::trim).collect();
    if parts.contains(&"firewall=1") {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    for p in parts {
        if p.starts_with("firewall=") {
            out.push("firewall=1".into());
            replaced = true;
        } else {
            out.push(p.to_string());
        }
    }
    if !replaced {
        out.push("firewall=1".into());
    }
    Some(out.join(","))
}
