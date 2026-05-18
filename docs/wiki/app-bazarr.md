# App: Bazarr

> Subtitle management. Automatically downloads subtitles for movies and TV shows managed by Radarr and Sonarr.

## Container

| Field | Value |
|---|---|
| Image | `lscr.io/linuxserver/bazarr:latest` |
| Port | `6767` |
| Network | `media_network` |
| PUID/PGID | 1000/1000 |

## Volumes

| Host path | Container path |
|---|---|
| `/appdata/media/bazarr/config` | `/config` |
| `/mnt/data/18TB` | `/data/18TB` |
| `/mnt/data/12TB` | `/data/12TB` |

## See also

- [stack-media.md](stack-media.md)
- [app-sonarr.md](app-sonarr.md)
- [app-radarr.md](app-radarr.md)
