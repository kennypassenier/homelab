# App: Promtail

> Log shipper. Tails container logs and the `node-sync.sh` log file and forwards them to Loki with structured labels.

## Overview

Each stack runs a dedicated Promtail instance. All instances ship to the same Loki endpoint (`${LOKI_IP}:3100`, resolved from the SOPS-encrypted `.env`). The stack and app names are extracted as Loki labels for efficient log querying.

## Container

| Field | Value |
|---|---|
| Image | `grafana/promtail:latest` |
| Command | `-config.file=/etc/promtail/config.yml -config.expand-env=true` |

`-config.expand-env=true` enables `${VARIABLE}` substitution in the config file, used for the `LOKI_IP` variable.

## Configuration

`stacks/<stack>/promtail/config.yml` is mounted from the Git repo. The same file structure is reused across all stacks with the stack name substituted.

### Scrape Jobs

**1. varlogs** — system journal logs from `/var/log`:
```yaml
- targets: [localhost]
  labels:
    job: varlogs
    __path__: /var/log/*log
```

**2. docker** — all Docker container logs via the Docker socket:
```yaml
- targets: [localhost]
  labels:
    job: docker
  pipeline_stages:
    - docker: {}  # parses Docker log JSON, promotes stream/attrs
```
Labels `container_name` and `compose_service` are automatically extracted.

**3. node_sync** — structured logfmt output from `node-sync.sh`:
```yaml
- targets: [localhost]
  labels:
    job: node_sync
    __path__: /var/log/node-sync.log
  pipeline_stages:
    - logfmt: {}           # parses key=value pairs
    - labels:
        level:             # promotes 'level' field as Loki label
        stack:             # promotes 'stack' field as Loki label
        app:               # promotes 'app' field as Loki label
    - timestamp:
        source: ts
        format: RFC3339    # promotes 'ts' field as the log timestamp
```

## Volumes

| Host path | Container path | Purpose |
|---|---|---|
| `/var/log` | `/var/log` (ro) | System logs |
| `/var/lib/docker/containers` | `/var/lib/docker/containers` (ro) | Docker container log files |
| `/var/run/docker.sock` | `/var/run/docker.sock` | Docker API for metadata |
| `/var/log/node-sync.log` | `/var/log/node-sync.log` (ro) | node-sync.sh log file |
| (git repo path) | `/etc/promtail/config.yml` (ro) | Promtail configuration |

## Environment Variables

| Variable | Source | Description |
|---|---|---|
| `LOKI_IP` | `.env` (SOPS) | IP address of the Loki instance |

## See also

- [app-loki.md](app-loki.md)
- [script-node-sync.md](script-node-sync.md)
- [stack-monitoring.md](stack-monitoring.md)
