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

Loki listens on port 3100 on the monitoring LXC's static IP (`10.10.10.7`). All Promtail instances across all stacks write to this endpoint. Grafana auto-provisions Loki as its default datasource via a config file mounted from the Git repo.

## No `pre-sync.sh`

The monitoring stack does not need a shared Docker network — each app runs in isolation on the default bridge.

## See also

- [app-loki.md](app-loki.md)
- [app-grafana.md](app-grafana.md)
- [app-uptime-kuma.md](app-uptime-kuma.md)
- [app-promtail.md](app-promtail.md)
- [architecture-overview.md](architecture-overview.md)
