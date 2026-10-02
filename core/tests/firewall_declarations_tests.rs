//! fix-89: the firewall every stack file declares (fix-88), held against the
//! house as it was measured on 2026-09-27.
//!
//! Kenny's answer "samenvoegen-firewall-per-container" (2026-09-27): each
//! container gets its rules in its own stack file; only kp-soft's are in force
//! in this change, the rest are declared with `enabled: false` so the rollout
//! can be switched on one stack at a time. Declared-but-off rules are exactly
//! the kind nobody tests until the day they are switched on, so the flows
//! measured that day are written down here and every declaration must let
//! each of them through on both ends, as Proxmox would read the rules.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use homelab_core::firewall::{self, render};
use homelab_core::manifest::{FwDir, FwProto, StackManifest};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Every stack that carries a compose manifest, by directory name.
fn stacks() -> BTreeMap<String, StackManifest> {
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(repo_root().join("stacks"))
        .unwrap()
        .flatten()
    {
        let f = e.path().join("lxc-compose.yml");
        if !f.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let m: StackManifest = serde_yaml::from_str(&std::fs::read_to_string(&f).unwrap())
            .unwrap_or_else(|err| panic!("stacks/{}: {}", name, err));
        out.insert(name, m);
    }
    out
}

fn ip_of(m: &StackManifest) -> Ipv4Addr {
    m.network
        .ip
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap_or_else(|e| panic!("{}: {}", m.stack_name, e))
}

/// CT 116's declaration renders to the file on pve (captured byte for byte on
/// 2026-09-27) with these differences: the provenance line on top, the Loki
/// push going to the metrics stack (10.10.10.13) instead of the gateway,
/// where Loki no longer runs since fix-90 (2026-09-28: without it CT 116's
/// Alloy would push into a closed port), and — since 2026-10-01 — Uptime
/// Kuma's ping rule gone along with the rest of Uptime Kuma (its HTTP-monitor
/// rule, added 2026-09-27 by form item kp-soft-kuma-firewall, is gone the
/// same way: it was never in this captured snapshot to begin with).
/// covers: fix-89
#[test]
fn kp_soft_declares_its_live_firewall_minus_kuma() {
    let live =
        std::fs::read_to_string(repo_root().join("captured/pve-host/firewall/116.fw")).unwrap();
    let icmp = "IN ACCEPT -source 10.10.10.7 -p icmp\n";
    assert!(live.contains(icmp));
    let loki_old =
        "OUT ACCEPT -dest 10.10.10.4 -p tcp -dport 3100 # Loki push (Alloy), added 2026-09-25\n";
    assert!(live.contains(loki_old));
    // Uptime Kuma's retirement (2026-10-01) did not just drop its icmp rule
    // (above) — the stack file's own prose comment was rewritten to say so,
    // and the "# 4 · who may reach" rule comment dropped its mention too.
    // Both are hand-written text in `stacks/kp-soft/lxc-compose.yml`, copied
    // here rather than derived, the same way `icmp` and `loki_old` are.
    let header_old = "# route (8080, 8787), the Prometheus targets (8081, 9100) and the Uptime Kuma\n\
                       # ping. Kenny approved it on 2026-09-20 (\"Klopt\").\n";
    assert!(live.contains(header_old));
    let header_new = "# route (8080, 8787) and the Prometheus targets (8081, 9100).\n\
                       # Kenny approved it on 2026-09-20 (\"Klopt\"). Uptime Kuma's rules, added\n\
                       # 2026-09-27, were retired with Uptime Kuma on 2026-10-01.\n";
    let rule4_old = "#     (cadvisor, node exporter), Uptime Kuma (ping), Kenny's desktop (ssh)\n";
    assert!(live.contains(rule4_old));
    let rule4_new = "#     (cadvisor, node exporter), Kenny's desktop (ssh)\n";
    let want = format!(
        "# Written by homelab from stacks/kp-soft/lxc-compose.yml (fix-88): edit that file, not this one\n{}",
        live.replace(icmp, "")
            .replace(
                loki_old,
                "OUT ACCEPT -dest 10.10.10.13 -p tcp -dport 3100 # Loki push (Alloy), added 2026-09-25\n"
            )
            .replace(header_old, header_new)
            .replace(rule4_old, rule4_new)
    );
    let all = stacks();
    let m = &all["kp-soft"];
    let fw = m.firewall.as_ref().expect("kp-soft declares its firewall");
    assert!(fw.enabled);
    assert_eq!(render("kp-soft", fw), want);
}

