//! T2: a stack brings its own Grafana dashboard.
//!
//! The dashboards that exist today were built by hand and lived in no
//! repository until 2026-08-30, which is how a Grafana rebuild would have
//! taken them. Worse, adding a stack meant remembering to open Grafana — and
//! the thing nobody remembers is the thing that is not there when it matters.
//!
//! So a dashboard is rendered from the manifest at deploy time and written
//! where Grafana's provisioning watcher picks it up. Provisioned dashboards
//! are files, not database rows: they survive a rebuild of the container and
//! they diff in review.
//!
//! The generated panels are deliberately the ones that mean the same thing for
//! every stack — CPU, memory, disk and restarts per container, from cadvisor,
//! plus the host-level view from node_exporter. Anything specific to one app
//! (Jellyfin's transcodes, qBittorrent's queue) belongs in a hand-written
//! dashboard beside it, because a generator that tries to know every app ends
//! up knowing none of them well.

/// One dashboard per stack, rendered from what the manifest already knows.
/// Byte-stable for the same input: a deploy that changes nothing must write
/// nothing, or the fleet check reports drift after every deploy.
/// The four metric panels, chosen for what the stack actually runs.
///
/// B3 (Kenny, 2026-09-02). The docker set asks cadvisor for per-container CPU,
/// memory and restarts. cadvisor measures docker containers, so on a stack
/// that runs none it can only ever answer nothing — which is what Kenny found
/// on the kyu and almanac dashboards: three empty graphs teaching their reader
/// that the dashboard has nothing to say.
///
/// The native set asks node_exporter about the container as a whole, and adds
/// the service's own counters. That last part came from the almanac session
/// (F248) and corrected this design before it was built: for a service that
/// logs only when something happens, a counter answers WHETHER it is working
/// and a log line answers WHY it is not. Their exact case — "did almanac
/// process anything today?" — was already answerable from Prometheus while
/// the panel beside it sat empty.
///
/// fix-70 (native-uptime-panel-wrong, 2026-09-27): that first version drew
/// "Service uptime" as `time() - node_boot_time_seconds`, which inside an LXC
/// is the hypervisor kernel's boot time: 58.5 days on kyu and on almanac
/// alike, measured the day both were redeployed, and a unit that kept dying
/// would have shown the same number. Nor did it add the counters it promised.
/// A unit's state and the uptime the service reports itself are what answer
/// "is it alive"; `units` are the stack's systemd units.
fn metric_panels(stack: &str, units: &[String], native: bool) -> Vec<(&'static str, String)> {
    if native {
        return vec![
            (
                "CPU (whole container)",
                format!(
                    "sum(rate(node_cpu_seconds_total{{stack=\"{s}\",mode!=\"idle\"}}[5m]))",
                    s = stack
                ),
            ),
            (
                "Memory used",
                format!(
                    "node_memory_MemTotal_bytes{{stack=\"{s}\"}} - node_memory_MemAvailable_bytes{{stack=\"{s}\"}}",
                    s = stack
                ),
            ),
            (
                // Not "restarts per container": there are none. One line per
                // unit from node_exporter's systemd collector, 1 while the
                // unit is active. `[.]` rather than an escaped dot: the
                // expression is embedded in JSON, where a backslash would
                // need escaping of its own.
                "Units active (1 = running)",
                format!(
                    "node_systemd_unit_state{{stack=\"{s}\",state=\"active\",name=~\"({u})[.]service\"}}",
                    s = stack,
                    u = units.join("|")
                ),
            ),
            (
                "Filesystem used",
                format!(
                    "100 - (node_filesystem_avail_bytes{{stack=\"{s}\",mountpoint=\"/\"}} / node_filesystem_size_bytes{{stack=\"{s}\",mountpoint=\"/\"}} * 100)",
                    s = stack
                ),
            ),
            (
                // The uptime the service itself exports, under its own
                // scrape job (named after the stack, as kyu's and almanac's
                // are). It resets on every restart, so a unit that keeps
                // dying shows as a saw-tooth. Empty for a service that
                // exports none.
                "Service uptime (as the service reports it)",
                format!(
                    "{{job=\"{s}\",__name__=~\".+_uptime_seconds\"}}",
                    s = stack
                ),
            ),
        ];
    }
    vec![
        (
            "CPU per container",
            format!(
                "rate(container_cpu_usage_seconds_total{{stack=\"{s}\"}}[5m])",
                s = stack
            ),
        ),
        (
            "Memory per container",
            format!("container_memory_usage_bytes{{stack=\"{s}\"}}", s = stack),
        ),
        (
            "Restarts per container",
            format!(
                "changes(container_start_time_seconds{{stack=\"{s}\"}}[1h])",
                s = stack
            ),
        ),
        (
            "Filesystem used",
            format!(
                "100 - (node_filesystem_avail_bytes{{stack=\"{s}\",mountpoint=\"/\"}} / node_filesystem_size_bytes{{stack=\"{s}\",mountpoint=\"/\"}} * 100)",
                s = stack
            ),
        ),
    ]
}

pub fn dashboard_json(stack: &str, apps: &[String]) -> String {
    dashboard_json_for(stack, apps, false)
}

