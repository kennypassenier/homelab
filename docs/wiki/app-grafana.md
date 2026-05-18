# App: Grafana

> Dashboard platform. Visualises logs from Loki and any other data sources. Loki is auto-provisioned as the default datasource from a config file in the Git repository.

## Container

| Field | Value |
|---|---|
| Image | `grafana/grafana:latest` |
| Port | `3000` |
| User | `0:0` (root) — required to read mounted provisioning files |

## Auto-provisioning

Grafana's provisioning directory is mounted directly from the Git repository inside the LXC:

```
/opt/gitops/stacks/monitoring/grafana/provisioning → /etc/grafana/provisioning
```

This means changes to datasource or dashboard provisioning configs in Git are applied on the next `docker compose up` restart without manual intervention.

### Loki Datasource

`stacks/monitoring/grafana/provisioning/datasources/loki.yaml` provisions Loki automatically:
- URL: `http://10.10.10.7:3100` (monitoring LXC static IP)
- Type: `loki`
- Set as default datasource

## Volumes

| Host path | Container path | Purpose |
|---|---|---|
| `/appdata/monitoring/grafana/data` | `/var/lib/grafana` | Grafana database + state |
| `/opt/gitops/stacks/monitoring/grafana/provisioning` | `/etc/grafana/provisioning` (ro) | Auto-provisioned datasources |

## Environment Variables

| Variable | Source | Description |
|---|---|---|
| `GF_SECURITY_ADMIN_PASSWORD` | `.env` (SOPS) | Admin password |
| `GF_PATHS_PROVISIONING` | compose | `/etc/grafana/provisioning` |

## See also

- [stack-monitoring.md](stack-monitoring.md)
- [app-loki.md](app-loki.md)
