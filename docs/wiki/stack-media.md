# Stack: Media

> Full Arr stack — Jellyfin media server with Sonarr, Radarr, Prowlarr, Bazarr, and Seerr. All services share a `media_network` Docker bridge for inter-service communication.

## Overview

The media stack provides a complete self-hosted media management and streaming pipeline. All apps are in separate compose projects but communicate over the shared `media_network` bridge network, created by `pre-sync.sh` before any app is deployed.

The LXC requires [GPU passthrough](script-enable-gpu.md) for Jellyfin hardware transcoding.

## Apps

| App | Purpose | Port |
|---|---|---|
| [Jellyfin](app-jellyfin.md) | Media server + transcoding | 8096 |
| [Sonarr](app-sonarr.md) | TV series management | 8989 |
| [Radarr](app-radarr.md) | Movie management | 7878 |
| [Prowlarr](app-prowlarr.md) | Indexer management (feeds Sonarr + Radarr) | 9696 |
| [Bazarr](app-bazarr.md) | Subtitle management | 6767 |
| [Seerr](app-seerr.md) | Media request / discovery | 5055 |
| [Watchtower](app-watchtower.md) | Automatic image updates | — |
| [Promtail](app-promtail.md) | Log shipping to Loki | — |

## Network

All apps join `media_network` (external Docker bridge). `pre-sync.sh` creates it idempotently before compose runs. Each compose file declares:

```yaml
networks:
  media_network:
    name: media_network
    external: true
```

## `pre-sync.sh`

`stacks/media/pre-sync.sh` does two things:
1. Creates `media_network` if it doesn't exist
2. **Jellyseerr → Seerr migration**: if `/appdata/media/jellyseerr` exists, stops the old container, renames the data directory to `seerr`, and fixes ownership (`chown -R 1000:1000`)

## Storage

| Host path | Container path | Purpose |
|---|---|---|
| `/opt/appdata/media/<app>/config` | `/config` | Per-app configuration |
| `/mnt/data/18TB` | `/data/18TB` | Primary media library |
| `/mnt/data/12TB` | `/data/12TB` | Secondary media library |

## LXC Requirements

- GPU passthrough for Jellyfin hardware transcoding — configured via [enable-gpu.sh](script-enable-gpu.md)
- The media storage arrays (`/mnt/data/18TB`, `/mnt/data/12TB`) must be bind-mounted into the LXC in its Proxmox config

## DNS Configuration

Sonarr, Radarr, Prowlarr, and Bazarr specify explicit DNS servers (`8.8.8.8`, `1.1.1.1`) to ensure reliable resolution of indexer and tracker hostnames, bypassing any local DNS issues.

## See also

- [app-jellyfin.md](app-jellyfin.md)
- [app-sonarr.md](app-sonarr.md)
- [app-radarr.md](app-radarr.md)
- [app-prowlarr.md](app-prowlarr.md)
- [app-bazarr.md](app-bazarr.md)
- [app-seerr.md](app-seerr.md)
- [Networking](networking.md)
- [script-enable-gpu.md](script-enable-gpu.md)