/// Every compose stack declares a firewall, and every declaration is valid.
/// Which ones are switched on is the rollout's state, not this test's: the
/// rollout of 2026-10-02 turns them on one stack at a time, each measured
/// (docs/deployment/REGISTER.md fix-89), and the flows they must let through
/// are held by `every_flow_the_stack_files_name_passes_the_declared_firewalls`.
/// covers: fix-89
#[test]
fn every_stack_declares_a_valid_firewall() {
    for (name, m) in stacks() {
        let fw = m
            .firewall
            .as_ref()
            .unwrap_or_else(|| panic!("stacks/{} declares no firewall", name));
        assert!(
            firewall::problems(fw).is_empty(),
            "stacks/{}: {:?}",
            name,
            firewall::problems(fw)
        );
    }
}

/// traefik-lan-host-header-bypass: the gateway's declaration opens port 80 to
/// nobody, so switching it on closes the Host-header route to the Proxmox and
/// OPNsense logins.
/// covers: fix-89
#[test]
fn the_gateway_declaration_opens_port_80_to_nobody() {
    let all = stacks();
    let fw = all["gateway"].firewall.as_ref().unwrap();
    assert!(
        firewall::gateway_problems(fw).is_empty(),
        "{:?}",
        firewall::gateway_problems(fw)
    );
}

/// One flow measured on 2026-09-27: `from` connects to `to` on `port`.
struct Flow {
    from: &'static str,
    to: &'static str,
    proto: FwProto,
    port: Option<u16>,
    why: &'static str,
}

fn tcp(from: &'static str, to: &'static str, ports: &[u16], why: &'static str) -> Vec<Flow> {
    ports
        .iter()
        .map(|p| Flow {
            from,
            to,
            proto: FwProto::Tcp,
            port: Some(*p),
            why,
        })
        .collect()
}

fn udp(from: &'static str, to: &'static str, ports: &[u16], why: &'static str) -> Vec<Flow> {
    ports
        .iter()
        .map(|p| Flow {
            from,
            to,
            proto: FwProto::Udp,
            port: Some(*p),
            why,
        })
        .collect()
}

const DOCKER: [&str; 8] = [
    "10.10.10.4",
    "10.10.10.5",
    "10.10.10.6",
    "10.10.10.8",
    "10.10.10.11",
    "10.10.10.13",
    "10.10.10.14",
    "10.10.10.16",
];
const MEDIA: [u16; 6] = [8096, 8989, 7878, 6767, 9696, 5055];

