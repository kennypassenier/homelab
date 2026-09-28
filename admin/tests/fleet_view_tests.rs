//! feat-overview-1 (skeleton cut): the fleet view the browser receives.

use homelab_admin::core::fleet::fleet_view;
use homelab_proto::{FleetState, HostView, StackView};

fn stack(name: &str, vmid: u16, online: bool, enabled: bool, apps: &[bool]) -> StackView {
    serde_json::from_value(serde_json::json!({
        "name": name, "vmid": vmid, "hostname": name,
        "apps": apps.iter().enumerate().map(|(i, r)| serde_json::json!({"name": format!("a{i}"), "running": r, "restarts": 0})).collect::<Vec<_>>(),
        "drift": false, "env_sealed": true, "online": online, "enabled": enabled,
    }))
    .expect("a StackView")
}

fn host() -> HostView {
    serde_json::from_value(serde_json::json!({
        "name": "pve", "cpu_pct": 7, "ram_pct": 40, "disk_pct": 31,
        "tls_fingerprint": "", "ram_total_mb": 32000, "ram_used_mb": 17000,
    }))
    .expect("a HostView")
}

#[test]
fn feat_overview_1_stacks_come_sorted_by_vmid_with_counts() {
    let state = FleetState {
        status_measured_at: None,
        host: host(),
        stacks: vec![
            stack("media", 106, true, true, &[true, true]),
            stack("gateway", 104, true, true, &[true, false]),
            stack("drill", 119, false, false, &[]),
        ],
    };
    let v = fleet_view(&state, 1_000);
    let order: Vec<u16> = v.stacks.iter().map(|s| s.vmid).collect();
    assert_eq!(order, vec![104, 106, 119]);
    assert_eq!(v.counts.stacks, 3);
    assert_eq!(v.counts.online, 2);
    assert_eq!(v.counts.parked, 1);
    assert_eq!(v.stacks[0].apps_running, 1);
    assert_eq!(v.stacks[0].apps_total, 2);
    assert_eq!(v.measured_at, 1_000);
    assert_eq!(v.host.ram_used_mb, 17000);
    // feat-stacks-1: the stack page reads its apps from the same view.
    let names: Vec<&str> = v.stacks[0].apps.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["a0", "a1"]);
    assert!(!v.stacks[0].apps[1].running);
    assert_eq!(v.stacks[0].hostname, "gateway");
    assert_eq!(v.stacks[0].uptime_s, None);
    assert_eq!(v.stacks[0].applied_source, None);
}

#[test]
fn feat_overview_1_an_empty_fleet_is_a_view_not_an_error() {
    let v = fleet_view(
        &FleetState {
            status_measured_at: None,
            host: host(),
            stacks: vec![],
        },
        5,
    );
    assert_eq!(v.counts.stacks, 0);
    assert!(v.stacks.is_empty());
}
