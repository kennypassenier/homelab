# client.sh — Client Manager

> Interactive menu on the Linux desktop for all client-side homelab operations.

## Overview

`client.sh` is the primary entry point for day-to-day homelab management from the developer's workstation. It presents a single interactive menu that delegates to the appropriate script for each operation. Run it from the root of the repository.

## Usage

```bash
./client.sh
```

Must be run from the repository root (the script checks for `stacks/` and `scripts/` directories).

## Menu Options

| Option | Script invoked | Description |
|---|---|---|
| Create a new Stack | [create-new-stack.sh](script-create-new-stack.md) | Scaffold a new LXC stack with optional Watchtower and Promtail |
| Create a new App inside a Stack | [create-new-app.sh](script-create-new-app.md) | Add a new app template to an existing stack |
| Remove an App | [remove-app.sh](script-remove-app.md) | Delete an app from Git (triggers GC on next sync) |
| Remove an entire Stack | [remove-stack.sh](script-remove-stack.md) | Delete a full stack from Git (triggers GC on next sync) |
| Register SSH alias for a new LXC | [add-ssh.sh](script-add-ssh.md) | Add or update `~/.ssh/config` alias for an LXC |

| Exit | — | Exits the menu loop |

## Key Setup Guard



## See also

- [host.sh](script-host-sh.md) — equivalent menu for Proxmox host operations
- [container.sh](script-container-sh.md) — equivalent menu for LXC container operations
- [lib-ui.md](lib-ui.md)
- [Architecture Overview](architecture-overview.md)
