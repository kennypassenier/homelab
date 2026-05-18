# reset-stack.sh

> Wipes Docker state and all application data for a stack inside a running LXC, leaving the container itself intact for a clean re-deploy.

## Overview

`scripts/host/reset-stack.sh` is a recovery tool for corrupted stacks. It stops all containers, removes them, prunes volumes, wipes the GitOps clone inside the LXC, and deletes the app data from `/opt/appdata/<STACK>`. The LXC container itself (IP, config, bind mount) is preserved. After the reset, running [node-sync.sh](script-node-sync.md) inside the LXC starts the stack from scratch.

**This is destructive and permanent.** Use only when a stack is in an unrecoverable state.

## Usage

```bash
./scripts/host/reset-stack.sh [OPTIONS] <VMID> <STACK_NAME>
# or via menu:
./host.sh → Reset a corrupted Stack
```

## Flags

| Flag | Description |
|---|---|
| `VMID` | Proxmox VMID of the LXC |
| `STACK_NAME` | Stack name (used to locate `/opt/appdata/<STACK>`) |
| `-y` | Skip interactive confirmation — for scripted use |
| `-h` | Show help |

## What Gets Deleted

Inside the LXC (via `pct exec`):
- All running and stopped Docker containers: `docker stop && docker rm`
- All Docker volumes: `docker volume rm`
- The GitOps clone: `rm -rf /opt/gitops`

On the Proxmox host:
- All app data: `rm -rf /opt/appdata/<STACK>/*` (contents only, directory and bind mount preserved)

## Recovery After Reset

```bash
# Trigger a fresh sync from inside the LXC:
./container.sh → Trigger Node Sync
# or directly:
/opt/gitops/scripts/container/node-sync.sh <STACK_NAME>
```

The sync will clone the repo, run `pre-sync.sh`, pull images, and bring up all containers from the Git configuration.

## See also

- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
- [script-node-sync.md](script-node-sync.md)
- [script-host-sh.md](script-host-sh.md)
