# App: Prowlarr

> Indexer aggregator. Manages torrent tracker and Usenet indexer credentials in one place and syncs them to Sonarr and Radarr automatically.

## Container

| Field | Value |
|---|---|
| Image | `lscr.io/linuxserver/prowlarr:latest` |
| Port | `9696` |
| Network | `media_network` |
| PUID/PGID | 1000/1000 |

## Volumes

| Host path | Container path |
|---|---|
| `/appdata/media/prowlarr/config` | `/config` |

## See also

- [stack-media.md](stack-media.md)
- [app-sonarr.md](app-sonarr.md)
- [app-radarr.md](app-radarr.md)
