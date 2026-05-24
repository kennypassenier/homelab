# Homelab Wiki

> Self-hosted GitOps homelab running on Proxmox VE. Unprivileged LXC containers with Docker Compose, managed via Git. Changes pushed to the repo are automatically applied within 5 minutes by `node-sync.sh`.

---

## Architecture Concepts

| Page | Description |
|---|---|
| [Architecture Overview](architecture-overview.md) | Three-tier diagram: client → host → containers |
| [GitOps Flow](gitops-flow.md) | How `node-sync.sh` syncs the repo to running containers |
| [Secret Management](secret-management.md) | SOPS + Age encryption & dynamic secrets provisioning (Infisical) |
| [Storage Layout](storage-layout.md) | Proxmox host paths, bind mounts & automatische directory-creatie |
| [Networking](networking.md) | Static IPs, SSH aliases, per-stack Docker networks |
| [Backups](backups.md) | Restic backup, automatische OS security updates |

---

## Shared Libraries

| Page | Description |
|---|---|
| [lib-ui.sh](lib-ui.md) | TUI output, prompts, spinners — used by all scripts |
| [lib-stack.sh](lib-stack.md) | Stack/app selection and file generation helpers |

---

## Scripts — Client

Run on the Linux desktop from the repo root.

| Page | Description |
|---|---|
| [client.sh](script-client-sh.md) | Main TUI menu entry point |
| [create-new-stack.sh](script-create-new-stack.md) | Scaffold a new stack directory |
| [create-new-app.sh](script-create-new-app.md) | Add an app to an existing stack |
| [remove-app.sh](script-remove-app.md) | Remove an app (triggers GitOps GC) |
| [remove-stack.sh](script-remove-stack.md) | Remove an entire stack |
| [add-ssh.sh](script-add-ssh.md) | Add an SSH alias to `~/.ssh/config` |


---

## Scripts — Host

Run on the Proxmox VE host.

| Page | Description |
|---|---|
| [host.sh](script-host-sh.md) | Main TUI menu entry point |
| [bootstrap-lxc.sh](script-bootstrap-lxc.md) | Bootstrap a new LXC container |
| [sync-host.sh](script-sync-host.md) | Sync host helper scripts from the repo |
| [setup-cron.sh](script-setup-cron.md) | Install hourly cron + logrotate |
| [backup-stacks.sh](script-backup-stacks.md) | Run Restic backup with container pause/resume |
| [enable-gpu.sh](script-enable-gpu.md) | Add GPU passthrough to an LXC |
| [enable-tun.sh](script-enable-tun.md) | Add TUN device passthrough to an LXC |
| [reset-stack.sh](script-reset-stack.md) | Wipe Docker state + app data for a corrupted stack |

---

## Scripts — Container

Run inside an LXC.

| Page | Description |
|---|---|
| [container.sh](script-container-sh.md) | Trigger node-sync or other container operations |
| [node-sync.sh](script-node-sync.md) | Core GitOps sync loop (runs every 5 min via cron) |

---

## Stacks

| Page | LXC Purpose | Apps |
|---|---|---|
| [downloader](stack-downloader.md) | VPN-protected BitTorrent | qBittorrent + Gluetun, Watchtower, Promtail |
| [media](stack-media.md) | Media server + Arr stack | Jellyfin, Sonarr, Radarr, Prowlarr, Bazarr, Seerr, Watchtower, Promtail |
| [gateway](stack-gateway.md) | Reverse proxy + security | Nginx Proxy Manager, CrowdSec, GoAccess, Watchtower, Promtail |
| [monitoring](stack-monitoring.md) | Observability | Loki, Grafana, Uptime Kuma, Watchtower |
| [paperless](stack-paperless.md) | Document management | Paperless-NGX, PostgreSQL, Redis, paperless-ai, Watchtower, Promtail |
| [cloudflared](stack-cloudflared.md) | Cloudflare Tunnel | Cloudflared, Watchtower, Promtail |

---

## Apps

### Downloader stack
- [qBittorrent + Gluetun](app-qbittorrent.md) — VPN kill switch, VueTorrent UI

### Media stack
- [Jellyfin](app-jellyfin.md) — hardware transcoding, stream-check Watchtower hook
- [Sonarr](app-sonarr.md) — TV series management
- [Radarr](app-radarr.md) — movie management
- [Prowlarr](app-prowlarr.md) — indexer aggregation
- [Bazarr](app-bazarr.md) — subtitle management
- [Seerr](app-seerr.md) — media request portal

### Gateway stack
- [Nginx Proxy Manager](app-nginx-proxy-manager.md) — HTTPS reverse proxy
- [CrowdSec](app-crowdsec.md) — intrusion prevention
- [GoAccess](app-goaccess.md) — web log analytics

### Monitoring stack
- [Loki](app-loki.md) — log aggregation
- [Grafana](app-grafana.md) — dashboards
- [Uptime Kuma](app-uptime-kuma.md) — uptime monitoring

### Cloudflared stack
- [Cloudflared](app-cloudflared.md) — Cloudflare Tunnel daemon

### Shared (every stack)
- [Watchtower](app-watchtower.md) — automatic image updates
- [Promtail](app-promtail.md) — log shipping to Loki

---

## For AI Agents

- [LLM Context](llm-context.md) — dense structured reference: architecture, rules, quirks, all stacks
