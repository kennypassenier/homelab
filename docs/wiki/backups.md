# Backups

> Restic backs up all application data from the Proxmox host, automatically pausing and resuming Docker containers marked with `com.homelab.backup.pause=true`.

## Overview

Backups are centralised on the Proxmox host using [Restic](https://restic.net/). Because all container data is stored on the host at `/opt/appdata` (see [storage-layout.md](storage-layout.md)), a single Restic run covers every stack. Containers that hold databases or critical state are paused during the backup to avoid file corruption.

## How It Works

The backup is run by [backup-stacks.sh](script-backup-stacks.md) (`scripts/host/backup-stacks.sh`).

### Steps

1. **Find running LXCs** — `pct list` returns all running containers on the Proxmox host.
2. **Pause containers** — For each LXC that has Docker, pause every container labelled `com.homelab.backup.pause=true`. Pausing freezes process execution without stopping the container, so the on-disk state is consistent.
3. **Run Restic** — `restic backup /opt/appdata --cleanup-cache` backs up all application data.
4. **Resume containers** — A `trap cleanup EXIT` ensures containers are always unpaused, even if the script crashes or is interrupted with Ctrl+C.

```bash
# Simplified flow
for VMID in $LXC_IDS; do
    docker pause <containers with label>
done

restic backup /opt/appdata --cleanup-cache

# trap guarantees this runs even on error:
for VMID in $LXC_IDS; do
    docker unpause <paused containers>
done
```

### The `com.homelab.backup.pause=true` Label

Any service that should be paused during backup must carry this label in its `docker-compose.yml`:

```yaml
labels:
  - "com.homelab.backup.pause=true"
```

Services that hold databases (PostgreSQL, Redis, Loki) or application state (Jellyfin, qBittorrent, Paperless) all carry this label. Stateless services like Watchtower do not.

## Configuration

Restic credentials are loaded from a `.env` file on the Proxmox host at `scripts/host/.env` (not committed to Git, `chmod 600`):

```bash
RESTIC_REPOSITORY=s3:https://s3.example.com/mybucket
RESTIC_PASSWORD=your-strong-restic-password
```

If either variable is missing, the script exits immediately with a clear error — it never runs with empty credentials.

## Running the Backup

```bash
# On the Proxmox host:
./host.sh → Backup Stacks (Restic)
# or directly:
./scripts/host/backup-stacks.sh
```

## What Is Not Backed Up

- **Media files** (`/mnt/data/18TB`, `/mnt/data/12TB`) — assumed to be replaceable (re-downloadable)
- **LXC container filesystems** — only the app data bind-mounted from `/opt/appdata` is backed up
- The host's own `/root/homelab` Git clone — recoverable by re-cloning

## See also

- [script-backup-stacks.md](script-backup-stacks.md)
- [Storage Layout](storage-layout.md)
- [Architecture Overview](architecture-overview.md)
