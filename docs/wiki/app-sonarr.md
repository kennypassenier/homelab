# App: Sonarr

> TV series management. Monitors RSS feeds, sends download requests to qBittorrent via Prowlarr indexers, and organises completed downloads.

## Container

| Field | Value |
|---|---|
| Image | `lscr.io/linuxserver/sonarr:latest` |
| Port | `8989` |
| Network | `media_network` |
| PUID/PGID | 1000/1000 |
| DNS | `8.8.8.8`, `1.1.1.1` |

## Volumes

| Host path | Container path |
|---|---|
| `/appdata/media/sonarr/config` | `/config` |
| `/mnt/data/18TB` | `/data/18TB` |
| `/mnt/data/12TB` | `/data/12TB` |

## See also

- [stack-media.md](stack-media.md)
- [app-prowlarr.md](app-prowlarr.md)
- [app-radarr.md](app-radarr.md)