/// `native` = the stack runs systemd units rather than docker containers.
pub fn dashboard_json_for(stack: &str, apps: &[String], native: bool) -> String {
    let mut panels = String::new();
    let metrics = metric_panels(stack, apps, native);
    // fix-70: the native set has five panels, the docker set four. The error
    // panels below number and place themselves after however many there are;
    // with four that is ids 5-7 at y 16, byte for byte what it always was.
    let (log_id, log_y) = (metrics.len() + 1, metrics.len().div_ceil(2) * 8);
    for (i, (title, expr)) in metrics.into_iter().enumerate() {
        let id = i + 1;
        if id > 1 {
            panels.push_str(",\n");
        }
        panels.push_str(&format!(
            concat!(
                "    {{\n",
                "      \"id\": {id},\n",
                "      \"type\": \"timeseries\",\n",
                "      \"title\": \"{title}\",\n",
                "      \"datasource\": {{\"type\": \"prometheus\", \"uid\": \"prometheus\"}},\n",
                "      \"gridPos\": {{\"h\": 8, \"w\": 12, \"x\": {x}, \"y\": {y}}},\n",
                "      \"targets\": [{{\"expr\": \"{expr}\", \"refId\": \"A\"}}]\n",
                "    }}"
            ),
            id = id,
            title = title,
            expr = expr.replace('"', "\\\""),
            x = i % 2 * 12,
            y = i / 2 * 8,
        ));
    }
    // ── Errors only, per stack ───────────────────────────────────────────
    //
    // Kenny asked for this on every stack dashboard, not just the fleet-wide
    // one, and it belongs in the generator rather than in each file: a stack
    // deployed next month gets it without anyone remembering.
    //
    // The `!= "level=info"` is not tidiness. Loki logs every query it runs,
    // those queries contain the word "error", so without it Loki finds its own
    // search for errors and counts it as one — that inflated the gateway from
    // 29 to 314 in a single hour on 2026-08-31.
    //
    // These read a different datasource than the four panels above, which is
    // why they are built here instead of in that loop.
    let err = format!(
        "{{stack=\"{s}\"}} |~ \"(?i)(error|exception|fatal|panic)\" != \"level=info\"",
        s = stack
    );
    panels.push_str(&format!(
        concat!(
            ",\n    {{\n",
            "      \"id\": {id1},\n",
            "      \"type\": \"stat\",\n",
            "      \"title\": \"Errors in range\",\n",
            "      \"datasource\": {{\"type\": \"loki\", \"uid\": \"loki\"}},\n",
            "      \"gridPos\": {{\"h\": 5, \"w\": 6, \"x\": 0, \"y\": {y1}}},\n",
            "      \"options\": {{\"reduceOptions\": {{\"calcs\": [\"lastNotNull\"]}}, \"colorMode\": \"value\", \"graphMode\": \"none\"}},\n",
            "      \"targets\": [{{\"expr\": \"sum(count_over_time({e} [$__range]))\", \"queryType\": \"instant\", \"refId\": \"A\"}}]\n",
            "    }},\n",
            "    {{\n",
            "      \"id\": {id2},\n",
            "      \"type\": \"bargauge\",\n",
            "      \"title\": \"Errors by container\",\n",
            "      \"description\": \"One container producing thousands while the rest produce single digits is the normal shape here, and the useful one: it says where to look first.\",\n",
            "      \"datasource\": {{\"type\": \"loki\", \"uid\": \"loki\"}},\n",
            "      \"gridPos\": {{\"h\": 5, \"w\": 18, \"x\": 6, \"y\": {y1}}},\n",
            "      \"options\": {{\"displayMode\": \"gradient\", \"orientation\": \"horizontal\", \"reduceOptions\": {{\"calcs\": [\"lastNotNull\"]}}}},\n",
            "      \"targets\": [{{\"expr\": \"topk(10, sum by (container_name) (count_over_time({e} [$__range])))\", \"queryType\": \"instant\", \"refId\": \"A\"}}]\n",
            "    }},\n",
            "    {{\n",
            "      \"id\": {id3},\n",
            "      \"type\": \"logs\",\n",
            "      \"title\": \"Error lines\",\n",
            "      \"datasource\": {{\"type\": \"loki\", \"uid\": \"loki\"}},\n",
            "      \"gridPos\": {{\"h\": 12, \"w\": 24, \"x\": 0, \"y\": {y2}}},\n",
            "      \"options\": {{\"showTime\": true, \"showLabels\": true, \"sortOrder\": \"Descending\", \"wrapLogMessage\": true, \"dedupStrategy\": \"none\"}},\n",
            "      \"targets\": [{{\"expr\": \"{e}\", \"refId\": \"A\"}}]\n",
            "    }}"
        ),
        e = err.replace('"', "\\\""),
        id1 = log_id,
        id2 = log_id + 1,
        id3 = log_id + 2,
        y1 = log_y,
        y2 = log_y + 5,
    ));

    format!(
        concat!(
            "{{\n",
            "  \"uid\": \"homelab-{stack}\",\n",
            "  \"title\": \"{stack}\",\n",
            "  \"tags\": [\"homelab\", \"generated\"],\n",
            "  \"timezone\": \"browser\",\n",
            "  \"schemaVersion\": 39,\n",
            "  \"refresh\": \"1m\",\n",
            "  \"time\": {{\"from\": \"now-6h\", \"to\": \"now\"}},\n",
            "  \"description\": \"Generated by the homelab orchestrator for stack '{stack}' ({apps}). Edits here are overwritten on the next deploy — change the generator, not the dashboard.\",\n",
            "  \"panels\": [\n{panels}\n  ]\n",
            "}}\n"
        ),
        stack = stack,
        apps = apps.join(", "),
        panels = panels,
    )
}

/// Where the dashboard lands in Grafana's provisioning directory.
pub fn dashboard_file(dir: &str, stack: &str) -> String {
    format!("{}/homelab-{}.json", dir.trim_end_matches('/'), stack)
}
