# App: Seerr

> Media discovery and request portal. Users can browse and request movies/TV shows; approved requests are sent automatically to Radarr or Sonarr.

## Overview

Seerr is a fork/replacement for Jellyseerr. `pre-sync.sh` handles a one-time automated migration from the old `jellyseerr` data directory.

## Container

| Field | Value |
|---|---|
| Image | `ghcr.io/seerr-team/seerr:latest` |
| Port | `5055` |
| Network | `media_network` |
| PUID/PGID | 1000/1000 |

## Migration from Jellyseerr

`stacks/media/pre-sync.sh` runs before every `docker compose up`. If `/appdata/media/jellyseerr` exists, the script:
1. Stops any running `jellyseerr` container
2. Renames `/appdata/media/jellyseerr` → `/appdata/media/seerr`
3. Fixes ownership: `chown -R 1000:1000 /appdata/media/seerr`

This migration is idempotent — once the directory is renamed, the condition no longer triggers.

## Volumes

| Host path | Container path |
|---|---|
| `/appdata/media/seerr/config` | `/config` |

## See also

- [stack-media.md](stack-media.md)
- [app-radarr.md](app-radarr.md)
- [app-sonarr.md](app-sonarr.md)
- [app-jellyfin.md](app-jellyfin.md)
