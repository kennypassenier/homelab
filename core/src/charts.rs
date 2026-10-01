//! replace-grafana (Kenny, 2026-09-30: "liefst Grafana vervangen door ons
//! dashboard zodat ik maar 1 plek heb om naar te kijken"): the panels the
//! dashboard draws, as Prometheus queries.
//!
//! Platform knowledge only: what homelab itself installs in every container
//! (node_exporter, cadvisor beside docker) and on the hypervisor, labelled
//! by the targets homelab writes (`stack`, `host`). No app is named; a stack
//! with no docker app simply gets empty per-app panels, which the page hides.

use serde::Serialize;

/// How a panel's numbers read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Cores,
    Bytes,
    Percent,
    Celsius,
    /// 1 = yes, 0 = no.
    Flag,
    /// A plain count: a load average, a number of sectors, hours, restarts.
    Count,
}

/// One chart: a title, a query and how its numbers read. `legend` names the
/// label whose value tells the series apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Panel {
    pub title: String,
    pub query: String,
    pub unit: Unit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<String>,
}

fn p(title: &str, query: String, unit: Unit, legend: Option<&str>) -> Panel {
    Panel {
        title: title.into(),
        query,
        unit,
        legend: legend.map(String::from),
    }
}

/// A stack's panels: the container as a whole (node_exporter), then per
/// docker app (cadvisor). `stack` is the label homelab's target files set.
pub fn stack_panels(stack: &str) -> Vec<Panel> {
    let s = stack.replace('"', "");
    vec![
        p(
            // Kenny, 2026-09-30 ("219m" read like a Kubernetes label nobody
            // in this house speaks): node_exporter inside the container only
            // ever sees the cores Proxmox gave it, so averaging over them
            // (rather than summing) is already a percentage of the stack's
            // own cores, with no extra lookup of how many that is.
            "CPU (whole container)",
            format!("100 * avg(rate(node_cpu_seconds_total{{stack=\"{s}\",mode!=\"idle\"}}[5m]))"),
            Unit::Percent,
            None,
        ),
        p(
            "Memory used (whole container)",
            format!(
                "node_memory_MemTotal_bytes{{stack=\"{s}\"}} - node_memory_MemAvailable_bytes{{stack=\"{s}\"}}"
            ),
            Unit::Bytes,
            None,
        ),
        p(
            "Disk used (root filesystem)",
            format!(
                "100 - (node_filesystem_avail_bytes{{stack=\"{s}\",mountpoint=\"/\"}} / node_filesystem_size_bytes{{stack=\"{s}\",mountpoint=\"/\"}} * 100)"
            ),
            Unit::Percent,
            None,
        ),
        p(
            "CPU per app",
            // Percent of the container's own cores: node_exporter inside an
            // LXC sees only the cores Proxmox gave it (measured 2026-09-30:
            // CT 120 counts 1, CT 106 counts 6).
            format!("100 * sum by (name) (rate(container_cpu_usage_seconds_total{{stack=\"{s}\",name!=\"\"}}[5m])) / scalar(count(node_cpu_seconds_total{{stack=\"{s}\",mode=\"idle\"}}))"),
            Unit::Percent,
            Some("name"),
        ),
        p(
            "Memory per app",
            format!("sum by (name) (container_memory_usage_bytes{{stack=\"{s}\",name!=\"\"}})"),
            Unit::Bytes,
            Some("name"),
        ),
        p(
            // replace-grafana parity (2026-10-01): Grafana's "Disk writes
            // per container" panel, scoped to this stack like every other
            // per-app panel here rather than fleet-wide.
            "Disk writes per app",
            format!(
                "sum by (name) (rate(container_fs_writes_bytes_total{{stack=\"{s}\",name!=\"\"}}[5m]))"
            ),
            Unit::Bytes,
            Some("name"),
        ),
        p(
            "Network in per app",
            format!(
                "sum by (name) (rate(container_network_receive_bytes_total{{stack=\"{s}\",name!=\"\"}}[5m]))"
            ),
            Unit::Bytes,
            Some("name"),
        ),
        p(
            // Grafana's "Container restarts" panel: how many times cadvisor
            // saw this app's start time change in the last hour.
            "Restarts (last hour)",
            format!(
                "sum by (name) (changes(container_start_time_seconds{{stack=\"{s}\",name!=\"\"}}[1h]))"
            ),
            Unit::Count,
            Some("name"),
        ),
    ]
}

