# CLIENT Features (Current)

Last updated: 2026-05-28

## Scope

- Ratatui TUI for all interactive flows.
- Stack/app scaffolding, activation/deactivation, deploy/update queueing.
- Backup/restore/patch orchestration surfaces.
- Structured client logfmt-style event emission for critical operations.
- Live deploy telemetry streamed from LXC daemon WebSocket logs during sync actions.

## Implemented Highlights

- Add/delete stack and add/delete app flows.
- Core app management.
- Deploy selected and batch deploy/update of active stacks.
- Fail-closed pre-sync and filesystem-layout validation gates.
- Transaction ledger for add_stack and delete_stack phases.
- Reusable operation progress modal used by backup/restore/patch actions.
- GPU compose wiring toggles per selected app (g/G) and host hint writes to lxc-compose.
- Stack creation wizard now captures provisioning defaults (CPU 1-8, memory in 512 MiB steps, root disk GiB) and writes them into stack `lxc-compose.yml`.
- Stack creation wizard now captures boot policy defaults (autostart + boot order) and writes them into stack `lxc-compose.yml`.
- Stack config editor allows stack-level editing of deploy state, resources, hostname, MAC address, IP mode, and reserved IPv4 from the Scaffolding tab.
- Stack config editor allows stack-level editing of autostart and boot order policy.
- Stack config editor can sync stack-owned DHCP reservations to OPNsense Kea using the stack's deterministic MAC address and reserved IPv4 intent.
- App rows now expose a real config editor for Git-managed app metadata, starting with Docker image updates.
- New stack defaults explicitly set `deploy.enabled=false` to keep manual activation as the safe default.
- Latch clone orchestration module can perform offer/create/apply credential sync through local + LXC command execution.
- LXC naming standardization supports canonical `vmid-app-<stack>` hostnames while preserving legacy alias compatibility.

## Notes

- CLIENT remains GitOps-first and commits generated changes through the existing Git helper path.
- HOST-only operations (for example real GPU passthrough on Proxmox) are represented as CLIENT orchestration intent, not direct local host mutation.
- DHCP automation only mutates reservations proven to be homelab stack-owned; unrelated/manual reservations are treated as hard conflicts.
