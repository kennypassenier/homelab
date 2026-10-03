//! fix-68 (see REGISTER.md): `homelab status` renders the same `FleetState`
//! the dashboard and `homelab ui` read as a short human table; `--json`
//! still gets the raw structure, for a script.

use homelab_proto::FleetState;

/// One line per stack plus a one-line host summary. Pure and sync so it is
/// cheap to test without a link.
pub fn render_table(fleet: &FleetState) -> String {
    let mut out = String::new();
    let cpu = match fleet.host.cpu_pct {
        // fix-175: no second /proc/stat sample yet (daemon just started),
        // or the pair could not be trusted — say so, not a fabricated 0%.
        Some(p) => format!("{p}%"),
        None => "unknown".to_string(),
    };
    out.push_str(&format!(
        "HOST {} :: cpu {} ram {}%/{} MiB disk {}% load {:.2}\n",
        fleet.host.name,
        cpu,
        fleet.host.ram_pct,
        fleet.host.ram_total_mb,
        fleet.host.disk_pct,
        fleet.host.load1_x100 as f64 / 100.0,
    ));
    if fleet.stacks.is_empty() {
        out.push_str("no stacks managed\n");
        return out;
    }
    let name_w = fleet
        .stacks
        .iter()
        .map(|s| s.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    out.push_str(&format!(
        "{:<name_w$}  {:>5}  {:<8}  {:<7}  {:>5}  apps\n",
        "STACK",
        "VMID",
        "ONLINE",
        "ENABLED",
        "DRIFT",
        name_w = name_w
    ));
    for s in &fleet.stacks {
        let online = if s.online { "up" } else { "down" };
        let enabled = if s.enabled { "yes" } else { "OFF" };
        let drift = if s.drift { "yes" } else { "-" };
        let running = s.apps.iter().filter(|a| a.running).count();
        let apps = if s.apps.is_empty() {
            "-".to_string()
        } else {
            format!("{}/{}", running, s.apps.len())
        };
        out.push_str(&format!(
            "{:<name_w$}  {:>5}  {:<8}  {:<7}  {:>5}  {}\n",
            s.name,
            s.vmid,
            online,
            enabled,
            drift,
            apps,
            name_w = name_w
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use homelab_proto::{AppView, HostView, StackView};

    fn fleet(stacks: Vec<StackView>) -> FleetState {
        FleetState {
            host: HostView {
                name: "pve-01".into(),
                cpu_pct: Some(3),
                ram_pct: 40,
                disk_pct: 55,
                tls_fingerprint: "abc".into(),
                ram_total_mb: 1000,
                ram_used_mb: 400,
                ram_committed_mb: 600,
                cores_total: 8,
                load1_x100: 125,
                home_address: None,
                disk_detail: None,
                uptime_s: None,
                release: None,
                guests_usage: None,
            },
            stacks,
            status_measured_at: None,
        }
    }

    fn stack(name: &str, online: bool, enabled: bool, drift: bool) -> StackView {
        StackView {
            name: name.into(),
            vmid: 110,
            hostname: format!("110-app-{name}"),
            apps: vec![AppView {
                name: "app".into(),
                // deliberately independent of `online`: online tracks
                // whether the host/vmid answers at all, not whether a
                // container the last known state remembers is running.
                running: true,
                restarts: 0,
                health: None,
            }],
            drift,
            applied_hash: "h".into(),
            env_sealed: true,
            env_sealed_read: Some(true),
            online,
            enabled,
            usage: None,
            applied_source: None,
            component_digests: Default::default(),
            native: false,
        }
    }

    // fix_68_status_table_fits_on_one_screen: the whole fleet in a handful
    // of short lines, not 1,473 of raw JSON and `pct list`.
    #[test]
    fn fix_68_status_table_fits_on_one_screen() {
        let f = fleet(vec![stack("kyu", true, true, false)]);
        let table = render_table(&f);
        assert!(table.contains("HOST pve-01"));
        assert!(table.contains("kyu"));
        assert!(table.lines().count() < 10);
    }

    #[test]
    fn fix_68_status_table_shows_down_off_and_drift() {
        let f = fleet(vec![stack("paperless", false, false, true)]);
        let table = render_table(&f);
        assert!(table.contains("down"));
        assert!(table.contains("OFF"));
        // the drift column, not the word "drift" from a comment
        assert!(
            table
                .lines()
                .any(|l| l.starts_with("paperless") && l.trim_end().ends_with("1/1"))
        );
    }

    #[test]
    fn fix_68_empty_fleet_says_so_instead_of_an_empty_table() {
        let f = fleet(vec![]);
        let table = render_table(&f);
        assert!(table.contains("no stacks managed"));
    }

    // fix_175_host_cpu_unknown_before_first_poll: no `/proc/stat` delta yet
    // renders "unknown", never a fabricated "0%".
    #[test]
    fn fix_175_host_cpu_unknown_before_first_poll() {
        let mut f = fleet(vec![]);
        f.host.cpu_pct = None;
        let table = render_table(&f);
        assert!(table.contains("cpu unknown"));
        assert!(!table.contains("cpu 0%"));
    }
}
