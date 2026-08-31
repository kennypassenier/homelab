//! T1: Prometheus learns about a stack from a file the orchestrator writes.
//!
//! Today eleven node addresses and six cadvisor addresses sit hardcoded in
//! `prometheus.yml`. Nothing keeps that list honest: a container added to the
//! fleet is simply not measured until somebody remembers, and one removed
//! keeps alerting as `HostDown` on its way out. Both happened here — the
//! scratch container at 10.10.10.14 was still a target this morning.
//!
//! So the list stops being an administration and becomes a consequence: on
//! deploy the orchestrator writes one small file per stack, on destroy it
//! removes it, and Prometheus picks the change up on its own through
//! `file_sd_configs` — no reload, no restart.
//!
//! What is written is only what the golden template guarantees on every
//! container: node_exporter and cadvisor. Anything an app exposes beyond that
//! is the app's own scrape job, because only the stack file knows its port.

/// Ports the golden template puts on every managed container (O2).
const NODE_EXPORTER_PORT: u16 = 9100;
/// 8081, not cadvisor's own 8080: gluetun already publishes 8080 on the
/// downloader stack, and one uniform port keeps the scrape config to a single
/// pattern.
const CADVISOR_PORT: u16 = 8081;

/// The file-based discovery document for one stack. Stable field order, so a
/// rewrite that changes nothing produces a byte-identical file and the drift
/// check has nothing to report.
pub fn targets_json(stack: &str, ip: &str, runs_docker: bool) -> String {
    let host = ip_only(ip);
    let node = format!(
        concat!(
            "  {{\n",
            "    \"targets\": [\"{host}:{port}\"],\n",
            "    \"labels\": {{\"job\": \"node\", \"stack\": \"{stack}\", \"host\": \"{stack}\", \"role\": \"lxc\"}}\n",
            "  }}"
        ),
        host = host,
        port = NODE_EXPORTER_PORT,
        stack = stack,
    );
    // A native-service stack has no docker and therefore no cadvisor.
    // Measured on this fleet 2026-08-31: kyu (CT 109) and almanac (CT 112)
    // answer on 9100 and refuse 8081. Writing the cadvisor target anyway
    // would give Prometheus a permanently unreachable endpoint and
    // Alertmanager a permanently firing rule — an alert that is always on is
    // an alert nobody reads, which costs more than the missing panel.
    if !runs_docker {
        return format!("[\n{}\n]\n", node);
    }
    let cadvisor = format!(
        concat!(
            "  {{\n",
            "    \"targets\": [\"{host}:{port}\"],\n",
            "    \"labels\": {{\"job\": \"cadvisor\", \"stack\": \"{stack}\", \"host\": \"{stack}\", \"role\": \"lxc\"}}\n",
            "  }}"
        ),
        host = host,
        port = CADVISOR_PORT,
        stack = stack,
    );
    format!("[\n{},\n{}\n]\n", node, cadvisor)
}

/// Manifests carry CIDR (`10.10.10.13/24`); a scrape target must not.
fn ip_only(ip: &str) -> &str {
    ip.split('/').next().unwrap_or(ip)
}

/// Where this stack's discovery file lives under the configured directory.
pub fn target_file(dir: &str, stack: &str) -> String {
    format!("{}/{}.json", dir.trim_end_matches('/'), stack)
}
