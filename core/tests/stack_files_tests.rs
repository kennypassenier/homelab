//! G10 · the thirteen stack files that actually run this house, validated.
//!
//! Every other test in this workspace builds a manifest in code. That proves
//! the validator works; it proves nothing about the files on disk, which are
//! the ones a deploy reads. The gap the Phase-7 audit found is exactly that
//! distance — and the register is full of faults that lived in a real file
//! while every synthetic one was fine: a template pointing at the retired
//! golden image, a stack contradicting itself in consecutive lines, a
//! promtail label naming the wrong container.
//!
//! These run offline. The half that needs latch (substituting secrets into
//! the compose files before parsing them) is deliberately not here — a test
//! that cannot run without a decrypted vault is a test that does not run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use homelab_core::checks::ServiceChecks;
use homelab_core::manifest::{validate_manifest, StackManifest};

fn stacks_dir() -> PathBuf {
    // core/tests/ -> repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join("stacks")
}

/// Every stack directory that carries a compose manifest. Native stacks
/// (`service.yml`) are a different shape and are not this test's subject.
fn compose_stacks() -> Vec<(String, StackManifest)> {
    let mut out = Vec::new();
    let dir = stacks_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("stacks/ must be readable at {:?}: {}", dir, e))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        let f = p.join("lxc-compose.yml");
        if !f.is_file() {
            continue;
        }
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&f).unwrap();
        let m: StackManifest = serde_yaml::from_str(&text)
            .unwrap_or_else(|e| panic!("{} does not parse as a manifest: {}", name, e));
        out.push((name, m));
    }
    assert!(
        out.len() >= 10,
        "found only {} compose stacks — this test is looking in the wrong place",
        out.len()
    );
    out
}

#[test]
fn every_stack_file_on_disk_passes_the_validator_a_deploy_would_run() {
    for (name, m) in compose_stacks() {
        validate_manifest(&m).unwrap_or_else(|e| panic!("stacks/{} is not valid: {:?}", name, e));
    }
}

#[test]
fn the_directory_name_is_the_stack_name() {
    for (name, m) in compose_stacks() {
        assert_eq!(
            m.stack_name, name,
            "stacks/{} calls itself {} — every lookup in this project keys off the \
             directory, so the two must agree",
            name, m.stack_name
        );
    }
}

#[test]
fn no_two_stacks_claim_the_same_vmid_hostname_or_address() {
    let mut vmids: BTreeMap<u16, String> = BTreeMap::new();
    let mut hosts: BTreeMap<String, String> = BTreeMap::new();
    let mut ips: BTreeMap<String, String> = BTreeMap::new();
    for (name, m) in compose_stacks() {
        if let Some(prev) = vmids.insert(m.vmid, name.clone()) {
            panic!("{} and {} both claim vmid {}", prev, name, m.vmid);
        }
        if let Some(prev) = hosts.insert(m.hostname.clone(), name.clone()) {
            panic!("{} and {} both claim hostname {}", prev, name, m.hostname);
        }
        let ip = m.network.ip.clone();
        if let Some(prev) = ips.insert(ip.clone(), name.clone()) {
            panic!("{} and {} both claim {}", prev, name, ip);
        }
    }
}

/// The layout's own rule, and the one that makes a container findable without
/// looking anything up: `<vmid>-app-<stack>` at `10.10.10.<vmid - 100>`.
#[test]
fn hostname_and_address_both_follow_from_the_vmid() {
    for (name, m) in compose_stacks() {
        assert!(
            m.hostname.starts_with(&format!("{}-", m.vmid)),
            "stacks/{}: hostname {} does not start with its vmid {}",
            name,
            m.hostname,
            m.vmid
        );
        assert!(
            m.hostname.ends_with(&format!("-{}", m.stack_name)),
            "stacks/{}: hostname {} does not end with its stack name",
            name,
            m.hostname
        );
        let expected = format!("10.10.10.{}/24", m.vmid - 100);
        assert_eq!(
            m.network.ip, expected,
            "stacks/{}: vmid {} means {}, not {}",
            name, m.vmid, expected, m.network.ip
        );
    }
}

