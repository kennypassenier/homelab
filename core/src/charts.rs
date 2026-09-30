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
    ]
}
