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
pub fn dashboard_json(stack: &str, apps: &[String]) -> String {
    let mut panels = String::new();
    for (i, (title, expr)) in [
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
    .into_iter()
    .enumerate()
    {
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
            "      \"id\": 5,\n",
            "      \"type\": \"stat\",\n",
            "      \"title\": \"Errors in range\",\n",
            "      \"datasource\": {{\"type\": \"loki\", \"uid\": \"loki\"}},\n",
            "      \"gridPos\": {{\"h\": 5, \"w\": 6, \"x\": 0, \"y\": 16}},\n",
            "      \"options\": {{\"reduceOptions\": {{\"calcs\": [\"lastNotNull\"]}}, \"colorMode\": \"value\", \"graphMode\": \"none\"}},\n",
            "      \"targets\": [{{\"expr\": \"sum(count_over_time({e} [$__range]))\", \"queryType\": \"instant\", \"refId\": \"A\"}}]\n",
            "    }},\n",
            "    {{\n",
            "      \"id\": 6,\n",
            "      \"type\": \"bargauge\",\n",
            "      \"title\": \"Errors by container\",\n",
            "      \"description\": \"One container producing thousands while the rest produce single digits is the normal shape here, and the useful one: it says where to look first.\",\n",
            "      \"datasource\": {{\"type\": \"loki\", \"uid\": \"loki\"}},\n",
            "      \"gridPos\": {{\"h\": 5, \"w\": 18, \"x\": 6, \"y\": 16}},\n",
            "      \"options\": {{\"displayMode\": \"gradient\", \"orientation\": \"horizontal\", \"reduceOptions\": {{\"calcs\": [\"lastNotNull\"]}}}},\n",
            "      \"targets\": [{{\"expr\": \"topk(10, sum by (container_name) (count_over_time({e} [$__range])))\", \"queryType\": \"instant\", \"refId\": \"A\"}}]\n",
            "    }},\n",
            "    {{\n",
            "      \"id\": 7,\n",
            "      \"type\": \"logs\",\n",
            "      \"title\": \"Error lines\",\n",
            "      \"datasource\": {{\"type\": \"loki\", \"uid\": \"loki\"}},\n",
            "      \"gridPos\": {{\"h\": 12, \"w\": 24, \"x\": 0, \"y\": 21}},\n",
            "      \"options\": {{\"showTime\": true, \"showLabels\": true, \"sortOrder\": \"Descending\", \"wrapLogMessage\": true, \"dedupStrategy\": \"none\"}},\n",
            "      \"targets\": [{{\"expr\": \"{e}\", \"refId\": \"A\"}}]\n",
            "    }}"
        ),
        e = err.replace('"', "\\\""),
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
