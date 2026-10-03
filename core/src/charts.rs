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

/// One chart: a title, a one-sentence description of what it shows and how
/// to read it (fix-220, rule 8: "ik moet niet raden naar wat een functie
/// doet"), a query and how its numbers read. `legend` names the label whose
/// value tells the series apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Panel {
    pub title: String,
    pub desc: String,
    pub query: String,
    pub unit: Unit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<String>,
}

fn p(title: &str, desc: &str, query: String, unit: Unit, legend: Option<&str>) -> Panel {
    Panel {
        title: title.into(),
        desc: desc.into(),
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
            "CPU (whole container)",
            // Kenny, 2026-09-30 ("219m" read like a Kubernetes label nobody
            // in this house speaks): node_exporter inside the container only
            // ever sees the cores Proxmox gave it, so averaging over them
            // (rather than summing) is already a percentage of the stack's
            // own cores, with no extra lookup of how many that is.
            "How busy this container's own CPU allowance is, averaged over the \
             last 5 minutes; 100% means it is using every core Proxmox gave it.",
            format!("100 * avg(rate(node_cpu_seconds_total{{stack=\"{s}\",mode!=\"idle\"}}[5m]))"),
            Unit::Percent,
            None,
        ),
        p(
            "Memory used (whole container)",
            "How much RAM this container is actually using right now, out of \
             what Proxmox gave it.",
            format!(
                "node_memory_MemTotal_bytes{{stack=\"{s}\"}} - node_memory_MemAvailable_bytes{{stack=\"{s}\"}}"
            ),
            Unit::Bytes,
            None,
        ),
        p(
            "Disk used (root filesystem)",
            "How full this container's own disk is.",
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
            "Each app's own share of this container's CPU allowance, so a busy \
             app inside a shared stack can be told apart from the others.",
            format!(
                "100 * sum by (name) (rate(container_cpu_usage_seconds_total{{stack=\"{s}\",name!=\"\"}}[5m])) / scalar(count(node_cpu_seconds_total{{stack=\"{s}\",mode=\"idle\"}}))"
            ),
            Unit::Percent,
            Some("name"),
        ),
        p(
            "Memory per app",
            "Each app's own RAM use inside this container.",
            format!("sum by (name) (container_memory_usage_bytes{{stack=\"{s}\",name!=\"\"}})"),
            Unit::Bytes,
            Some("name"),
        ),
        p(
            // replace-grafana parity (2026-10-01): Grafana's "Disk writes
            // per container" panel, scoped to this stack like every other
            // per-app panel here rather than fleet-wide.
            "Disk writes per app",
            "How fast each app is writing to disk, so a runaway log or \
             database can be spotted.",
            format!(
                "sum by (name) (rate(container_fs_writes_bytes_total{{stack=\"{s}\",name!=\"\"}}[5m]))"
            ),
            Unit::Bytes,
            Some("name"),
        ),
        p(
            "Network in per app",
            "How much network traffic each app inside this container is \
             receiving.",
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
            "How many times each app restarted in the last hour; a healthy \
             app reads 0.",
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
            "How busy each stack's own CPU allowance is right now, side by \
             side across the whole fleet.",
            format!(
                "100 * avg by (stack) (rate(node_cpu_seconds_total{{stack=~\"{list}\",mode!=\"idle\"}}[5m]))"
            ),
            Unit::Percent,
            Some("stack"),
        ),
        p(
            "Memory used",
            "How full each stack's RAM allowance is right now, side by side \
             across the whole fleet.",
            format!(
                "(node_memory_MemTotal_bytes{{stack=~\"{list}\"}} - node_memory_MemAvailable_bytes{{stack=~\"{list}\"}}) / node_memory_MemTotal_bytes{{stack=~\"{list}\"}} * 100"
            ),
            Unit::Percent,
            Some("stack"),
        ),
        p(
            "Disk used (root filesystem)",
            "How full each stack's own disk is right now, side by side \
             across the whole fleet.",
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
        "How each stack's disk has filled up over the window, fitted forward \
         to warn before it runs out.",
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
        "How each of the hypervisor's own filesystems has filled up over the \
         window, fitted forward to warn before it runs out.",
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
            "How much network traffic each stack is receiving right now, \
             side by side across the whole fleet.",
            format!(
                "sum by (stack) (rate(node_network_receive_bytes_total{{stack=~\"{list}\",device!=\"lo\"}}[5m]))"
            ),
            Unit::Bytes,
            Some("stack"),
        ),
        p(
            "Transmitted",
            "How much network traffic each stack is sending right now, side \
             by side across the whole fleet.",
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
            "How busy the hypervisor's own CPU is right now, averaged over \
             the last 5 minutes, across every guest and the host itself.",
            format!(
                "100 * (1 - avg(rate(node_cpu_seconds_total{{host=\"{h}\",mode=\"idle\"}}[5m])))"
            ),
            Unit::Percent,
            None,
        ),
        p(
            "Memory used",
            "How much of the hypervisor's own RAM is in use right now, \
             across every guest and the host itself.",
            format!(
                "node_memory_MemTotal_bytes{{host=\"{h}\"}} - node_memory_MemAvailable_bytes{{host=\"{h}\"}}"
            ),
            Unit::Bytes,
            None,
        ),
        p(
            "Disk used per filesystem",
            "How full each of the hypervisor's own filesystems is right now.",
            format!(
                "100 - (node_filesystem_avail_bytes{{host=\"{h}\",fstype=~\"zfs|ext4|xfs\",mountpoint!~\".*/subvol-.*\"}} / node_filesystem_size_bytes{{host=\"{h}\",fstype=~\"zfs|ext4|xfs\",mountpoint!~\".*/subvol-.*\"}} * 100)"
            ),
            Unit::Percent,
            Some("mountpoint"),
        ),
        p(
            "Temperature (hottest sensor per chip)",
            "The hottest reading of each physical chip (CPU, NVMe \
             controller, …) on the hypervisor's motherboard — not the \
             drives themselves, see \"Drive temperature\" below.",
            format!("max by (chip) (node_hwmon_temp_celsius{{host=\"{h}\"}})"),
            Unit::Celsius,
            Some("chip"),
        ),
        p(
            "Drive health (SMART, 1 = ok)",
            "Whether SMART reports each physical drive healthy right now; \
             shown as each drive's own current state, not a line that jumps \
             between ok and not ok, since every healthy drive reads the \
             same constant value and would otherwise overlap into one line.",
            "smart_device_health_ok".to_string(),
            Unit::Flag,
            Some("device"),
        ),
        p(
            "Memory per container (as Proxmox counts it)",
            "Each guest's own RAM use as Proxmox itself measures it, side by \
             side across the fleet.",
            "sum by (id) (pve_memory_usage_bytes{id=~\"lxc/.*\"})".to_string(),
            Unit::Bytes,
            Some("id"),
        ),
        p(
            // replace-grafana parity (2026-10-01): "Network in per host".
            "Network in",
            "How much network traffic the hypervisor's own uplink and each \
             container's virtual network device is receiving, labelled by \
             the container it belongs to.",
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
            "How many processes wanted the CPU, averaged over the last 5 \
             minutes; above the hypervisor's own core count, work is \
             waiting its turn.",
            format!("node_load5{{host=\"{h}\"}}"),
            Unit::Count,
            None,
        ),
        p(
            // Distinct from "Temperature (hottest sensor per chip)" above:
            // that's the motherboard/CPU sensors, this is the drives'.
            "Drive temperature",
            "Each physical drive's own temperature — distinct from \
             \"Temperature (hottest sensor per chip)\" above, which is the \
             motherboard and CPU sensors, not the drives.",
            "smart_device_temperature_celsius".to_string(),
            Unit::Celsius,
            Some("device"),
        ),
        p(
            "Drive pending sectors",
            "How many sectors each physical drive cannot yet confirm are \
             readable; a rising count is an early warning sign.",
            "smart_device_pending_sectors".to_string(),
            Unit::Count,
            Some("device"),
        ),
        p(
            "Drive reallocated sectors",
            "How many sectors each physical drive has already swapped out \
             for a spare because they failed; any non-zero count is worth \
             watching.",
            "smart_device_reallocated_sectors".to_string(),
            Unit::Count,
            Some("device"),
        ),
        p(
            "Drive power-on hours",
            "How many hours each physical drive has been powered on over \
             its life.",
            "smart_device_power_on_hours".to_string(),
            Unit::Count,
            Some("device"),
        ),
    ]
}

