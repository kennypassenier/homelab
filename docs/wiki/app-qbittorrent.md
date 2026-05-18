# App: qBittorrent + Gluetun

> BitTorrent client with a mandatory WireGuard VPN kill switch via Gluetun. qBittorrent cannot reach the internet unless the VPN tunnel is healthy.

## Overview

qBittorrent and Gluetun live in the **same** `docker-compose.yml` file. This is a Docker Compose requirement: `network_mode: service:<name>` only works when both services are in the same compose project. The kill switch is enforced at the network level — qBittorrent is joined to Gluetun's network namespace, so if Gluetun's process dies or the tunnel drops, all of qBittorrent's sockets close immediately.

## Service: gluetun

| Field | Value |
|---|---|
| Image | `qmcgaw/gluetun:latest` |
| VPN provider | Surfshark (WireGuard) |
| WireGuard addresses | `10.14.0.2/32` |
| Server | `ch-zur.prod.surfshark.com` |
| Ports (exposed) | `8080:8080` — qBittorrent Web UI (declared here because qBittorrent has no network stack) |

### Health Check

```yaml
test: ["CMD-SHELL", "wget -qO /dev/null http://127.0.0.1:9999"]
interval: 10s
timeout: 5s
retries: 5
start_period: 30s
```

**Critical quirk**: Gluetun's internal health server only supports **GET** requests, not HEAD. `wget --spider` sends a HEAD request and receives `HTTP 405`, causing the healthcheck to always fail even when the VPN is working. Use `wget -qO /dev/null` (GET) instead.

**Health target addresses**: Set to IP addresses (`HEALTH_TARGET_ADDRESSES=1.1.1.1:443,8.8.8.8:443`), not hostnames. Using hostnames creates a DNS race condition at startup: Gluetun's own DNS isn't ready when the first health check runs, so it fails and the container is marked unhealthy before it has had a chance to connect.

## Service: qbittorrent

| Field | Value |
|---|---|
| Image | `lscr.io/linuxserver/qbittorrent:latest` |
| Network | `network_mode: service:gluetun` — no own network stack |
| Depends on | `gluetun` with `condition: service_healthy` |
| Web UI port | `8080` (declared on gluetun) |
| PUID/PGID | 1000/1000 |

### VueTorrent UI

qBittorrent ships with a vanilla Web UI. [VueTorrent](https://github.com/VueTorrent/VueTorrent) is installed via the LSIO Docker mod:

```yaml
environment:
  - DOCKER_MODS=ghcr.io/vuetorrent/vuetorrent-lsio-mod:latest
```

The mod runs on every container start and installs VueTorrent to `/vuetorrent`. Watchtower keeps the mod image up-to-date. After first deployment, activate it once manually:
`Settings → Web UI → Use alternative Web UI → /vuetorrent`

## Environment Variables

| Variable | Source | Description |
|---|---|---|
| `VPN_SERVICE_PROVIDER` | compose | `surfshark` |
| `VPN_TYPE` | compose | `wireguard` |
| `WIREGUARD_ADDRESSES` | compose | Client address in the WireGuard tunnel |
| `SERVER_HOSTNAMES` | compose | Surfshark endpoint hostname |
| `HEALTH_TARGET_ADDRESSES` | compose | IP:port targets for the VPN health check |
| `WIREGUARD_PRIVATE_KEY` | `.env` (SOPS) | WireGuard private key — encrypted |
| `PUID`, `PGID`, `TZ` | compose | LinuxServer.io standard vars |
| `WEBUI_PORT` | compose | `8080` |
| `DOCKER_MODS` | compose | VueTorrent LSIO mod URL |

## Volumes

| Host path | Container path | Purpose |
|---|---|---|
| `/appdata/downloader/qbittorrent/config` | `/config` | qBittorrent settings, torrents |
| `/mnt/downloads` | `/downloads` | Download destination |

## Labels

| Label | Value | Purpose |
|---|---|---|
| `com.centurylinklabs.watchtower.enable` | `true` | Auto-update via Watchtower |
| `com.homelab.backup.pause` | `true` | Pause during Restic backups |

## LXC Requirements

The LXC must have `/dev/net/tun` passthrough configured. See [enable-tun.sh](script-enable-tun.md). This is handled automatically by [bootstrap-lxc.sh](script-bootstrap-lxc.md).

## See also

- [stack-downloader.md](stack-downloader.md)
- [script-enable-tun.md](script-enable-tun.md)
- [app-watchtower.md](app-watchtower.md)
