# setup-cron.sh

> Installs the hourly cron job for `sync-host.sh` and configures log rotation for the host sync log.

## Overview

`scripts/host/setup-cron.sh` is a one-time idempotent setup script that adds [sync-host.sh](script-sync-host.md) to the root crontab. It also writes a logrotate configuration so the log never grows unbounded.

## Usage

```bash
./scripts/host/setup-cron.sh [-d <REPO_DIR>] [-h]
# or via menu:
./host.sh → Setup Host Cronjob for automated sync
```

## Flags

| Flag | Default | Description |
|---|---|---|
| `-d REPO_DIR` | `/root/homelab` | Path to the homelab repository |
| `-h` | — | Show help and exit |

## What It Does

1. Verifies `sync-host.sh` exists at `<REPO_DIR>/scripts/host/sync-host.sh`
2. Makes it executable (`chmod +x`)
3. Checks if the cron entry already exists (`crontab -l | grep ...`) — skips if present
4. Appends to crontab:
   ```
   0 * * * * /root/homelab/scripts/host/sync-host.sh /root/homelab >> /var/log/host-sync.log 2>&1
   ```
5. Writes `/etc/logrotate.d/host-sync`:
   ```
   /var/log/host-sync.log { daily; rotate 7; compress; missingok; notifempty }
   ```

## Idempotency

The cron entry is only added if `grep` does not find the sync script path already in the crontab. The logrotate file is always overwritten (writing an identical config is harmless).

## See also

- [script-sync-host.md](script-sync-host.md)
- [script-host-sh.md](script-host-sh.md)
