# Use Case: GPU Passthrough

**Tier:** CLIENT + HOST hinting contract
**Status:** Implemented

---

## Implemented Scope

GPU passthrough app wiring is implemented in the CLIENT scaffolding flow.

Trigger in Scaffolding tab:

- [g] enable GPU for selected app row
- [G] disable GPU for selected app row

Behavior:

- Updates selected app docker-compose.yml with GPU device mappings
- Adds/removes group_add mappings for render/video groups
- Adds/removes jellyfin DOCKER_MODS hint when applicable
- Writes/removes hardware.gpu hint block in stack lxc-compose.yml
- Creates Git commit and queues stack sync when stack is active

---

## Files

- client-app/src/stack_features.rs
- client-app/src/events.rs
- client-app/src/ui.rs

---

## Host Responsibility

Actual Proxmox passthrough remains HOST-owned because device cgroup/mount entries are host-level LXC config operations.
The CLIENT-side hardware.gpu block is the host-execution hint contract.
