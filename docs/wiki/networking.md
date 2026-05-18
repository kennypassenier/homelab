# Networking

> Static IPs via DHCP reservations in OPNsense, SSH aliases in `~/.ssh/config`, and per-stack Docker bridge networks created by `pre-sync.sh`.

## Overview

There are three layers of networking in this homelab: the LAN (physical/VLAN), SSH access from the client, and Docker networking inside each LXC.

## LAN — Static IPs via DHCP

Each LXC gets a static IP through a DHCP reservation in OPNsense, keyed to the LXC's MAC address. This means:
- IPs are stable and predictable without configuring static IPs inside the LXC
- Changing an IP is done in OPNsense, not in the container
- The LXC itself uses DHCP — no manual `/etc/network/interfaces` editing

## SSH Access — Aliases via `~/.ssh/config`

The [add-ssh.sh](script-add-ssh.md) script manages SSH aliases on the developer's Linux desktop. Each LXC gets an entry in `~/.ssh/config`:

```
Host media
    HostName 10.10.10.x
    User root

Host gateway
    HostName 10.10.10.x
    User root
```

This allows `ssh media`, `ssh gateway`, etc. instead of remembering IPs. The script is idempotent — running it again for an existing alias updates the IP instead of creating a duplicate entry.

Run via `./client.sh → Register SSH alias for a new LXC`.

## Docker Networking

### Per-stack Bridge Networks

Some stacks have multiple apps that need to communicate with each other (e.g. Paperless-NGX talking to its PostgreSQL database). These are connected via a shared Docker bridge network named after the stack.

Each such stack has a `pre-sync.sh` that creates the network before compose runs:

| Stack | Network name | Created by |
|---|---|---|
| [gateway](stack-gateway.md) | `gateway_network` | `stacks/gateway/pre-sync.sh` |
| [media](stack-media.md) | `media_network` | `stacks/media/pre-sync.sh` |
| [paperless](stack-paperless.md) | `paperless_network` | `stacks/paperless/pre-sync.sh` |

The network must exist before any `docker compose up` runs. Since the apps in a stack are in separate `docker-compose.yml` files (separate compose projects), Docker Compose cannot create the network automatically — each project would try to create it and fail with a conflict. `pre-sync.sh` solves this by creating it once, idempotently.

Each compose file that uses a shared network declares it as external:

```yaml
networks:
  media_network:
    name: media_network
    external: true  # Do not create — it already exists from pre-sync.sh

services:
  jellyfin:
    networks:
      - media_network
```

### Apps Without a Shared Network

Apps that don't need to talk to siblings use Docker's default bridge. The [downloader stack](stack-downloader.md) is a special case: [qBittorrent](app-qbittorrent.md) uses `network_mode: service:gluetun`, meaning it has no network stack of its own — all traffic is routed through the [Gluetun](app-qbittorrent.md) VPN container.

### Monitoring Network Access

[Promtail](app-promtail.md) reaches [Loki](app-loki.md) via IP (`LOKI_IP` env variable in `.env`), not via a Docker network — Loki runs in a different LXC. The IP is injected at runtime using `-config.expand-env=true` in Promtail's command, avoiding hardcoded IPs in `config.yml`.

Similarly, [Grafana's Loki datasource](stack-monitoring.md) provisioning points directly to `http://10.10.10.7:3100` — the static IP of the monitoring LXC.

## Port Allocation Summary

| Service | Port | Stack |
|---|---|---|
| Nginx Proxy Manager (HTTP) | 80 | gateway |
| Nginx Proxy Manager (HTTPS) | 443 | gateway |
| Nginx Proxy Manager (Admin UI) | 81 | gateway |
| GoAccess | 7880 | gateway |
| Jellyfin | 8096 | media |
| Sonarr | 8989 | media |
| Radarr | 7878 | media |
| Prowlarr | 9696 | media |
| Bazarr | 6767 | media |
| Seerr | 5055 | media |
| qBittorrent Web UI | 8080 | downloader |
| Grafana | 3000 | monitoring |
| Loki | 3100 | monitoring |
| Uptime Kuma | 3001 | monitoring |
| Paperless-NGX | 8000 | paperless |
| Paperless AI Tagger | 3002 | paperless |
| Paperless AI RAG | 8001 | paperless |

## See also

- [script-add-ssh.md](script-add-ssh.md)
- [Architecture Overview](architecture-overview.md)
- [stack-gateway.md](stack-gateway.md)
- [app-qbittorrent.md](app-qbittorrent.md) — VPN kill switch networking
- [GitOps Flow](gitops-flow.md) — pre-sync.sh network creation