/// Found by making this mistake while building the drill stack: a `sed` over
/// a copied config left `host: 117-app-drill` on a container called
/// `118-app-drill`. Nothing would have complained — the logs would simply
/// have arrived under another container's name, and the three dashboards
/// that group by host would have quietly lied. Twelve of these labels are
/// hand-copied and nothing checked a single one.
#[test]
fn every_promtail_label_names_the_container_it_actually_runs_on() {
    for (name, m) in compose_stacks() {
        let cfg = stacks_dir()
            .join(&name)
            .join("promtail")
            .join("promtail-config.yml");
        if !cfg.is_file() {
            continue;
        }
        let text = std::fs::read_to_string(&cfg).unwrap();
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if let Some(v) = t.strip_prefix("host: ") {
                assert_eq!(
                    v.trim(),
                    m.hostname,
                    "stacks/{}/promtail/promtail-config.yml:{} labels its logs {} \
                     while the container is {}",
                    name,
                    i + 1,
                    v.trim(),
                    m.hostname
                );
            }
            if let Some(v) = t.strip_prefix("stack: ") {
                assert_eq!(
                    v.trim(),
                    m.stack_name,
                    "stacks/{}/promtail/promtail-config.yml:{} labels its logs for stack {}",
                    name,
                    i + 1,
                    v.trim()
                );
            }
        }
    }
}

#[test]
fn every_app_listed_by_a_stack_has_a_compose_file_and_the_other_way_round() {
    for (name, m) in compose_stacks() {
        let dir = stacks_dir().join(&name);
        for app in &m.apps {
            let f = dir.join(app).join("docker-compose.yml");
            assert!(
                f.is_file(),
                "stacks/{} lists app {} and there is no {:?}",
                name,
                app,
                f
            );
        }
        // And nothing on disk that the manifest forgot: a compose file in a
        // directory nobody lists is a service that silently never deploys.
        for e in std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
            let p = e.path();
            if p.join("docker-compose.yml").is_file() {
                let app = p.file_name().unwrap().to_string_lossy().to_string();
                assert!(
                    m.apps.contains(&app),
                    "stacks/{} has a compose file for {} that the manifest does not list — \
                     it would never be deployed and nothing would say so",
                    name,
                    app
                );
            }
        }
    }
}

#[test]
fn every_mount_belongs_to_an_app_the_stack_actually_runs() {
    for (name, m) in compose_stacks() {
        for s in &m.storage {
            // `app` is optional in the schema; a mount without one belongs to
            // the stack rather than to a service, which is legal.
            let Some(app) = s.app.as_ref() else { continue };
            // A native stack (T5) runs systemd units rather than compose
            // apps, and its mounts are owned by those.
            assert!(
                m.apps.contains(app) || m.natives.contains(app),
                "stacks/{}: mount {} is owned by {}, which this stack neither runs as \
                 a compose app nor as a native service",
                name,
                s.host_path,
                app
            );
        }
    }
}

/// F187's shape, generalised: a stack cloning a template that is not there
/// rebuilds into nothing, and it fails at the moment the rebuild matters.
#[test]
fn every_template_is_one_of_the_golden_ones_that_exist() {
    for (name, m) in compose_stacks() {
        let t = m.lxc.template.trim_matches('"').to_string();
        let Some(vmid) = t.strip_prefix("clone:") else {
            continue;
        };
        assert!(
            ["997", "998"].contains(&vmid.trim()),
            "stacks/{} clones {} — 999 is the retired v1 image and anything else \
             does not exist on this host",
            name,
            t
        );
    }
}

/// The one that should have existed before F215 shipped.
///
/// On 2026-09-02 I added a `layer: container` to eleven promtail check files.
/// There is no such layer — the enum has network, process, application and
/// user_visible — so every one of those files failed to parse, and eleven of
/// the thirteen stacks could not be deployed at all. The whole suite was
/// green, CI was green, and the fault was found only because a drill deploy
/// refused to start. A check file is code that runs on the machine; it
/// belongs under the same test as the manifest beside it.
#[test]
fn every_check_file_on_disk_parses_as_the_deploy_would_read_it() {
    let dir = stacks_dir();
    let mut seen = 0usize;
    for (name, _) in compose_stacks() {
        let stack_dir = dir.join(&name);
        for e in std::fs::read_dir(&stack_dir)
            .unwrap()
            .filter_map(|e| e.ok())
        {
            let f = e.path().join("checks.yml");
            if !f.is_file() {
                continue;
            }
            seen += 1;
            let text = std::fs::read_to_string(&f).unwrap();
            let parsed: Result<ServiceChecks, _> = serde_yaml::from_str(&text);
            parsed.unwrap_or_else(|err| {
                panic!(
                    "stacks/{}/{}/checks.yml does not parse — this stack cannot be \
                     deployed at all: {}",
                    name,
                    e.path().file_name().unwrap().to_string_lossy(),
                    err
                )
            });
        }
    }
    assert!(
        seen >= 20,
        "found only {} check files — this test is looking in the wrong place",
        seen
    );
}

