# sync-host.sh

> Pulls the latest scripts from Git onto the Proxmox host and ensures all host scripts remain executable.

## Overview

`scripts/host/sync-host.sh` keeps the Proxmox host's local Git clone up to date. It is run automatically every hour by the cron job set up via [setup-cron.sh](script-setup-cron.md), and can also be triggered manually via `./host.sh → Sync Host scripts from Git`.

Unlike [node-sync.sh](script-node-sync.md) (which runs in LXC containers and deploys Docker apps), `sync-host.sh` only updates the host's own copy of the repository — it does not deploy any containers.

## Usage

```bash
./scripts/host/sync-host.sh [REPO_DIR]
# or via menu:
./host.sh → Sync Host scripts from Git
```

| Argument | Default | Description |
|---|---|---|
| `REPO_DIR` | `/root/homelab` | Path to the homelab Git repository on the host |

## What It Does

1. Verifies `REPO_DIR` is a valid Git repository (contains `.git/`)
2. `git fetch origin main --quiet`
3. `git reset --hard origin/main --quiet` — enforces the exact remote state, discarding any local modifications
4. `chmod +x scripts/host/*.sh` — ensures all host scripts remain executable after checkout

## Log Output

Timestamped plain-text lines are written to stdout (captured to `/var/log/host-sync.log` by cron):
```
[2026-05-18T12:00:01+02:00] Starting Proxmox host repository sync...
[2026-05-18T12:00:02+02:00] Permissions verified for host scripts.
[2026-05-18T12:00:02+02:00] Host sync completed successfully.
```

## See also

- [script-setup-cron.md](script-setup-cron.md)
- [script-host-sh.md](script-host-sh.md)
- [GitOps Flow](gitops-flow.md)
