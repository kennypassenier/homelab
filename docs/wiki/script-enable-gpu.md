# enable-gpu.sh

> Appends Intel/AMD GPU passthrough configuration to a Proxmox LXC config file, allowing the container to access `/dev/dri` for hardware-accelerated transcoding.

## Overview

`scripts/host/enable-gpu.sh` modifies the Proxmox LXC configuration to expose the host GPU's DRM devices to an unprivileged LXC. This is used to enable hardware transcoding in [Jellyfin](app-jellyfin.md). The change requires restarting the LXC to take effect.

## Usage

```bash
./scripts/host/enable-gpu.sh [-h] <VMID>
# or via menu:
./host.sh → Enable GPU Passthrough for an LXC
```

| Argument | Description |
|---|---|
| `VMID` | Proxmox VMID of the LXC to configure |
| `-h` | Show help |

## What Gets Added to `/etc/pve/lxc/<VMID>.conf`

```ini
# Allow container cgroups to access GPU devices (card0 and renderD*)
lxc.cgroup2.devices.allow: c 226:0 rwm
lxc.cgroup2.devices.allow: c 226:128 rwm

# Bind mount the host's GPU nodes into the container
lxc.mount.entry: /dev/dri/card0 dev/dri/card0 none bind,optional,create=file
lxc.mount.entry: /dev/dri/renderD128 dev/dri/renderD128 none bind,optional,create=file
```

`226` is the Linux major device number for DRM (Direct Rendering Manager) devices.

## Idempotency

The script checks for `lxc.cgroup2.devices.allow: c 226:` before appending. If it is already present, it exits cleanly with a message. Safe to run multiple times.

## After Running

```bash
pct stop <VMID> && pct start <VMID>
```

Inside the LXC (via Docker Compose), expose the device:

```yaml
devices:
  - /dev/dri:/dev/dri
```

Jellyfin's compose file also adds the required group memberships (`group_add`) for access to `render` and `video` groups.

## See also

- [app-jellyfin.md](app-jellyfin.md)
- [script-host-sh.md](script-host-sh.md)
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
