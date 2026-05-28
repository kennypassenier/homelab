# CLIENT Features (Current)

Last updated: 2026-05-28

## Scope

- Ratatui TUI for all interactive flows.
- Stack/app scaffolding, activation/deactivation, deploy/update queueing.
- Backup/restore/patch orchestration surfaces.
- Structured client logfmt-style event emission for critical operations.

## Implemented Highlights

- Add/delete stack and add/delete app flows.
- Core app management.
- Deploy selected and batch deploy/update of active stacks.
- Fail-closed pre-sync and filesystem-layout validation gates.
- Transaction ledger for add_stack and delete_stack phases.
- Reusable operation progress modal used by backup/restore/patch actions.
- GPU compose wiring toggles per selected app (g/G) and host hint writes to lxc-compose.
- Stack creation wizard now captures provisioning defaults (CPU 1-8, memory in 512 MiB steps, root disk GiB) and writes them into stack `lxc-compose.yml`.
- New stack defaults explicitly set `deploy.enabled=false` to keep manual activation as the safe default.

## Notes

- CLIENT remains GitOps-first and commits generated changes through the existing Git helper path.
- HOST-only operations (for example real GPU passthrough on Proxmox) are represented as CLIENT orchestration intent, not direct local host mutation.
