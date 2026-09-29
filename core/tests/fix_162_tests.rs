//! fix-162: CT 120's firewall (policy_in DROP, in force since the rollout of
//! 2026-09-29 ~02:45) let Traefik, Uptime Kuma and Kenny's desktop in but
//! not Prometheus on CT 113, so node-exporter on 10.10.10.20:9100 timed out
//! and HostDown fired from 00:43 UTC while the dashboard itself answered.
//! kp-soft's firewall had the rule; admin's did not.

use std::path::Path;

/// Every stack whose container firewall drops inbound traffic and that is
/// measured lets Prometheus (CT 113, 10.10.10.13) reach node-exporter on 9100.
#[test]
fn fix_162_every_dropping_firewall_lets_prometheus_scrape_node_exporter() {
    let stacks = Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks");
    let mut checked = Vec::new();
    for e in std::fs::read_dir(&stacks).unwrap() {
        let dir = e.unwrap().path();
        let file = dir.join("lxc-compose.yml");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let doc: serde_yaml::Value = serde_yaml::from_str(&text).unwrap();
        let Some(fw) = doc.get("firewall") else {
            continue;
        };
        if fw.get("enabled").and_then(|v| v.as_bool()) != Some(true)
            || fw.get("policy_in").and_then(|v| v.as_str()) != Some("DROP")
        {
            continue;
        }
        let service = std::fs::read_to_string(dir.join("service.yml")).unwrap_or_default();
        if service.lines().any(|l| l.trim() == "metrics: false") {
            continue;
        }
        let rules = fw
            .get("rules")
            .and_then(|r| r.as_sequence())
            .cloned()
            .unwrap_or_default();
        let lets_in = rules.iter().any(|r| {
            let s = |k: &str| {
                r.get(k).and_then(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .or_else(|| v.as_u64().map(|n| n.to_string()))
                })
            };
            s("dir").as_deref() == Some("in")
                && s("action").as_deref() == Some("ACCEPT")
                && s("source").as_deref() == Some("10.10.10.13")
                && s("dport").is_some_and(|p| p.split(',').any(|x| x.trim() == "9100"))
        });
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            lets_in,
            "{name}: the firewall drops inbound traffic but has no `in ACCEPT source 10.10.10.13 dport 9100` for Prometheus"
        );
        checked.push(name);
    }
    assert!(checked.iter().any(|n| n == "kp-soft"), "{:?}", checked);
}
