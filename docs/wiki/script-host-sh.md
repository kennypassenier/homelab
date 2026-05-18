# host.sh — Host Manager

> Interactive numbered menu on the Proxmox host for all host-side homelab operations.

## Overview

`host.sh` is the entry point for all operations that must be run on the Proxmox VE host. It uses a plain numbered menu (no Gum dependency required on the host) and delegates to the appropriate script for each option. Run it from the repository root on the Proxmox host (`/root/homelab/`).

## Usage

```bash
./host.sh
```

## Menu Options

| Option | Script invoked | Description |
|---|---|---|
| 1 | [bootstrap-lxc.sh](script-bootstrap-lxc.md) | Bootstrap a new LXC container |
| 2 | [backup-stacks.sh](script-backup-stacks.md) | Run a Restic backup of all app data |
| 3 | [enable-gpu.sh](script-enable-gpu.md) | Enable Intel/AMD GPU passthrough for an LXC |
| 4 | [enable-tun.sh](script-enable-tun.md) | Enable `/dev/net/tun` passthrough for a VPN LXC |
| 5 | [reset-stack.sh](script-reset-stack.md) | Wipe Docker state and app data for a stack |
| 6 | [sync-host.sh](script-sync-host.md) | Pull latest scripts from Git |
| 7 | [setup-cron.sh](script-setup-cron.md) | Install hourly cron job for automated host sync |
| 0 | — | Exit |

## See also

- [script-client-sh.md](script-client-sh.md)
- [script-container-sh.md](script-container-sh.md)
- [Architecture Overview](architecture-overview.md)
