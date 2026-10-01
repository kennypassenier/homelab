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
    let want = format!(
        "# Written by homelab from stacks/kp-soft/lxc-compose.yml (fix-88): edit that file, not this one\n{}",
        live.replace(icmp, "").replace(
            loki_old,
            "OUT ACCEPT -dest 10.10.10.13 -p tcp -dport 3100 # Loki push (Alloy), added 2026-09-25\n"
        )
    );
    let all = stacks();
    let m = &all["kp-soft"];
    let fw = m.firewall.as_ref().expect("kp-soft declares its firewall");
    assert!(fw.enabled);
    assert_eq!(render("kp-soft", fw), want);
}

/// Every compose stack declares a firewall, and only kp-soft's and admin's
/// are switched on. Turning another one on is a one-word change in its stack
/// file, made on purpose, one stack at a time. admin's is on from its first
/// deploy (arch-firewall, Kenny approved it with arch-safety on 2026-09-28).
/// covers: fix-89
#[test]
fn every_stack_declares_a_firewall_and_only_kp_soft_enables_it() {
    for (name, m) in stacks() {
        let fw = m
            .firewall
            .as_ref()
            .unwrap_or_else(|| panic!("stacks/{} declares no firewall", name));
        let on = name == "kp-soft" || name == "admin";
        assert_eq!(fw.enabled, on, "stacks/{}: enabled must be {}", name, on);
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
