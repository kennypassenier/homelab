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

use crate::manifest::{FirewallRule, FirewallSpec, FwAction, FwDir, FwProto, Tile};

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
    let mut s = format!(
        "{}{} {}",
        if r.disabled { "|" } else { "" },
        dir(r.dir),
        action(r.action)
    );
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

/// `fw` with the tile-watch rules added (empty when `source` is empty or
/// derives nothing) — what every renderer and the fleet check must agree
/// is "the declaration", so a hand-typed rule and a derived one both show
/// up the same way. Called once per render, right before [`render`].
///
/// Two halves, found a day apart (2026-09-30 then 2026-10-01): the IN rule
/// below opens the watched stack to the watcher; [`derive_watcher_out_rules`]
/// opens the WATCHER's own outbound side towards every watched stack, and is
/// PREPENDED so it is matched before this container's own declared `OUT
/// DROP` rules (every container's east-west guard, `flat-vlan-no-east-
/// west-control`) — appending it after those would never be reached. The IN
/// rule stays appended: `policy_in` is DROP fleet-wide, so order among IN
/// rules never matters the same way.
pub fn with_tile_watch(
    fw: &FirewallSpec,
    own_ip: &str,
    tiles: &std::collections::BTreeMap<String, Tile>,
    source: &str,
    fleet_targets: &FleetTileTargets,
) -> FirewallSpec {
    let mut fw = fw.clone();
    let out_rules = derive_watcher_out_rules(own_ip, source, fleet_targets);
    if !out_rules.is_empty() {
        fw.rules = out_rules.into_iter().chain(fw.rules).collect();
    }
    fw.rules
        .extend(derive_tile_watch_rules(own_ip, tiles, source));
    fw
}

/// One fleet stack's own address and the tile-watch ports its own `tiles:`
/// open on it (the same ports [`derive_tile_watch_rules`] would open
/// inbound for it) — the fleet-wide input the watcher's OUT rule is derived
/// from. `(ip, ports)`, sorted by ip, `ip` CIDR-stripped.
pub type FleetTileTargets = Vec<(String, std::collections::BTreeSet<u16>)>;

/// [`FleetTileTargets`] from every applied stack's tiles: one entry per
/// probe HOST, wherever it is — a stack's own container or a device a
/// gateway route reaches (Kenny, 2026-10-01: "HA, Proxmox en OPN moeten
/// wel gemeten worden door ons"). Each probe opens exactly its host and
/// port in the watcher's OUT rules, nothing wider. The `&str` of each
/// stack is unused now and kept so callers walk `state.stacks` unchanged.
pub fn derive_fleet_tile_targets<'a>(
    stacks: impl IntoIterator<Item = (&'a str, &'a std::collections::BTreeMap<String, Tile>)>,
) -> FleetTileTargets {
    let mut by_host: std::collections::BTreeMap<String, std::collections::BTreeSet<u16>> =
        std::collections::BTreeMap::new();
    for (_own_ip, tiles) in stacks {
        for t in tiles.values() {
            if let Some((host, port)) = t.probe.as_deref().and_then(probe_target) {
                by_host.entry(host).or_default().insert(port);
            }
        }
    }
    by_host.into_iter().collect()
}

/// `targets` joined with the probes of a manifest a deploy is writing right
/// now — so the watcher's OUT rules already include a tile that this very
/// deploy adds, without a second state load. A tile this deploy removes
/// stays in the watcher's rules until the next op reads the state again
/// (an extra, narrow allow, never a missing one). `enabled` false adds
/// nothing.
pub fn with_fresh_target(
    targets: &FleetTileTargets,
    _own_ip: &str,
    tiles: &std::collections::BTreeMap<String, Tile>,
    enabled: bool,
) -> FleetTileTargets {
    let mut by_host: std::collections::BTreeMap<String, std::collections::BTreeSet<u16>> =
        targets.iter().cloned().collect();
    if enabled {
        for (h, ports) in derive_fleet_tile_targets(std::iter::once(("", tiles))) {
            by_host.entry(h).or_default().extend(ports);
        }
    }
    by_host.into_iter().collect()
}

