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
pub fn targets_json(stack: &str, ip: &str) -> String {
    let host = ip_only(ip);
    format!(
        concat!(
            "[\n",
            "  {{\n",
            "    \"targets\": [\"{host}:{node}\"],\n",
            "    \"labels\": {{\"job\": \"node\", \"stack\": \"{stack}\", \"host\": \"{stack}\", \"role\": \"lxc\"}}\n",
            "  }},\n",
            "  {{\n",
            "    \"targets\": [\"{host}:{cadvisor}\"],\n",
            "    \"labels\": {{\"job\": \"cadvisor\", \"stack\": \"{stack}\", \"host\": \"{stack}\", \"role\": \"lxc\"}}\n",
            "  }}\n",
            "]\n"
        ),
        host = host,
        node = NODE_EXPORTER_PORT,
        cadvisor = CADVISOR_PORT,
        stack = stack,
    )
}

/// Manifests carry CIDR (`10.10.10.13/24`); a scrape target must not.
fn ip_only(ip: &str) -> &str {
    ip.split('/').next().unwrap_or(ip)
}

/// Where this stack's discovery file lives under the configured directory.
pub fn target_file(dir: &str, stack: &str) -> String {
    format!("{}/{}.json", dir.trim_end_matches('/'), stack)
}