/// A systemd key in the wrong section is silently ignored, and the guarantee
/// it was written for simply is not there.
///
/// Found on 2026-09-02 by a drill: `StartLimitIntervalSec=0` sat in
/// `[Service]` in kyu's unit, where systemd prints "Unknown key … ignoring"
/// and carries on. The live hub was measured the same minute running
/// systemd's defaults instead — give up after 5 restarts in 10 s — which is
/// the opposite of what the comment above the line asked for, and exactly how
/// newsflash lost two hours of production the day before.
///
/// The three `StartLimit*` keys are the ones that move between sections
/// between systemd versions, so they are the ones worth pinning.
#[test]
fn no_unit_file_puts_a_start_limit_key_where_systemd_ignores_it() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().map(|x| x == "service").unwrap_or(false) {
                out.push(p);
            }
        }
    }
    let mut units = Vec::new();
    walk(&stacks_dir(), &mut units);
    assert!(!units.is_empty(), "no unit files found — wrong directory");
    for u in units {
        let text = std::fs::read_to_string(&u).unwrap();
        let mut section = String::new();
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.starts_with('[') {
                section = t.to_string();
            }
            if t.starts_with("StartLimit") && section != "[Unit]" {
                panic!(
                    "{}:{} puts {} in {} — systemd reads StartLimit* only in [Unit] and \
                     ignores it silently everywhere else, so the guarantee is not there",
                    u.display(),
                    i + 1,
                    t.split('=').next().unwrap_or(t),
                    section
                );
            }
        }
    }
}

/// G3 · the seeder's hand-written half, checked against the fleet.
///
/// The mechanical half of the watch list is generated and tested
/// (`monitors.rs`). The application half is a Python list in
/// `stacks/uptime/kuma-seeder/seed.py` — deliberately hand-written, because
/// whether a service answers on `/health` or `/ping` or `?strict=1` is
/// knowledge no manifest holds. What a manifest DOES hold is the address, and
/// that is exactly what went stale on 2026-09-01: a monitor reported Uptime
/// Kuma itself as down for eight hours because it still named the address
/// the service had left that morning (F157), and another named a stack that
/// no longer existed (F158).
///
/// So this does not try to test the seeder's knowledge. It tests the one
/// thing the repository can check: every internal address it points at
/// belongs to a stack that exists, at the IP that stack actually has, and the
/// name in front of the `·` is that stack.
mod kuma_seeder {
    use super::*;