/// A probe URL's host and port (explicit, else 443/80 by scheme); None for
/// a URL without a scheme or an unknown scheme without a port.
pub fn probe_target(url: &str) -> Option<(String, u16)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            (h, p.parse::<u16>().ok())
        }
        _ => (authority, None),
    };
    let port = port.or(match scheme {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    })?;
    (!host.is_empty()).then(|| (host.to_string(), port))
}

/// Everything a deploy needs to re-render and write the WATCHER's own
/// firewall file — the stack whose own ip is the fleet's
/// `tile_watch_source` — without redeploying it. Found 2026-10-01: the
/// watcher's OUT rule depends on every OTHER stack's tiles, so a tile
/// arriving or leaving anywhere must reach the watcher's file, not just the
/// stack that changed.
#[derive(Debug, Clone)]
pub struct TileWatcher {
    pub vmid: u16,
    pub stack: String,
    pub own_ip: String,
    pub firewall: FirewallSpec,
    pub tiles: std::collections::BTreeMap<String, Tile>,
}

/// OUT rules the watcher itself needs: one `OUT ACCEPT -dest <ip> -p tcp
/// -dport <ports>` per fleet target, when `own_ip` (CIDR-stripped) is the
/// fleet's `tile_watch_source` (also CIDR-stripped, by the same convention
/// every other tile-watch function uses). The missing half measured
/// 2026-10-01: the watched stack's IN rule came from [`derive_tile_watch_rules`]
/// readily enough, but the watcher's own `OUT DROP -dest <lan>` (every
/// container's east-west guard) silently ate its outbound probes, because
/// nothing ever told the watcher's own firewall about them — 23 false "does
/// not answer" notices from curl returning 000. Empty when `source` is
/// empty or `own_ip` is not the watcher.
pub fn derive_watcher_out_rules(
    own_ip: &str,
    source: &str,
    targets: &FleetTileTargets,
) -> Vec<FirewallRule> {
    let source = source.trim();
    if source.is_empty() {
        return Vec::new();
    }
    let own_ip = own_ip.split('/').next().unwrap_or(own_ip);
    let source = source.split('/').next().unwrap_or(source);
    if own_ip != source {
        return Vec::new();
    }
    targets
        .iter()
        .filter(|(_, ports)| !ports.is_empty())
        .map(|(ip, ports)| {
            let dport = ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(",");
            FirewallRule {
                disabled: false,
                dir: FwDir::Out,
                action: FwAction::Accept,
                source: None,
                dest: Some(ip.clone()),
                proto: Some(FwProto::Tcp),
                dport: Some(dport),
                comment: None,
                note: Some("tile watch (derived from tiles)".to_string()),
            }
        })
        .collect()
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
        for r in guard_rules() {
            s.push_str(&rule_line(&r));
            s.push('\n');
        }
    }
    s
}

/// replace-homepage / tile-watch (owner decision "Afgeleid uit de tegels",
/// 2026-09-30): the inbound rule the dashboard's once-a-minute tile watch
/// needs on this container, derived from its own `tiles:` declarations —
/// never hand-typed, so a tile that goes takes its rule with it at the next
/// deploy, and a tile that arrives opens for it without anyone touching the
/// firewall block.
///
/// `source` is the fleet-wide `tile_watch_source` (host.toml); `own_ip` is
/// this container's own address, CIDR or bare (`network.ip` carries the
/// `/24`; the CIDR is stripped before comparing, so every caller can pass
/// it unchanged). A tile counts only when its `probe` — the plain backend
/// address the client resolved at deploy time, `Tile::probe`, never a
/// hostname read here — names `own_ip`: a tile with no `probe` (a Traefik
/// hostname no route in this stack's own `traefik-routes.yml` resolves) or
/// one whose probe names some other container is not this firewall's
/// business.
///
/// Returns nothing when `source` is empty (feature off) or no tile resolves
/// to a port on this container.
pub fn derive_tile_watch_rules(
    own_ip: &str,
    tiles: &std::collections::BTreeMap<String, Tile>,
    source: &str,
) -> Vec<FirewallRule> {
    let source = source.trim();
    if source.is_empty() {
        return Vec::new();
    }
    let own_ip = own_ip.split('/').next().unwrap_or(own_ip);
    let mut ports: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
    for t in tiles.values() {
        let Some(probe) = t.probe.as_deref() else {
            continue;
        };
        if let Some(port) = tile_port_on(probe, own_ip) {
            ports.insert(port);
        }
    }
    if ports.is_empty() {
        return Vec::new();
    }
    let dport = ports
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    vec![FirewallRule {
        disabled: false,
        dir: FwDir::In,
        action: FwAction::Accept,
        source: Some(source.to_string()),
        dest: None,
        proto: Some(FwProto::Tcp),
        dport: Some(dport),
        comment: None,
        note: Some("tile watch (derived from tiles)".to_string()),
    }]
}

