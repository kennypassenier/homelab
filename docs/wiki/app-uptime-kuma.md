# App: Uptime Kuma

> Self-hosted uptime monitoring. Pings services and sends alerts when they go down.

## Container

| Field | Value |
|---|---|
| Image | `louislam/uptime-kuma:latest` |
| Port | `3001` |

## Volumes

| Host path | Container path |
|---|---|
| `/appdata/monitoring/uptime-kuma/config` | `/app/data` |

## See also

- [stack-monitoring.md](stack-monitoring.md)