    fn application_monitors() -> Vec<(String, String)> {
        let src = std::fs::read_to_string(
            stacks_dir()
                .join("uptime")
                .join("kuma-seeder")
                .join("seed.py"),
        )
        .expect("the seeder must be where this test says it is");
        let start = src
            .find("APPLICATION_MONITORS = [")
            .expect("the hand-written list must still be called APPLICATION_MONITORS");
        let body = &src[start..];
        let body = &body[..body.find("\n]").expect("unterminated list")];
        let mut out = Vec::new();
        for line in body.lines() {
            let t = line.trim();
            if !t.starts_with("(\"") {
                continue;
            }
            let mut parts = t.split('"').skip(1);
            let (Some(name), Some(_), Some(url)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            out.push((name.to_string(), url.to_string()));
        }
        assert!(
            out.len() >= 20,
            "parsed only {} monitors — the list changed shape and this test is reading \
             nothing, which is worse than failing",
            out.len()
        );
        out
    }

    #[test]
    fn every_internal_monitor_points_at_a_stack_that_exists_at_the_address_it_has() {
        let stacks = compose_stacks();
        for (name, url) in application_monitors() {
            let Some(rest) = url.strip_prefix("http://") else {
                continue; // external checks go through Cloudflare, not a stack
            };
            let host = rest.split(['/', ':']).next().unwrap_or("");
            if !host.starts_with("10.10.10.") {
                continue;
            }
            let owner = stacks
                .iter()
                .find(|(_, m)| m.network.ip.split('/').next() == Some(host));
            let (stack_dir, m) = owner.unwrap_or_else(|| {
                panic!(
                    "the seeder monitors '{}' at {}, and no stack in this repository has \
                     that address — this is exactly the shape of F157",
                    name, host
                )
            });
            let prefix = name.split(" · ").next().unwrap_or("");
            assert_eq!(
                prefix, m.stack_name,
                "the seeder calls {} '{}', but {} belongs to stack {} (directory {})",
                host, name, host, m.stack_name, stack_dir
            );
        }
    }

    /// A stack with no monitor at all is not automatically wrong — the
    /// mechanical half already pings every container. But a stack running
    /// services that answer on the network, with nothing in the application
    /// half, is worth knowing about, so the list of deliberate omissions is
    /// written down rather than assumed.
    #[test]
    fn every_stack_without_an_application_monitor_is_one_we_named() {
        const NO_APPLICATION_MONITOR: &[(&str, &str)] = &[
            (
                "registry",
                "a pull-through cache: the mechanical monitor covers reachability, and \
                 there is no application answer that means more than that",
            ),
            (
                "productivity",
                "supersync speaks its own protocol to its clients and has no health \
                 endpoint that answers without one",
            ),
            (
                "drill",
                "a throwaway container, created and destroyed in the same sitting",
            ),
        ];
        let watched: Vec<String> = application_monitors()
            .into_iter()
            .map(|(n, _)| n.split(" · ").next().unwrap_or("").to_string())
            .collect();
        for (name, m) in compose_stacks() {
            if watched.contains(&m.stack_name) {
                continue;
            }
            assert!(
                NO_APPLICATION_MONITOR.iter().any(|(s, _)| *s == name),
                "stack '{}' has no application monitor in the seeder and is not in the \
                 list of stacks we decided not to watch — add the monitor, or add the \
                 stack here with the reason",
                name
            );
        }
    }
}

/// gap-11 · the OPNsense syslog receiver is declared by the gateway stack and
/// by nothing else.
///
/// OPNsense is configured to send to 10.10.10.4:1514, which is the gateway
/// container; a receiver declared on any other stack would open a port
/// nothing sends to, and one missing from the gateway would silently end
/// remote logging at the next deploy — the state the fleet was in between
/// 2026-09-18 and this test.
#[test]
fn only_the_gateway_declares_the_opnsense_syslog_receiver() {
    for (name, m) in compose_stacks() {
        if name == "gateway" {
            let r = m
                .syslog_receivers
                .iter()
                .find(|r| r.host == "opnsense")
                .expect("stacks/gateway/lxc-compose.yml must declare the opnsense receiver");
            assert_eq!(
                r.listen, "0.0.0.0:1514",
                "OPNsense is configured to send here"
            );
            assert_eq!(r.protocol, "udp");
            assert_eq!(r.format, "rfc5424");
        } else {
            assert!(
                m.syslog_receivers.is_empty(),
                "{} declares a syslog receiver and nothing sends to it",
                name
            );
        }
    }
}

/// The validator refuses a receiver the shipper could not actually open —
/// Alloy runs as its own user, so a port below 1024 binds nothing and Alloy
/// merely logs it while the deploy reports success.
#[test]
fn a_receiver_the_shipper_could_not_open_is_refused_at_plan_time() {
    use homelab_core::manifest::SyslogReceiver;
    let (_, gw) = compose_stacks()
        .into_iter()
        .find(|(n, _)| n == "gateway")
        .expect("gateway stack");
    let mut bad = gw.clone();
    bad.syslog_receivers = vec![SyslogReceiver {
        host: "opnsense".into(),
        listen: "0.0.0.0:514".into(),
        protocol: "udp".into(),
        format: "rfc5424".into(),
    }];
    let err = validate_manifest(&bad).expect_err("port 514 cannot be bound unprivileged");
    let msg = err.to_string();
    assert!(msg.contains("514") && msg.contains("1024"), "{}", msg);

    let mut twice = gw.clone();
    twice
        .syslog_receivers
        .push(twice.syslog_receivers[0].clone());
    let err = validate_manifest(&twice).expect_err("two receivers on one address");
    assert!(err.to_string().contains("0.0.0.0:1514"), "{}", err);

    let mut odd = gw.clone();
    odd.syslog_receivers[0].protocol = "sctp".into();
    odd.syslog_receivers[0].format = "cef".into();
    let msg = validate_manifest(&odd)
        .expect_err("a protocol or format Alloy does not speak")
        .to_string();
    assert!(msg.contains("sctp") && msg.contains("cef"), "{}", msg);
}

/// Kenny, 2026-09-27: the watch list is declarative, all of it. A monitor no
/// file declares is removed (a hand-made one too: those belong in the file),
/// a monitor whose address changed is corrected, and a desired list that
/// suddenly shrinks by more than a quarter is refused as probably truncated.
/// Runs the seeder's own planning function with python3.
///
/// covers: step-21
#[test]
fn step_21_the_seeder_keeps_uptime_kuma_equal_to_the_files() {
    let dir = stacks_dir().join("uptime").join("kuma-seeder");
    let script = r#"
import json, seed
tag = [{"name": "homelab-seeder"}]
monitors = [
    {"id": 1, "name": "host · gateway", "hostname": "10.10.10.4", "tags": tag},
    {"id": 2, "name": "host · drill", "hostname": "10.10.10.19", "tags": []},
    {"id": 3, "name": "gateway · traefik", "url": "http://10.10.10.4:8080/ping", "tags": tag},
    {"id": 4, "name": "old · gone", "url": "http://10.10.10.99/", "tags": tag},
    {"id": 5, "name": "my own check", "url": "http://example/", "tags": []},
    {"id": 6, "name": "media · jellyfin", "url": "http://10.10.10.66:8096/health", "tags": []},
]
desired = {"host · gateway": "10.10.10.4", "gateway · traefik": "http://10.10.10.4:8080/ping",
           "media · jellyfin": "http://10.10.10.6:8096/health"}
desired.update({k: "x" for k in "abcdefghijklmn"})
to_tag, to_delete, refused, to_fix = seed.plan_owned(monitors, desired, True)
print(json.dumps([sorted(m["id"] for m in to_tag), sorted(m["id"] for m in to_delete), refused,
                  [(m["id"], w) for m, w in to_fix]]))
_, d2, r2, _ = seed.plan_owned(monitors, desired, False)
print(json.dumps([len(d2), r2 is not None]))
many = [{"id": i, "name": f"host · s{i}", "hostname": "h", "tags": tag} for i in range(10)]
_, d3, r3, _ = seed.plan_owned(many, {"host · s0": "h"}, True)
print(json.dumps([len(d3), r3 is not None]))
"#;
    let out = std::process::Command::new("python3")
        .arg("-c")
        .arg(script)
        .env("PYTHONPATH", &dir)
        // Never leave __pycache__ in the stack directory: the client refuses
        // a non-UTF-8 file in a stack, and the next deploy failed on it.
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("python3 must be available (it is in the CI image)");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0], r#"[[6], [2, 4, 5], null, [[6, "http://10.10.10.6:8096/health"]]]"#,
        "tag the declared untagged one, remove everything no file declares (a hand-made \
         monitor too, since 2026-09-27), and correct an address that changed"
    );
    assert_eq!(
        lines[1], "[0, true]",
        "without the generated list nothing is judged"
    );
    assert_eq!(
        lines[2], "[0, true]",
        "nine of ten gone at once is refused as a truncated list"
    );
}