/// The port `url` opens on `own_ip` (bare, no `/prefix`), or None when its
/// host is not `own_ip`. An explicit port in the URL wins; otherwise 443
/// for `https`, 80 for `http`, and None for any other scheme.
fn tile_port_on(url: &str, own_ip: &str) -> Option<u16> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(rest)
        .trim_end_matches('/');
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            (h, p.parse::<u16>().ok())
        }
        _ => (authority, None),
    };
    if host != own_ip {
        return None;
    }
    port.or(match scheme {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    })
}

/// The management guard as rules: DNS to the router over udp and tcp, then
/// nothing else on the management network.
fn guard_rules() -> Vec<FirewallRule> {
    let rule = |action, proto, dport: Option<&str>, dest: &str| FirewallRule {
        disabled: false,
        dir: FwDir::Out,
        action,
        source: None,
        dest: Some(dest.to_string()),
        proto,
        dport: dport.map(str::to_string),
        comment: None,
        note: None,
    };
    vec![
        rule(FwAction::Accept, Some(FwProto::Udp), Some("53"), ROUTER_DNS),
        rule(FwAction::Accept, Some(FwProto::Tcp), Some("53"), ROUTER_DNS),
        rule(FwAction::Drop, None, None, MANAGEMENT_NET),
    ]
}

/// Whether `ip` falls under `spec` (an address or a CIDR network).
fn addr_matches(spec: &str, ip: Ipv4Addr) -> bool {
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

/// fix-89: would this declaration let one flow through, reading its rules
/// the way Proxmox does — first match in order, the management guard after
/// the declared rules, the policy when nothing matches.
///
/// It is how the declarations are held against the flows measured on
/// 2026-09-27: a rule set that would break a real connection fails a test
/// before it is ever enabled. `own` is the container's address, `peer` the
/// other end; `port` is the destination port (None for icmp).
pub fn permits(
    fw: &FirewallSpec,
    own: Ipv4Addr,
    dir: FwDir,
    peer: Ipv4Addr,
    proto: FwProto,
    port: Option<u16>,
) -> bool {
    let mut rules: Vec<FirewallRule> = fw.rules.clone();
    if fw.management_open.is_none() {
        rules.extend(guard_rules());
    }
    let (src, dst) = match dir {
        FwDir::In => (peer, own),
        FwDir::Out => (own, peer),
    };
    for r in rules.iter().filter(|r| r.dir == dir && !r.disabled) {
        if r.source.as_deref().is_some_and(|s| !addr_matches(s, src))
            || r.dest.as_deref().is_some_and(|d| !addr_matches(d, dst))
            || r.proto.is_some_and(|p| p != proto)
        {
            continue;
        }
        if let Some(ports) = &r.dport {
            let Some(port) = port else { continue };
            let hit = port_ranges(ports)
                .map(|rs| rs.iter().any(|(a, b)| (*a..=*b).contains(&u32::from(port))))
                .unwrap_or(false);
            if !hit {
                continue;
            }
        }
        return r.action == FwAction::Accept;
    }
    match dir {
        FwDir::In => fw.policy_in == FwAction::Accept,
        FwDir::Out => fw.policy_out == FwAction::Accept,
    }
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
    if let Some(reason) = &fw.management_open
        && reason.trim().len() < 10
    {
        out.push(format!(
            "firewall management_open '{}' is no usable reason — the guard that keeps the \
                 container off the management network may only be dropped with a reason a \
                 person can read a year from now",
            reason.trim()
        ));
    }
    for (i, r) in fw.rules.iter().enumerate() {
        let label = rule_label(i, r);
        for (field, v) in [("source", &r.source), ("dest", &r.dest)] {
            if let Some(v) = v
                && let Some(why) = addr_problem(v)
            {
                out.push(format!("{}: {} {}", label, field, why));
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