/// feat-overview-11 (capacity map): CPU, memory and disk for every stack at
/// once, one query per metric across the whole fleet (a regex over
/// `stack=~"a|b|c"`) rather than one call per stack — the same panels
/// [`stack_panels`] draws per container, read together so the capacity map
/// costs three Prometheus calls, not `3 * len(stacks)`.
pub fn fleet_capacity_panels(stacks: &[String]) -> Vec<Panel> {
    let list = stacks
        .iter()
        .map(|s| s.replace('"', ""))
        .collect::<Vec<_>>()
        .join("|");
    vec![
        p(
            "CPU used",
            format!("100 * avg by (stack) (rate(node_cpu_seconds_total{{stack=~\"{list}\",mode!=\"idle\"}}[5m]))"),
            Unit::Percent,
            Some("stack"),
        ),
        p(
            "Memory used",
            format!(
                "(node_memory_MemTotal_bytes{{stack=~\"{list}\"}} - node_memory_MemAvailable_bytes{{stack=~\"{list}\"}}) / node_memory_MemTotal_bytes{{stack=~\"{list}\"}} * 100"
            ),
            Unit::Percent,
            Some("stack"),
        ),
        p(
            "Disk used (root filesystem)",
            format!(
                "100 - (node_filesystem_avail_bytes{{stack=~\"{list}\",mountpoint=\"/\"}} / node_filesystem_size_bytes{{stack=~\"{list}\",mountpoint=\"/\"}} * 100)"
            ),
            Unit::Percent,
            Some("stack"),
        ),
    ]
}

/// feat-overview-12 (disk-growth prediction): percent-used history for the
/// root filesystem of every stack at once, over `range` — the series
/// [`crate::diskgrowth::fit`] reads. One regex query across the fleet, like
/// [`fleet_capacity_panels`].
pub fn fleet_disk_growth_query(stacks: &[String]) -> Panel {
    let list = stacks
        .iter()
        .map(|s| s.replace('"', ""))
        .collect::<Vec<_>>()
        .join("|");
    p(
        "Disk used (root filesystem)",
        format!(
            "100 - (node_filesystem_avail_bytes{{stack=~\"{list}\",mountpoint=\"/\"}} / node_filesystem_size_bytes{{stack=~\"{list}\",mountpoint=\"/\"}} * 100)"
        ),
        Unit::Percent,
        Some("stack"),
    )
}

/// feat-overview-12: the same prediction, for every filesystem of the
/// hypervisor itself (where capacity usually matters most: a ZFS pool, not
/// one container's root disk).
pub fn host_disk_growth_query(host: &str) -> Panel {
    let h = host.replace('"', "");
    p(
        "Disk used per filesystem",
        format!(
            "100 - (node_filesystem_avail_bytes{{host=\"{h}\",fstype=~\"zfs|ext4|xfs\",mountpoint!~\".*/subvol-.*\"}} / node_filesystem_size_bytes{{host=\"{h}\",fstype=~\"zfs|ext4|xfs\",mountpoint!~\".*/subvol-.*\"}} * 100)"
        ),
        Unit::Percent,
        Some("mountpoint"),
    )
}

/// feat-firewall-3 (measured traffic on the topology): total network
/// throughput per stack, bytes/s, received and transmitted. There is no
/// per-neighbour flow metric in this fleet (node_exporter counts a
/// container's interface as a whole, not by remote address), so the
/// topology shows each node's own measured traffic rather than claiming a
/// precision the data does not have — a decision recorded in
/// docs/admin/REALIZATION_PLAN.md's visuals note.
pub fn fleet_traffic_panels(stacks: &[String]) -> Vec<Panel> {
    let list = stacks
        .iter()
        .map(|s| s.replace('"', ""))
        .collect::<Vec<_>>()
        .join("|");
    vec![
        p(
            "Received",
            format!(
                "sum by (stack) (rate(node_network_receive_bytes_total{{stack=~\"{list}\",device!=\"lo\"}}[5m]))"
            ),
            Unit::Bytes,
            Some("stack"),
        ),
        p(
            "Transmitted",
            format!(
                "sum by (stack) (rate(node_network_transmit_bytes_total{{stack=~\"{list}\",device!=\"lo\"}}[5m]))"
            ),
            Unit::Bytes,
            Some("stack"),
        ),
    ]
}