/// Expert panel 2026-09-27 (crowdsec-blind-to-internet). Measured on CT 104
/// the same day: every request for a kp-soft.dev name in the last two access
/// logs had `ClientHost` 172.18.0.1 (462 of 464), the docker bridge the
/// tunnel's requests arrive through. CrowdSec whitelists 172.16.0.0/12, so it
/// could never ban anyone on the internet. Traefik and the bouncer now take
/// the visitor's address from the header the tunnel sets, but only from
/// docker's own range, which only containers on CT 104 can send from.
#[test]
fn traefik_and_the_bouncer_read_the_visitor_address_from_the_tunnel() {
    let compose =
        std::fs::read_to_string(stacks_dir().join("gateway/traefik/docker-compose.yml")).unwrap();
    assert!(
        compose.contains("--entrypoints.web.forwardedHeaders.trustedIPs=172.16.0.0/12"),
        "Traefik must trust X-Forwarded-For from the tunnel only"
    );
    assert!(compose.contains("plugin.bouncer.forwardedheaderstrustedips=172.16.0.0/12"));
    assert!(compose.contains("plugin.bouncer.forwardedheaderscustomname=CF-Connecting-IP"));
    assert!(
        !compose.contains("forwardedHeaders.insecure"),
        "trusting every sender would let anyone on the LAN pick their own address"
    );
}

