# Stack: Monitoring

> Centralised observability stack. Loki aggregates logs from all stacks via Promtail; Grafana visualises them; Uptime Kuma tracks service uptime.

## Overview

The monitoring stack is the observability hub for the entire homelab. Every other stack ships its container logs and `node-sync.sh` structured logs to Loki via per-stack Promtail instances.

## Apps

| App | Purpose | Port |
|---|---|---|
| [Loki](app-loki.md) | Log aggregation backend | 3100 |
| [Grafana](app-grafana.md) | Metrics + log dashboards | 3000 |
| [Uptime Kuma](app-uptime-kuma.md) | Uptime + ping monitoring | 3001 |
| [Watchtower](app-watchtower.md) | Automatic image updates | — |

## Architecture


```
All stacks → Promtail (per LXC) → Loki (10.10.10.7:3100)
                                        │
                                    Grafana (10.10.10.7:3000)
```

Loki listens on port 3100 on the monitoring LXC's static IP (`10.10.10.7`). All Promtail instances across all stacks write to this endpoint.

## Universal Log Dashboard (Zero Manual Edits)

Grafana is auto-provisioned with a universal log dashboard (`stacks/monitoring/grafana/provisioning/dashboards/homelab-logs.json`). This dashboard automatically discovers all stacks and apps by reading log labels from Loki. As soon as a new stack or app is added (with Promtail configured), it appears in the dashboard dropdowns—**no manual dashboard edits required**.

- **Location:** [stacks/monitoring/grafana/provisioning/dashboards/homelab-logs.json](../../stacks/monitoring/grafana/provisioning/dashboards/homelab-logs.json)
- **How:** The dashboard uses template variables for `stack` and `app`, populated from Loki labels. All logs are instantly searchable and filterable by stack/app, with full-text search and time range selection.
- **Automation:** The dashboard is provisioned from Git. Any changes to the dashboard JSON are picked up on the next `docker compose up` in the monitoring stack. No manual steps are needed to add/remove stacks or apps.

**Result:**
- All logs from all stacks/apps are visible in Grafana out-of-the-box.
- Adding a new stack/app (with Promtail) makes its logs appear in the dashboard automatically.
- No manual dashboard edits or provisioning steps are ever needed.

## No `pre-sync.sh`

The monitoring stack does not need a shared Docker network — each app runs in isolation on the default bridge.

## See also

- [app-loki.md](app-loki.md)
- [app-grafana.md](app-grafana.md)
- [app-uptime-kuma.md](app-uptime-kuma.md)
- [app-promtail.md](app-promtail.md)
- [architecture-overview.md](architecture-overview.md)