/// Every flow the 2026-09-27 measurement found (tcpdump on pve's veths for
/// 29 minutes, conntrack and ss in each container, the Prometheus targets,
/// the Traefik routes, the stacks' configuration). Uptime Kuma's and
/// Homepage's own flows, measured the same day, were removed 2026-10-01 when
/// both were retired.
fn measured_flows() -> Vec<Flow> {
    let mut f = Vec::new();
    let all: Vec<&'static str> = DOCKER
        .iter()
        .copied()
        .chain(["10.10.10.9", "10.10.10.12", "10.10.10.17"])
        .collect();
    for s in &all {
        f.extend(udp(s, "10.10.5.1", &[53], "DNS at the router"));
        f.extend(tcp(
            s,
            "10.10.5.1",
            &[53],
            "DNS at the router, tcp fallback",
        ));
        f.extend(tcp("10.10.10.10", s, &[22], "Kenny's desktop, ssh"));
        // Loki moved to the metrics stack (fix-90/93, 2026-09-27): every
        // Alloy pushes to the loki-push front on CT 113.
        if *s != "10.10.10.13" {
            f.extend(tcp(s, "10.10.10.13", &[3100], "Loki push (Alloy)"));
        }
    }
    for s in DOCKER {
        f.extend(tcp(
            s,
            "10.10.10.17",
            &[5000, 5001, 5002, 5003],
            "registry cache",
        ));
        if s != "10.10.10.13" {
            f.extend(tcp("10.10.10.13", s, &[8081, 9100], "Prometheus scrape"));
        }
    }
    f.extend(tcp(
        "10.10.10.13",
        "10.10.10.17",
        &[8081, 9100],
        "Prometheus scrape",
    ));
    for s in ["10.10.10.9", "10.10.10.12"] {
        f.extend(tcp("10.10.10.13", s, &[8080, 9100], "Prometheus scrape"));
    }
    f.extend(tcp("10.10.10.13", "10.10.10.4", &[8082], "traefik metrics"));
    f.extend(tcp(
        "10.10.10.13",
        "10.10.5.250",
        &[8006, 9100],
        "pve-exporter, pve node exporter",
    ));
    f.extend(tcp(
        "10.10.10.13",
        "10.10.10.9",
        &[8080],
        "Alertmanager webhook to kyu",
    ));
    // The daemon's Loki queries run inside CT 113 since fix-93 (loki_vmid),
    // so there is no flow from pve to port 3100 any more.
    f.extend(tcp(
        "10.10.10.250",
        "10.10.10.13",
        &[9090],
        "homelab daemon's Prometheus queries",
    ));
    f.extend(tcp(
        "10.10.10.250",
        "10.10.10.9",
        &[8080],
        "homelab daemon's notifications",
    ));
    f.extend(udp("10.10.10.1", "10.10.10.4", &[1514], "OPNsense syslog"));
    // Traefik routes.
    f.extend(tcp("10.10.10.4", "10.10.10.5", &[8080], "route"));
    f.extend(tcp("10.10.10.4", "10.10.10.6", &MEDIA, "route"));
    f.extend(tcp("10.10.10.4", "10.10.10.8", &[8384], "route"));
    f.extend(tcp("10.10.10.4", "10.10.10.9", &[8080], "route"));
    f.extend(tcp("10.10.10.4", "10.10.10.11", &[1900], "route"));
    f.extend(tcp("10.10.10.4", "10.10.10.12", &[8080], "route"));
    f.extend(tcp("10.10.10.4", "10.10.10.13", &[9090, 9093], "route"));
    f.extend(tcp(
        "10.10.10.4",
        "10.10.10.14",
        &[5006, 8080, 8000],
        "route",
    ));
    f.extend(tcp("10.10.10.4", "10.10.10.16", &[8787], "route"));
    f.extend(tcp(
        "10.10.10.4",
        "10.10.10.2",
        &[8123],
        "route to Home Assistant",
    ));
    f.extend(tcp("10.10.10.4", "10.10.5.1", &[443], "route opn"));
    f.extend(tcp("10.10.10.4", "10.10.5.250", &[8006], "route prox"));
    // Between the services.
    f.extend(tcp(
        "10.10.10.6",
        "10.10.10.5",
        &[8080],
        "*arr download client",
    ));
    f.extend(tcp(
        "10.10.10.6",
        "10.10.10.9",
        &[8080],
        "*arr webhook to kyu",
    ));
    f.extend(tcp("10.10.10.6", "10.10.10.2", &[8123], "Jellyfin webhook"));
    f.extend(tcp(
        "10.10.10.9",
        "10.10.10.2",
        &[8123],
        "kyu-runner, http-switchboard",
    ));
    f.extend(tcp("10.10.10.2", "10.10.10.5", &[8080], "HA qBittorrent"));
    f.extend(tcp(
        "10.10.10.2",
        "10.10.10.9",
        &[8080],
        "HA mailbox_publish",
    ));
    f.extend(tcp(
        "10.10.10.16",
        "10.10.10.12",
        &[8080],
        "JobTracker events",
    ));
    f.extend(tcp(
        "10.10.10.10",
        "10.10.10.6",
        &[8096],
        "desktop Jellyfin",
    ));
    f.extend(tcp(
        "10.10.10.10",
        "10.10.10.9",
        &[8080],
        "desktop kyu clients",
    ));
    // Syncthing with the desktop and the phone.
    for peer in ["10.10.10.10", "10.10.10.153"] {
        f.extend(tcp("10.10.10.8", peer, &[22000], "sync"));
        f.extend(tcp(peer, "10.10.10.8", &[22000], "sync"));
        f.extend(udp(peer, "10.10.10.8", &[21027], "local discovery"));
    }
    f.extend(udp(
        "10.10.10.8",
        "10.10.10.255",
        &[21027],
        "local discovery broadcast",
    ));
    f
}