/// Expert panel 2026-09-27: the metrics stack could fill pve's root with no
/// size cap (tsdb-on-pve-root-no-cap: 6.33 GB growing 0.22 GB a day on
/// /dev/mapper/pve-root), a full pve root arrived as 14 alerts named after
/// containers (disk-alert-fanout-wrong-host), rules on vanished series never
/// fired (alert-rules-blind-to-missing-data), and nothing watched delivery
/// (alert-chain-unwatched).
#[test]
fn the_metrics_stack_caps_its_disk_and_alerts_on_missing_data() {
    let dir = stacks_dir().join("metrics/prometheus");
    let compose = std::fs::read_to_string(dir.join("docker-compose.yml")).unwrap();
    assert!(compose.contains("--storage.tsdb.retention.size=15GB"));
    let rules = std::fs::read_to_string(dir.join("rules/homelab.rules.yml")).unwrap();
    for alert in [
        "SmartCollectorStale",
        "DriveMissing",
        "ZpoolNotOnline",
        "TargetDown",
        "AlmanacJournalUnreadable",
        "SystemdUnitFailed",
        "AlertDeliveryFailing",
        "HypervisorRootFillingUp",
    ] {
        assert!(rules.contains(&format!("alert: {alert}")), "{alert}");
    }
    assert!(
        rules.contains("max by (device)"),
        "one filesystem alert per device, not per bind mount"
    );
    let prom = std::fs::read_to_string(dir.join("prometheus.yml")).unwrap();
    assert!(
        prom.contains("job_name: alertmanager"),
        "AlertDeliveryFailing reads Alertmanager's own counters"
    );
}

/// Expert panel 2026-09-27 (phone-path-loses-information): HTTPSwitchboard
/// reshapes an Alertmanager group from its first alert, so a group of two
/// lost the second. One alert per group closes that.
#[test]
fn alertmanager_sends_one_alert_per_notification() {
    let am = std::fs::read_to_string(stacks_dir().join("metrics/alertmanager/alertmanager.yml"))
        .unwrap();
    assert!(am.contains("group_by: [\"...\"]"), "{am}");
}

/// Expert panel 2026-09-27, the Gewenst list: pve-exporter-forwards-token,
/// zfs-metrics-duplicated, traefik-no-metrics, access-log-query-tokens,
/// pve-exporter-data-unused, kyu-backlog-invisible.
#[test]
fn the_gewenst_metrics_and_gateway_changes_are_in_the_stack_files() {
    let read = |p: &str| std::fs::read_to_string(stacks_dir().join(p)).unwrap();
    let pve = read("metrics/pve-exporter/docker-compose.yml");
    assert!(
        pve.contains("\"127.0.0.1:9221:9221\""),
        "the exporter hands its token to whoever names a target; only this CT may ask"
    );
    let prom = read("metrics/prometheus/prometheus.yml");
    assert!(prom.contains("job_name: traefik"));
    assert!(
        prom.contains("node_zfs_.*"),
        "the pools are counted once, on pve, not by every container"
    );
    let traefik = read("gateway/traefik/docker-compose.yml");
    assert!(traefik.contains("--metrics.prometheus=true"));
    let goaccess = read("gateway/goaccess/docker-compose.yml");
    assert!(goaccess.contains("--no-query-string"));
    let rules = read("metrics/prometheus/rules/homelab.rules.yml");
    for alert in [
        "PveStorageAlmostFull",
        "KyuBacklogGrowing",
        "TraefikServerErrors",
    ] {
        assert!(rules.contains(&format!("alert: {alert}")), "{alert}");
    }
}