/// fix-220 (Kenny, 2026-10-02: "fwbr117iO? wtf is dat?", "wat is bv
/// 0000:00:01_0_0000:01:00_0?"): raw Prometheus label values read like
/// nobody who isn't node_exporter's own author. No app or stack name is
/// hard-coded here — every mapping either reads the pattern Proxmox itself
/// always uses for a guest's virtual network device or container cgroup id
/// (the vmid is embedded in the id by construction, not looked up), or
/// falls back to a description of the pattern (a PCI address) rather than
/// ever showing the raw id alone.
pub mod humanize {
    /// One stack's `(vmid, name)`, the pair [`crate::charts`]'s callers
    /// already have from the fleet view — passed in rather than looked up,
    /// so this module stays free of any knowledge of what a stack IS.
    pub type Stack = (u16, String);

    fn by_vmid(stacks: &[Stack], vmid: u16) -> Option<&str> {
        stacks
            .iter()
            .find(|(v, _)| *v == vmid)
            .map(|(_, name)| name.as_str())
    }

    fn ct_label(vmid: u16, stacks: &[Stack]) -> String {
        match by_vmid(stacks, vmid) {
            Some(name) => format!("CT {vmid} · {name}"),
            None => format!("CT {vmid}"),
        }
    }

    /// Proxmox names a guest's own side of a network device
    /// `<prefix><vmid>i<n>` (an LXC's veth, or its firewall bridge `fwbr`/
    /// `fwln`) or `<prefix><vmid>p<n>` (`fwpr`, the bridge's uplink pair);
    /// the vmid is always the digits right after the prefix. Returns it
    /// when `raw` matches that shape.
    fn vmid_from_guest_iface(raw: &str) -> Option<u16> {
        for prefix in ["fwbr", "fwln", "fwpr", "veth", "tap"] {
            let Some(rest) = raw.strip_prefix(prefix) else {
                continue;
            };
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                continue;
            }
            let after = &rest[digits.len()..];
            if (after.starts_with('i') || after.starts_with('p'))
                && let Ok(vmid) = digits.parse::<u16>()
            {
                return Some(vmid);
            }
        }
        None
    }

    /// A network device name (Prometheus' `device` label): a guest's own
    /// virtual device becomes "CT \<vmid\> · \<stack\>"; the hypervisor's own
    /// bridge or physical NIC becomes "host uplink \<name\>"; anything else
    /// (a drive device name on the SMART panels, which also carry a
    /// `device` label) passes through unchanged, since it is already a
    /// plain name like "sda".
    pub fn iface(raw: &str, stacks: &[Stack]) -> String {
        if raw.is_empty() {
            return raw.to_string();
        }
        if let Some(vmid) = vmid_from_guest_iface(raw) {
            return ct_label(vmid, stacks);
        }
        if raw.starts_with("vmbr")
            || raw.starts_with("eth")
            || raw.starts_with("eno")
            || raw.starts_with("enp")
            || raw.starts_with("wlp")
            || raw.starts_with("bond")
        {
            return format!("host uplink {raw}");
        }
        raw.to_string()
    }

    /// A cgroup/Proxmox resource id (`id` label), e.g. `lxc/117`, becomes
    /// "CT \<vmid\> · \<stack\>"; anything that does not parse passes
    /// through unchanged.
    pub fn cgroup_id(raw: &str, stacks: &[Stack]) -> String {
        if let Some(vmid) = raw
            .split('/')
            .nth(1)
            .and_then(|rest| rest.parse::<u16>().ok())
        {
            return ct_label(vmid, stacks);
        }
        raw.to_string()
    }

    /// node_exporter's hwmon collector names a `chip` after the sysfs path
    /// to it, with every `/` and `.` turned into `_` — e.g.
    /// `/sys/devices/pci0000:00/0000:00:01.0/0000:01:00.0/hwmon/hwmon0`
    /// becomes `0000:00:01_0_0000:01:00_0`. This finds every PCI address
    /// still readable inside that (`XXXX:XX:XX_X`, `_` standing for the
    /// function's `.`) and takes the LAST — the deepest device, closest to
    /// the sensor actually being read — never the raw chip id alone.
    fn pci_addrs(raw: &str) -> Vec<String> {
        let b: Vec<char> = raw.chars().collect();
        let n = b.len();
        let hex = |c: char| c.is_ascii_hexdigit();
        let mut out = Vec::new();
        let mut i = 0;
        while i + 11 <= n {
            if b[i..i + 4].iter().all(|&c| hex(c))
                && b[i + 4] == ':'
                && b[i + 5..i + 7].iter().all(|&c| hex(c))
                && b[i + 7] == ':'
                && b[i + 8..i + 10].iter().all(|&c| hex(c))
                && b[i + 10] == '_'
            {
                let func_start = i + 11;
                let mut j = func_start;
                while j < n && hex(b[j]) {
                    j += 1;
                }
                if j > func_start {
                    let domain: String = b[i..i + 4].iter().collect();
                    let bus: String = b[i + 5..i + 7].iter().collect();
                    let dev: String = b[i + 8..i + 10].iter().collect();
                    let func: String = b[func_start..j].iter().collect();
                    out.push(format!("{domain}:{bus}:{dev}.{func}"));
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
        out
    }

    /// A `chip` label: the deepest PCI address it encodes, domain dropped
    /// when it is the common default (`0000:`), as "PCI device 01:00.0" —
    /// node_exporter's hwmon collector carries no separate chip-name label
    /// in this fleet to read instead, so this is as specific as the data
    /// allows (never the raw chip id alone).
    pub fn pci_chip(raw: &str) -> String {
        match pci_addrs(raw).pop() {
            Some(addr) => {
                let short = addr.strip_prefix("0000:").unwrap_or(&addr);
                format!("PCI device {short}")
            }
            None => format!("PCI device {raw}"),
        }
    }

    /// Humanizes every series' `label` in place (routes.rs's `charts()`),
    /// dispatching on the panel's own `legend` key — the only three that
    /// ever carry a raw id in this fleet are `device`, `id` and `chip`;
    /// every other legend (`name`, `stack`, `mountpoint`, …) is already a
    /// name a person chose. A label two series land on after humanizing
    /// (e.g. two chips mapping to the same PCI address) gets a `#2`, `#3`,
    /// … suffix so no two series in one panel ever read identically.
    pub fn series(
        mut series: Vec<serde_json::Value>,
        legend: Option<&str>,
        stacks: &[Stack],
    ) -> Vec<serde_json::Value> {
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for s in series.iter_mut() {
            let Some(raw) = s.get("label").and_then(|v| v.as_str()).map(str::to_string) else {
                continue;
            };
            let mut h = match legend {
                Some("device") => iface(&raw, stacks),
                Some("id") => cgroup_id(&raw, stacks),
                Some("chip") => pci_chip(&raw),
                _ => raw,
            };
            let count = seen.entry(h.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                h = format!("{h} #{count}");
            }
            if let Some(obj) = s.as_object_mut() {
                obj.insert("label".into(), serde_json::Value::String(h));
            }
        }
        series
    }
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
            assert!(!panel.desc.is_empty(), "every panel has a description");
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

    #[test]
    fn every_stack_and_host_panel_has_a_one_sentence_description() {
        for panel in stack_panels("gateway")
            .into_iter()
            .chain(host_panels("pve"))
        {
            assert!(
                !panel.desc.trim().is_empty(),
                "{} has no description",
                panel.title
            );
        }
    }
}

#[cfg(test)]
mod humanize_tests {
    use super::humanize::*;

    fn stacks() -> Vec<Stack> {
        vec![(117, "registry".to_string()), (118, "gateway".to_string())]
    }

    #[test]
    fn fwbr_veth_and_tap_map_to_the_stack_that_owns_the_vmid() {
        assert_eq!(iface("fwbr117i0", &stacks()), "CT 117 · registry");
        assert_eq!(iface("veth117i0", &stacks()), "CT 117 · registry");
        assert_eq!(iface("tap118i0", &stacks()), "CT 118 · gateway");
        assert_eq!(iface("fwln117i0", &stacks()), "CT 117 · registry");
        assert_eq!(iface("fwpr118p0", &stacks()), "CT 118 · gateway");
    }

    #[test]
    fn an_unknown_vmid_still_never_reads_as_a_raw_id() {
        assert_eq!(iface("fwbr999i0", &stacks()), "CT 999");
        assert!(!iface("fwbr999i0", &stacks()).contains("fwbr"));
    }

    #[test]
    fn physical_nics_and_bridges_are_named_as_such() {
        assert_eq!(iface("vmbr0", &stacks()), "host uplink vmbr0");
        assert_eq!(iface("eno1", &stacks()), "host uplink eno1");
        assert_eq!(iface("enp3s0", &stacks()), "host uplink enp3s0");
    }

    #[test]
    fn a_plain_drive_device_name_passes_through() {
        assert_eq!(iface("sda", &stacks()), "sda");
        assert_eq!(iface("nvme0n1", &stacks()), "nvme0n1");
    }

    #[test]
    fn cgroup_id_maps_lxc_slash_vmid() {
        assert_eq!(cgroup_id("lxc/117", &stacks()), "CT 117 · registry");
        assert_eq!(cgroup_id("lxc/42", &stacks()), "CT 42");
        assert_eq!(cgroup_id("not-an-id", &stacks()), "not-an-id");
    }

    #[test]
    fn pci_chip_reads_the_deepest_address_domain_dropped() {
        assert_eq!(pci_chip("0000:00:01_0_0000:01:00_0"), "PCI device 01:00.0");
        assert_eq!(pci_chip("0000:00:1f_2"), "PCI device 00:1f.2");
    }

    #[test]
    fn pci_chip_never_shows_the_bare_raw_id_when_it_cannot_parse() {
        assert_eq!(pci_chip("acpitz-virtual-0"), "PCI device acpitz-virtual-0");
    }

    #[test]
    fn series_dedupes_two_chips_landing_on_the_same_label() {
        let raw = vec![
            serde_json::json!({"label": "fwbr117i0", "points": []}),
            serde_json::json!({"label": "veth117i0", "points": []}),
        ];
        let out = super::humanize::series(raw, Some("device"), &stacks());
        assert_eq!(out[0]["label"], "CT 117 · registry");
        assert_eq!(out[1]["label"], "CT 117 · registry #2");
    }

    #[test]
    fn series_leaves_non_id_legends_untouched() {
        let raw = vec![serde_json::json!({"label": "films", "points": []})];
        let out = super::humanize::series(raw, Some("stack"), &stacks());
        assert_eq!(out[0]["label"], "films");
    }
}
