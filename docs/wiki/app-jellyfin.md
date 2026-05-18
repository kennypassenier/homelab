# App: Jellyfin

> Media server with hardware-accelerated transcoding. GPU passthrough from the Proxmox host into the LXC, then into the container via device mounts.

## Overview

Jellyfin serves movies and TV shows from the NAS. It uses the Intel iGPU (or dedicated GPU) on the Proxmox host for hardware transcoding via `/dev/dri`. A custom healthcheck script (`check-streams.sh`) prevents Watchtower from restarting the container while any stream is actively playing.

## Container

| Field | Value |
|---|---|
| Image | `lscr.io/linuxserver/jellyfin:latest` |
| Port | `8096` |
| PUID/PGID | 1000/1000 |
| User (for GPU) | `user: "0:0"` — runs as root initially to access `/dev/dri` |
| shm size | `4gb` — shared memory for hardware transcode buffers |
| Transcode dir | `/dev/shm` — RAM disk for transcode working files |

## Hardware Transcoding

Jellyfin needs access to the GPU device nodes. The compose file mounts `/dev/dri` and adds the container to all relevant device groups.

```yaml
devices:
  - /dev/dri:/dev/dri
group_add:
  - "993"  # render group
  - "44"   # video group
  - "104"  # additional GPU group
  - "105"
  - "106"
  - "107"
```

Group IDs must match those on the **LXC** host, not the Proxmox hypervisor. See [enable-gpu.sh](script-enable-gpu.md) for how the GPU is passed from Proxmox into the LXC.

The environment variable `JELLYFIN_TRANSCODE_DIR=/dev/shm` tells Jellyfin to write transcode working files to tmpfs, dramatically reducing I/O and improving transcode performance.

## Watchtower Pre-Check: `check-streams.sh`

Before Watchtower stops the container for an update, it runs `check-streams.sh` as a lifecycle pre-check. If any stream is currently playing, the script exits non-zero and Watchtower aborts the update for this cycle.

```yaml
labels:
  com.centurylinklabs.watchtower.lifecycle.pre-check: "/check-streams.sh"
```

The script queries the Jellyfin Sessions API:
```bash
curl -s "http://localhost:8096/Sessions?apiKey=${JELLYFIN_API_KEY}" | grep -q '"IsPlaying": true'
```

`JELLYFIN_API_KEY` is loaded from a SOPS-encrypted `.env` file.

## Volumes

| Host path | Container path | Purpose |
|---|---|---|
| `/appdata/media/jellyfin/config` | `/config` | Jellyfin configuration + database |
| `/mnt/data/18TB` | `/data/18TB` | Primary media library (read-write) |
| `/mnt/data/12TB` | `/data/12TB` | Secondary media library (read-write) |

## Environment Variables

| Variable | Source | Description |
|---|---|---|
| `PUID`, `PGID`, `TZ` | compose | LinuxServer.io standard vars |
| `JELLYFIN_TRANSCODE_DIR` | compose | `/dev/shm` — RAM disk for transcode |
| `JELLYFIN_API_KEY` | `.env` (SOPS) | API key for check-streams.sh |

## Labels

| Label | Value |
|---|---|
| `com.centurylinklabs.watchtower.enable` | `true` |
| `com.centurylinklabs.watchtower.lifecycle.pre-check` | `/check-streams.sh` |
| `com.homelab.backup.pause` | `true` |

## See also

- [stack-media.md](stack-media.md)
- [script-enable-gpu.md](script-enable-gpu.md)
- [app-watchtower.md](app-watchtower.md)