/// Expert panel 2026-09-27 (kuma-coverage-and-seeder-drift): the public
/// SuperSync endpoint and the registry cache every pull goes through had no
/// Uptime Kuma monitor.
#[test]
fn supersync_and_the_registry_cache_are_monitored() {
    let seed = std::fs::read_to_string(stacks_dir().join("uptime/kuma-seeder/seed.py")).unwrap();
    assert!(seed.contains("\"http://10.10.10.11:1900/health\""));
    assert!(seed.contains("\"http://10.10.10.17:5000/v2/\""));
}

/// Expert panel 2026-09-27 (never-decreases-false-positives). Measured the
/// same evening: the receiver count grepped JSON `"name":` fields and read 0
/// with one receiver configured, so it could never decrease; `count(up==1)`
/// reads low for the first scrape after a restart; the dashboard count drops
/// by design when a stack is destroyed.
#[test]
fn service_checks_read_what_they_claim_to_count() {
    let read = |p: &str| std::fs::read_to_string(stacks_dir().join(p)).unwrap();
    let am = read("metrics/alertmanager/checks.yml");
    assert!(
        am.contains("- name: "),
        "receivers are counted in the YAML config"
    );
    assert!(!am.contains("\"name\":\"[a-z0-9_-]*\""));
    let prom = read("metrics/prometheus/checks.yml");
    assert!(
        prom.contains("max_over_time(up"),
        "up at any moment of the last two minutes"
    );
    let grafana = read("metrics/grafana/checks.yml");
    let dash = &grafana[grafana.find("name: \"dashboards\"").unwrap()..];
    let dash = &dash[..dash.find("layer:").unwrap()];
    assert!(dash.contains("expect: must_be_present"), "{dash}");
}

/// Expert panel 2026-09-27 (dashboards-two-sources-ui-edits-lost): an edit in
/// the browser was accepted and then lost at the next provisioning, and a
/// second copy of the provisioning tree sat under captured/.
#[test]
fn grafana_refuses_browser_edits_it_would_lose() {
    let p = std::fs::read_to_string(
        stacks_dir().join("metrics/grafana/provisioning/dashboards/dashboards.yaml"),
    )
    .unwrap();
    assert!(p.contains("allowUiUpdates: false"), "{p}");
    assert!(!stacks_dir()
        .parent()
        .unwrap()
        .join("captured/gateway/grafana/provisioning")
        .exists());
}

/// Kenny's triage answer 2026-09-27 (backup-pause-stops-monitoring: "Meetdata
/// niet back-uppen, alleen configuratie"). The nightly backup stopped
/// Prometheus, Alertmanager and Loki (measured: both metrics services
/// restarted at 02:11 UTC and Uptime Kuma logged them down) to copy data that
/// is regenerable or not worth restoring; their configuration is in this
/// repository.
#[test]
fn the_monitoring_data_is_not_backed_up_and_the_services_are_not_paused() {
    let read = |p: &str| std::fs::read_to_string(stacks_dir().join(p)).unwrap();
    for (stack, dir) in [
        (
            "metrics/lxc-compose.yml",
            "/appdata/metrics/prometheus-config",
        ),
        ("metrics/lxc-compose.yml", "/appdata/metrics/loki-config"),
    ] {
        let s = read(stack);
        let at = s.find(&format!("host_path: {dir}")).unwrap();
        let block = &s[at..at + s[at..].find("app:").unwrap()];
        assert!(block.contains("no_backup:"), "{dir}: {block}");
    }
    for compose in [
        "metrics/prometheus/docker-compose.yml",
        "metrics/alertmanager/docker-compose.yml",
        "metrics/loki/docker-compose.yml",
    ] {
        assert!(!read(compose).contains("backup.pause=true"), "{compose}");
    }
}

