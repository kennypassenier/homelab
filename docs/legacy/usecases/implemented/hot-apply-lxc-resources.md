# Use Case: Hot-Apply LXC Resources

**Tier:** CLIENT + HOST  
**Status:** Implemented

---

## Implemented Scope

HOST now reconciles CPU and memory resource intent from `lxc-compose.yml` and applies hot-safe updates.

Implemented behavior:

- reads target resources (`resources.cores`, `resources.memory_mb`, `vmid`, `hostname`) from stack intent
- compares against Proxmox runtime config from `pct config <vmid>`
- classifies drift as:
  - hot-applicable (equal/increase or stopped container)
  - restart-required (running container with CPU/memory decrease)
- applies hot-applicable drift using `pct set --cores ... --memory ...`
- keeps restart-required changes in preview output without forced apply
- supports preview and apply modes from HOST TUI

HOST keybinds:

- `h`: resources preview
- `H`: hot-applicable resources apply

---

## Files

- host-daemon/src/policy.rs
- host-daemon/src/main.rs
