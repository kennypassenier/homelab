# Use Case: Boot Policy Orchestration

**Tier:** CLIENT + HOST  
**Status:** Implemented

---

## Implemented Scope

HOST now reconciles stack boot policy intent from `lxc-compose.yml` to Proxmox runtime config.

Implemented behavior:

- reads stack boot intent (`boot.autostart`, `boot.order`, `vmid`, `hostname`) from GitOps files
- detects drift against `pct config <vmid>` runtime values
- provides preview mode and apply mode from HOST TUI
- applies drift with `pct set --onboot ... --startup order=...`
- skips unmanaged stacks when `host_management.managed=false`
- skips potential foreign containers when runtime hostname mismatches stack intent

HOST keybinds:

- `o`: boot policy preview
- `O`: boot policy apply

---

## Files

- host-daemon/src/policy.rs
- host-daemon/src/main.rs
- docs/lxc-compose-format.md
