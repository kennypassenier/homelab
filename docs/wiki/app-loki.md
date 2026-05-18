# App: Loki

> Log aggregation backend. Receives structured logs from all Promtail instances and stores them for querying in Grafana.

## Container

| Field | Value |
|---|---|
| Image | `grafana/loki:latest` |
| Port | `3100` |
| Config file | `loki-config.yaml` (mounted from Git repo) |

## Configuration

`stacks/monitoring/loki/loki-config.yaml` is mounted directly from the Git repo path inside the LXC (`/opt/gitops/stacks/monitoring/loki/loki-config.yaml`). Changes to the file are picked up on the next `docker compose up` (triggered by [node-sync.sh](script-node-sync.md)).

Key configuration choices:
- **Schema**: v13
- **Storage**: filesystem (`/loki/data`) — no S3 or object storage
- **Index store**: tsdb
- **Auth**: disabled — no multi-tenancy; all Promtail instances write to the single tenant

## Volumes

| Host path | Container path | Purpose |
|---|---|---|
| `/appdata/monitoring/loki/data` | `/loki/data` | Log chunk storage |
| (git repo path) | `/etc/loki/loki-config.yaml` (ro) | Loki configuration |

## See also

- [stack-monitoring.md](stack-monitoring.md)
- [app-grafana.md](app-grafana.md)
- [app-promtail.md](app-promtail.md)