/// Every measured flow passes the declared firewall at both ends, read the
/// way Proxmox reads it — including the declarations not yet enabled, which
/// is the point: they are switched on later, one at a time, and must not
/// break a real connection on that day.
/// covers: fix-89
#[test]
fn every_measured_flow_passes_the_declared_firewalls_at_both_ends() {
    let by_ip: BTreeMap<Ipv4Addr, StackManifest> =
        stacks().into_values().map(|m| (ip_of(&m), m)).collect();
    let mut broken = Vec::new();
    let mut undeclared = Vec::new();
    for fl in measured_flows() {
        let from: Ipv4Addr = fl.from.parse().unwrap();
        let to: Ipv4Addr = fl.to.parse().unwrap();
        for (ip, dir, peer) in [(from, FwDir::Out, to), (to, FwDir::In, from)] {
            let Some(m) = by_ip.get(&ip) else {
                continue;
            };
            let Some(fw) = m.firewall.as_ref() else {
                undeclared.push(m.stack_name.clone());
                continue;
            };
            if !firewall::permits(fw, ip, dir, peer, fl.proto, fl.port) {
                broken.push(format!(
                    "{} -> {} {:?}/{:?} ({}): refused {} {}",
                    fl.from,
                    fl.to,
                    fl.proto,
                    fl.port,
                    fl.why,
                    if dir == FwDir::Out {
                        "leaving"
                    } else {
                        "entering"
                    },
                    m.stack_name
                ));
            }
        }
    }
    undeclared.sort();
    undeclared.dedup();
    assert!(
        undeclared.is_empty(),
        "no firewall declared: {:?}",
        undeclared
    );
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

/// And what the findings are about stays shut once a stack is switched on:
/// no neighbour reaches Traefik's port 80 (the Host-header route to the
/// logins), and no container but the ones that declare it reaches pve's or
/// OPNsense's GUI.
/// covers: fix-89
#[test]
fn the_logins_stay_out_of_reach_from_the_neighbours() {
    let all = stacks();
    let gw = all["gateway"]
        .firewall
        .as_ref()
        .expect("the gateway declares its firewall");
    let gw_ip = ip_of(&all["gateway"]);
    for (name, m) in &all {
        let ip = ip_of(m);
        assert!(
            !firewall::permits(gw, gw_ip, FwDir::In, ip, FwProto::Tcp, Some(80)),
            "{} reaches Traefik's port 80",
            name
        );
        // The two that declare a management destination (the prox and opn
        // routes, pve-exporter), and kp-soft, which the router shuts off
        // from that network itself.
        if ["gateway", "metrics", "kp-soft"].contains(&name.as_str()) {
            continue;
        }
        let fw = m
            .firewall
            .as_ref()
            .unwrap_or_else(|| panic!("{} declares no firewall", name));
        for (mgmt, port) in [("10.10.5.250", 8006), ("10.10.5.1", 443)] {
            assert!(
                !firewall::permits(
                    fw,
                    ip,
                    FwDir::Out,
                    mgmt.parse().unwrap(),
                    FwProto::Tcp,
                    Some(port)
                ),
                "{} reaches {}:{}",
                name,
                mgmt,
                port
            );
        }
    }
}

/// Every `10.10.5.x:port` / `10.10.10.x:port` in one line of text.
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

/// Every `10.10.x.y:port` written in `dir`'s files (comment lines and `.env`
/// files skipped), outside the `routes/` directory.
fn addresses_named_in(dir: &Path) -> Vec<(Ipv4Addr, u16, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if name != "routes" && !name.starts_with('.') {
                    stack.push(p);
                }
                continue;
            }
            if name.ends_with(".env") || name.starts_with('.') {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
                for (ip, port) in house_addresses(line) {
                    out.push((ip, port, p.display().to_string()));
                }
            }
        }
    }
    out
}

