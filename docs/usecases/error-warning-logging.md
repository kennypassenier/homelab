# Use Case: Error, Warning, and Logging

**Tier:** All tiers — CLIENT, HOST, LXC — all emit; LXC forwards to Loki via Promtail  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Every component in the system emits structured logs in **logfmt** format. Logs flow from LXC containers → Promtail → Loki, and from HOST/LXC daemons → CLIENT SSE stream. Grafana dashboards consume Loki for long-term analysis and alerting.

This document defines:
1. The canonical logfmt schema.
2. Log level semantics.
3. The Promtail pipeline configuration for label extraction.
4. The Loki label taxonomy.
5. Grafana dashboard auto-discovery conventions.

---

## 2. Canonical Logfmt Schema

All log lines emitted by any component in the system must follow this schema:

```
ts=<ISO8601>  level=<level>  component=<component>  stack=<stack_name>  app=<app_name>  msg="<human-readable message>"  [optional key=value pairs]
```

### Required Fields

| Field | Type | Description |
|---|---|---|
| `ts` | ISO 8601 UTC | Timestamp in format `2006-01-02T15:04:05Z` |
| `level` | enum | `debug`, `info`, `warn`, `error` |
| `component` | enum | `client`, `host`, `lxc` |
| `msg` | string (quoted) | Human-readable description |

### Conditional Required Fields

| Field | When Required |
|---|---|
| `stack` | Any event scoped to a specific stack |
| `app` | Any event scoped to a specific Docker container/app |

### Optional Fields (Append As Relevant)

| Field | Example | Description |
|---|---|---|
| `phase` | `phase=git_push` | Current transaction phase |
| `duration_ms` | `duration_ms=4200` | Operation duration in milliseconds |
| `exit_code` | `exit_code=1` | Process exit code |
| `error` | `error="connection refused"` | Machine-readable error detail |
| `snapshot` | `snapshot=abc1234f` | Restic snapshot ID |
| `sha` | `sha=a1b2c3d` | Git commit SHA |
| `image` | `image=sha256:...` | Docker image ID or digest |
| `vmid` | `vmid=101` | Proxmox VMID |

---

## 3. Log Level Semantics

| Level | Use For |
|---|---|
| `debug` | Internal state transitions, API request/response bodies (disabled in production by default) |
| `info` | Normal operational events: phase started, phase completed, sync triggered, container healthy |
| `warn` | Non-fatal problems: `unhealthy` container (still running), legacy `pre-sync.sh` detected, optional alert send failed, upgrade-available notice |
| `error` | Fail-closed events: sync aborted, container crashed, secrets failed, rollback triggered, bootstrap failed |

**Rule:** Any `level=error` event must include an `error` field with machine-readable context. Any `level=warn` that requires user action must include a `remedy` field with a brief action description.

---

## 4. Promtail Pipeline Configuration

Each LXC has a Promtail instance (deployed as the `promtail` core app). Its `config.yml` is scaffolded by the CLIENT during `add-core-app-to-stack.md`.

### Log Source

Promtail scrapes Docker container logs via the Docker Promtail driver or by reading log files from `/var/lib/docker/containers/`:

```yaml
scrape_configs:
  - job_name: docker
    docker_sd_configs:
      - host: unix:///var/run/docker.sock
        refresh_interval: 5s
    relabel_configs:
      - source_labels: [__meta_docker_container_label_com_docker_compose_service]
        target_label: app
      - source_labels: [__meta_docker_container_label_com_docker_compose_project]
        target_label: stack
    pipeline_stages:
      - logfmt:
          mapping:
            ts: ts
            level: level
            component: component
            stack: stack
            app: app
            msg: msg
      - labels:
          level:
          component:
          stack:
          app:
      - timestamp:
          source: ts
          format: RFC3339
      - output:
          source: msg
```

### Label Extraction from Logfmt

Promtail's `logfmt` pipeline stage parses the logfmt line and promotes `level`, `component`, `stack`, and `app` to Loki labels. This enables high-cardinality filtering.

---

## 5. Loki Label Taxonomy

Labels that exist on **every** Loki log stream in the homelab:

| Label | Values | Source |
|---|---|---|
| `level` | `debug`, `info`, `warn`, `error` | Extracted from logfmt |
| `component` | `client`, `host`, `lxc` | Extracted from logfmt |
| `stack` | `cloudflared`, `media`, `paperless`, etc. | Extracted from logfmt or Docker label |
| `app` | `jellyfin`, `radarr`, `db`, `promtail`, etc. | Extracted from logfmt or Docker label |
| `job` | `docker` | Set by Promtail job name |

**High-cardinality anti-pattern to avoid:** Never add `ts`, `sha`, `image`, or `snapshot` as Loki labels. They belong in the log line body only.

---

## 6. Example Log Queries (LogQL)

```logql
# All errors across the entire homelab
{level="error"}

# All errors for a specific stack
{level="error", stack="paperless"}

# All events for a specific app
{stack="media", app="jellyfin"}

# Sync events across all LXC daemons
{component="lxc"} |= "sync"

# Rollback events (fail-closed)
{component="lxc", level="warn"} |= "rollback"

# Duration query: how long do syncs take?
{component="lxc"} | logfmt | duration_ms > 10000
```

---

## 7. Grafana Dashboard Auto-Discovery

Grafana provisioning uses the Loki data source with label-based queries. Each stack's Grafana dashboard is defined as a ConfigMap-equivalent JSON provisioned to `/appdata/grafana/dashboards/`:

```json
{
  "title": "${stack} Overview",
  "uid": "homelab-${stack}",
  "panels": [
    {
      "type": "logs",
      "title": "Recent Events",
      "targets": [{ "expr": "{stack=\"${stack}\"}" }]
    },
    {
      "type": "stat",
      "title": "Error Rate",
      "targets": [{ "expr": "count_over_time({stack=\"${stack}\", level=\"error\"}[5m])" }]
    }
  ]
}
```

New stacks are auto-discovered when their Promtail instance starts emitting logs with `stack=<name>` label — no manual Grafana configuration needed.

---

## 8. CLIENT Log Buffer and SSE Stream

The CLIENT subscribes to SSE streams from HOST (`GET /api/events/stream`) and from each active LXC (`GET /api/logs/stream`). Log events received over SSE are:

1. Appended to the in-memory ring buffer for the matching stack (max 500 lines, oldest dropped).
2. Written to `~/.local/share/homelab/logs/<stack>/<date>.log` for offline review.
3. Rendered in the deployment modal log pane (see `tui-deployment-modal-progress.md`).
4. Parsed for error events → if `level=error`, the stack's badge in the Stacks tab turns red.

### Log File Retention

Client-side log files are rotated daily and retained for 30 days. File format: plain logfmt, one line per event.

---

## 9. Error Badge in Stacks Tab

The CLIENT Stacks tab main list shows a compact health badge per stack:

| Badge | Condition |
|---|---|
| `✓` | No errors in last 24h |
| `⚠ N warn` | N warnings in last 24h, no errors |
| `✗ N err` | N errors in last 24h |

Selecting a stack with errors navigates to a "Recent Errors" detail view showing the last 20 error-level log lines with full logfmt context.

---

## 10. Logfmt Rust Crate

All Rust daemons (CLIENT, HOST, LXC) use a common logging setup:

```rust
// Cargo.toml
[dependencies]
tracing = "0.1"
tracing-logfmt = "0.3"        // formats tracing spans as logfmt
tracing-subscriber = "0.3"

// main.rs
tracing_subscriber::fmt()
    .with_writer(std::io::stderr)
    .event_format(tracing_logfmt::EventsFormatter::default())
    .init();
```

Structured fields are added via tracing's `instrument` macro and `info!` span fields:

```rust
#[tracing::instrument(fields(stack = %stack_name, phase = "git_push"))]
async fn push_to_git(stack_name: &str) -> Result<()> {
    tracing::info!("git push started");
    // ...
    tracing::info!(sha = %new_sha, "git push complete");
    Ok(())
}
```

This automatically produces logfmt output with `stack=`, `phase=`, `sha=` fields.

---

## 11. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-core-app-to-stack.md` | Promtail docker-compose.yml + config.yml scaffold |
| `tui-deployment-modal-progress.md` | Renders log lines from SSE stream; color-codes by level |
| `error-handling-fail-closed.md` | All fail-closed events emit `level=error` per this schema |
| `pre-sync-hooks.md` | `setup.sh` logfmt events defined here |
| `post-deploy-actions.md` | Post-deploy logfmt events defined here |
| `manual-backup-all.md` | Backup logfmt events defined here |