/// The hypervisor's panels: node_exporter on the host (`host="pve"` in
/// prometheus.yml's static target is the only host label it sets; `host`
/// here is that value), and every guest's memory as Proxmox counts it.
pub fn host_panels(host: &str) -> Vec<Panel> {
    let h = host.replace('"', "");
    vec![
        p(
            "CPU used",
            format!("100 * (1 - avg(rate(node_cpu_seconds_total{{host=\"{h}\",mode=\"idle\"}}[5m])))"),
            Unit::Percent,
            None,
        ),
        p(
            "Memory used",
            format!(
                "node_memory_MemTotal_bytes{{host=\"{h}\"}} - node_memory_MemAvailable_bytes{{host=\"{h}\"}}"
            ),
            Unit::Bytes,
            None,
        ),
        p(
            "Disk used per filesystem",
            format!(
                "100 - (node_filesystem_avail_bytes{{host=\"{h}\",fstype=~\"zfs|ext4|xfs\",mountpoint!~\".*/subvol-.*\"}} / node_filesystem_size_bytes{{host=\"{h}\",fstype=~\"zfs|ext4|xfs\",mountpoint!~\".*/subvol-.*\"}} * 100)"
            ),
            Unit::Percent,
            Some("mountpoint"),
        ),
        p(
            "Temperature (hottest sensor per chip)",
            format!("max by (chip) (node_hwmon_temp_celsius{{host=\"{h}\"}})"),
            Unit::Celsius,
            Some("chip"),
        ),
        p(
            "Drive health (SMART, 1 = ok)",
            "smart_device_health_ok".to_string(),
            Unit::Flag,
            Some("device"),
        ),
        p(
            "Memory per container (as Proxmox counts it)",
            "sum by (id) (pve_memory_usage_bytes{id=~\"lxc/.*\"})".to_string(),
            Unit::Bytes,
            Some("id"),
        ),
        p(
            // replace-grafana parity (2026-10-01): "Network in per host".
            "Network in",
            format!(
                "sum by (device) (rate(node_network_receive_bytes_total{{host=\"{h}\",device!~\"lo|veth.*|docker.*|br-.*\"}}[5m]))"
            ),
            Unit::Bytes,
            Some("device"),
        ),
        p(
            // Grafana's "Uptime and load" panel; uptime itself is already
            // implicit in a host that answers at all, so this carries the
            // load figure, which isn't derivable from anything else shown.
            "Load average (5 min)",
            format!("node_load5{{host=\"{h}\"}}"),
            Unit::Count,
            None,
        ),
        p(
            // Distinct from "Temperature (hottest sensor per chip)" above:
            // that's the motherboard/CPU sensors, this is the drives'.
            "Drive temperature",
            "smart_device_temperature_celsius".to_string(),
            Unit::Celsius,
            Some("device"),
        ),
        p(
            "Drive pending sectors",
            "smart_device_pending_sectors".to_string(),
            Unit::Count,
            Some("device"),
        ),
        p(
            "Drive reallocated sectors",
            "smart_device_reallocated_sectors".to_string(),
            Unit::Count,
            Some("device"),
        ),
        p(
            "Drive power-on hours",
            "smart_device_power_on_hours".to_string(),
            Unit::Count,
            Some("device"),
        ),
    ]
}

#[cfg(test)]
mod visuals_query_tests {
    use super::*;

    #[test]
    fn fleet_capacity_panels_covers_cpu_memory_disk_with_one_query_each() {
        let stacks = vec!["gateway".to_string(), "media".to_string()];
        let panels = fleet_capacity_panels(&stacks);
        assert_eq!(panels.len(), 3);
        for panel in &panels {
            assert!(panel.query.contains("gateway|media"), "{}", panel.query);
            assert_eq!(panel.legend.as_deref(), Some("stack"));
            assert_eq!(panel.unit, Unit::Percent);
        }
    }

    #[test]
    fn fleet_disk_growth_query_is_a_percent_query_with_quotes_stripped() {
        let stacks = vec!["a\"b".to_string(), "c".to_string()];
        let panel = fleet_disk_growth_query(&stacks);
        assert!(
            !panel.query.contains("a b|c"),
            "quote should be dropped not replaced with space"
        );
        assert!(panel.query.contains("ab|c"), "{}", panel.query);
        assert_eq!(panel.unit, Unit::Percent);
    }

    #[test]
    fn host_disk_growth_query_excludes_lxc_subvolumes() {
        let panel = host_disk_growth_query("pve");
        assert!(panel.query.contains("subvol"));
        assert!(panel.query.contains("host=\"pve\""));
    }

    #[test]
    fn fleet_traffic_panels_has_received_and_transmitted_per_stack() {
        let stacks = vec!["gateway".to_string()];
        let panels = fleet_traffic_panels(&stacks);
        assert_eq!(panels.len(), 2);
        assert!(panels[0].query.contains("receive"));
        assert!(panels[1].query.contains("transmit"));
        assert!(panels.iter().all(|p| p.legend.as_deref() == Some("stack")));
    }
}