/// Kenny's triage answer 2026-09-27 (gateway-shared-no-limits: "Loki en
/// Grafana naar CT 113 verhuizen"). A Loki query storm competed with Traefik
/// in one cgroup, and when the gateway went down the logs and dashboards that
/// would explain the outage went down with it. Both now run on the metrics
/// stack, beside the Prometheus they already read, and every address that
/// named them on CT 104 follows.
///
/// covers: fix-90
#[test]
fn fix_90_loki_and_grafana_run_on_the_metrics_stack() {
    let read = |p: &str| std::fs::read_to_string(stacks_dir().join(p)).unwrap();
    let stacks = compose_stacks();
    let get = |n: &str| stacks.iter().find(|(s, _)| s == n).unwrap().1.clone();
    let (metrics, gateway) = (get("metrics"), get("gateway"));
    for app in ["loki", "grafana"] {
        assert!(metrics.apps.iter().any(|a| a == app), "metrics runs {app}");
        assert!(
            !gateway.apps.iter().any(|a| a == app),
            "gateway keeps {app}"
        );
        assert!(!stacks_dir().join("gateway").join(app).exists(), "{app}");
        assert!(stacks_dir().join("metrics").join(app).is_dir(), "{app}");
    }
    let mount = |dir: &str| {
        metrics
            .storage
            .iter()
            .find(|s| s.host_path == dir)
            .unwrap_or_else(|| panic!("metrics declares {dir}"))
            .clone()
    };
    // The same ownership and backup decisions as on the gateway: Loki's
    // chunks are not backed up (fix-81), Grafana's database is.
    let loki = mount("/appdata/metrics/loki-config");
    assert_eq!(
        (loki.app.as_deref(), loki.host_owner_uid),
        (Some("loki"), Some(110001))
    );
    assert!(loki.no_backup.is_some(), "fix-81: Loki's chunks stay out");
    let grafana = mount("/appdata/metrics/grafana-config");
    assert_eq!(
        (grafana.app.as_deref(), grafana.host_owner_uid),
        (Some("grafana"), Some(101000))
    );
    assert!(grafana.no_backup.is_none() && !grafana.no_data);
    assert!(!gateway
        .storage
        .iter()
        .any(|s| s.host_path.contains("loki") || s.host_path.contains("grafana")));
    // Measured 2026-09-27 on CT 104 and CT 113 (14-day peaks from cadvisor):
    // metrics 415 MB, loki 342 MB, grafana 527 MB. 2560 MB keeps about half
    // free at those peaks; 1024 would have been full.
    assert!(
        metrics.resources.memory_mb >= 2560,
        "{}",
        metrics.resources.memory_mb
    );
    assert!(read("metrics/lxc-compose.yml").contains("latch_secrets: [pve-exporter, grafana]"));
    assert!(read("gateway/lxc-compose.yml").contains("latch_secrets: [traefik, cloudflared]"));
    // The route follows the backend; a name must never be routed twice.
    let routes = read("metrics/traefik-routes.yml");
    assert!(routes.contains("Host(`grafana.kp-soft.dev`)"), "{routes}");
    assert!(routes.contains("http://10.10.10.13:3000"), "{routes}");
    assert!(!read("gateway/traefik-routes.yml").contains("grafana"));
    // Inside the stack Grafana reaches both datasources by container name.
    let ds = "metrics/grafana/provisioning/datasources";
    assert!(read(&format!("{ds}/loki.yaml")).contains("url: http://loki:3100"));
    assert!(read(&format!("{ds}/prometheus.yaml")).contains("url: http://prometheus:9090"));
    let compose = read("metrics/grafana/docker-compose.yml");
    assert!(compose.contains("/appdata/metrics/grafana-config:/var/lib/grafana"));
    assert!(compose.contains("metrics_net") && !compose.contains("gateway_net"));
    assert!(read("metrics/grafana/checks.yml").contains("/opt/metrics/grafana/.env"));
    let loki_compose = read("metrics/loki/docker-compose.yml");
    assert!(loki_compose.contains("/appdata/metrics/loki-config/data:/loki"));
    assert!(loki_compose.contains("metrics_net") && !loki_compose.contains("gateway_net"));
    let seed = read("uptime/kuma-seeder/seed.py");
    assert!(seed.contains("(\"metrics · grafana\", \"http://10.10.10.13:3000/api/health\", OK)"));
    assert!(seed.contains("(\"metrics · loki\", \"http://10.10.10.13:3100/ready\", OK)"));
    assert!(read("home/homepage/services-overlay.yml").contains("url: http://10.10.10.13:3000"));
    // Nothing in any stack still names the old addresses.
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&stacks_dir(), &mut files);
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for old in ["10.10.10.4:3100", "10.10.10.4:3000"] {
            assert!(!text.contains(old), "{} still names {old}", f.display());
        }
    }
}
