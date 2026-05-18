# enable-tun.sh

> Configures `/dev/net/tun` passthrough for an unprivileged LXC, required for VPN containers (Gluetun). Auto-detects whether the stack actually needs TUN — exits cleanly if not.

## Overview

`scripts/host/enable-tun.sh` modifies the Proxmox LXC config to expose the host's TUN device. It is required for the [downloader stack](stack-downloader.md) which uses [Gluetun](app-qbittorrent.md) as a VPN kill switch. [bootstrap-lxc.sh](script-bootstrap-lxc.md) calls this automatically during bootstrap; this script handles the retroactive case for already-running LXCs.

## Usage

```bash
./scripts/host/enable-tun.sh [-h] <VMID>
# or via menu:
./host.sh → Enable TUN Passthrough for an LXC (VPN)
```

| Argument | Description |
|---|---|
| `VMID` | Proxmox VMID of the LXC to configure |
| `-h` | Show help |

## Auto-Detection

The script reads the stack name from the LXC's cron job:
```bash
pct exec <VMID> -- bash -c "grep -o 'node-sync.sh [^ ]*' /etc/cron.d/gitops-sync | awk '{print \$2}'"
```

It then scans all compose files in `stacks/<STACK>/` for `/dev/net/tun`. If not found, it exits immediately without modifying anything — safe to run on any LXC.

## What Gets Added to `/etc/pve/lxc/<VMID>.conf`

```ini
# 10:200 is the major:minor number for /dev/net/tun on Linux
lxc.cgroup2.devices.allow: c 10:200 rwm
lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file
```

## Idempotency

Checks for `lxc.cgroup2.devices.allow: c 10:200` before appending. Safe to run multiple times.

## Prerequisites

The TUN kernel module must be loaded on the Proxmox host:
```bash
modprobe tun
```

The script verifies `/dev/net/tun` exists on the host before proceeding.

## After Running

The LXC must be restarted. The script prompts for confirmation. On next [node-sync.sh](script-node-sync.md) run, Gluetun will be able to create the TUN interface.

## See also

- [app-qbittorrent.md](app-qbittorrent.md) — Gluetun VPN kill switch
- [stack-downloader.md](stack-downloader.md)
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
- [script-host-sh.md](script-host-sh.md)