/// fix-193: derived from the stack files themselves, so it cannot fall
/// behind the way a hand-kept flow list does (2026-10-02: the dashboard on
/// CT 120 arrived after the 2026-09-27 measurement, and neither the
/// gateway's route to it nor Alertmanager's webhook to it was allowed out —
/// switching either firewall on would have cut the dashboard off).
///
/// Two kinds of flow: every address a stack's own files name is one it
/// connects to (OUT at its end, IN at the other end when that is a managed
/// stack), and every route file under any `stacks/*/routes/` is a flow from
/// the gateway to the route's backend.
/// covers: fix-193
#[test]
fn every_flow_the_stack_files_name_passes_the_declared_firewalls() {
    let all = stacks();
    let by_ip: BTreeMap<Ipv4Addr, &StackManifest> = all.values().map(|m| (ip_of(m), m)).collect();
    let gateway_vmid = all
        .keys()
        .find_map(|name| {
            let raw = std::fs::read_to_string(
                repo_root()
                    .join("stacks")
                    .join(name)
                    .join("lxc-compose.yml"),
            )
            .ok()?;
            let v: serde_yaml::Value = serde_yaml::from_str(&raw).ok()?;
            v.get("gateway_route")?.get("gateway_vmid")?.as_u64()
        })
        .expect("some stack routes through the gateway") as u16;
    let gateway = all
        .values()
        .find(|m| m.vmid == gateway_vmid)
        .expect("the gateway is a stack");
    let mut flows: Vec<(Ipv4Addr, Ipv4Addr, u16, String)> = Vec::new();
    for (name, m) in &all {
        let dir = repo_root().join("stacks").join(name);
        for (to, port, file) in addresses_named_in(&dir) {
            flows.push((ip_of(m), to, port, file));
        }
        for e in std::fs::read_dir(dir.join("routes"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let text = std::fs::read_to_string(e.path()).unwrap_or_default();
            for line in text.lines().filter(|l| !l.trim_start().starts_with('#')) {
                for (to, port) in house_addresses(line) {
                    flows.push((ip_of(gateway), to, port, e.path().display().to_string()));
                }
            }
        }
    }
    let mut broken = Vec::new();
    for (from, to, port, file) in flows {
        if from == to {
            continue;
        }
        for (ip, dir, peer) in [(from, FwDir::Out, to), (to, FwDir::In, from)] {
            let Some(m) = by_ip.get(&ip) else { continue };
            let Some(fw) = m.firewall.as_ref() else {
                continue;
            };
            if !firewall::permits(fw, ip, dir, peer, FwProto::Tcp, Some(port)) {
                broken.push(format!(
                    "{from} -> {to}:{port} (named in {file}): refused {} {}",
                    if dir == FwDir::Out {
                        "leaving"
                    } else {
                        "entering"
                    },
                    m.stack_name
                ));
            }
        }
    }
    broken.sort();
    broken.dedup();
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}
