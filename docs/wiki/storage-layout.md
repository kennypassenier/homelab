# Storage Layout

> Application data lives on the Proxmox host's NVMe SSD at `/opt/appdata/<STACK>` and is bind-mounted into LXC containers at `/appdata`.

## Overview

All persistent container data is stored on the Proxmox host — not inside the LXC filesystem. This means data survives LXC resets, and backups only need to target one location on the host. The LXC has access to its data through an unprivileged bind mount.

## Directory Structure

```
Proxmox Host (NVMe SSD)
/opt/appdata/
├── media/
│   ├── jellyfin/config/
│   ├── sonarr/config/
│   ├── radarr/config/
│   ├── prowlarr/config/
│   ├── bazarr/config/
│   └── seerr/config/
├── gateway/
│   ├── nginx-proxy-manager/data/
│   ├── nginx-proxy-manager/letsencrypt/
│   ├── crowdsec/config/
│   ├── crowdsec/data/
│   └── goaccess/data/
├── monitoring/
│   ├── grafana/config/
│   ├── loki/config/
│   └── uptime-kuma/config/
├── paperless/
│   ├── data/
│   ├── media/
│   ├── export/
│   ├── consume/
│   ├── pgdata/
│   ├── redisdata/
│   └── ai-data/
├── downloader/
│   └── qbittorrent/config/
└── cloudflared/
    └── cloudflared/config/
```

Inside each LXC, the host path is mounted at `/appdata` (without the `/opt` prefix). Docker Compose files reference it as `/appdata/<STACK>/<APP>/...`:

```yaml
# In a docker-compose.yml inside the media LXC:
volumes:
  - /appdata/media/jellyfin/config:/config
  # This resolves to /opt/appdata/media/jellyfin/config on the Proxmox host
```

## Media Storage

Large media files (movies, TV shows) are stored on separate spinning-disk arrays and mounted directly from the host into the media LXC:

| Host path | Mount in LXC | Purpose |
|---|---|---|
| `/mnt/data/18TB` | `/mnt/data/18TB` | Primary media library |
| `/mnt/data/12TB` | `/mnt/data/12TB` | Secondary media library |
| `/mnt/downloads` | `/mnt/downloads` | qBittorrent download target |

These mounts are configured in the LXC config on the Proxmox host. They are separate from the `appdata` bind mount.

## Bind Mount Configuration

The bind mount from `/opt/appdata/<STACK>` to `/appdata` is set up during [bootstrap-lxc.sh](script-bootstrap-lxc.md). The entry in the LXC configuration file (`/etc/pve/lxc/<VMID>.conf`) looks like:

```
mp0: /opt/appdata/media,mp=/appdata
```

This is an unprivileged bind mount, meaning UID/GID mapping is applied. PUID=1000 in Docker Compose maps to the host's user 101000 in the storage.

## Garbage Collection and Storage

When [Garbage Collection](gitops-flow.md) runs during a sync, it deletes the entire `/appdata/<STACK>/<APP>` directory. This is permanent and cannot be undone. The deletion uses:

```bash
rm -rf "/appdata/${STACK_NAME}/${app_name}"
```

The `${:?}` guard in the script prevents accidental root-level deletion if either variable is empty.

## Backup Coverage

[Restic backups](backups.md) target `/opt/appdata` on the Proxmox host, which covers all application configuration data for all stacks in a single pass. The media files on `/mnt/data` are not backed up by Restic — they are assumed to be replaceable.

## See also

- [Backups](backups.md)
- [GitOps Flow](gitops-flow.md) — garbage collection deletes from `/appdata`
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
- [Architecture Overview](architecture-overview.md)
