# LLM Context (Current)

Last updated: 2026-05-28

## Architecture Summary

- CLIENT: sole interactive orchestrator (Ratatui).
- HOST: headless Proxmox-side execution tier.
- LXC: headless stack runtime tier.
- Communication is CLIENT->HOST and CLIENT->LXC only.

## Operational Rules

- GitOps-first: change source in repo, then deploy via orchestration.
- Prefer fail-closed behavior for pre-flight and deploy gates.
- Keep stack operations idempotent and module-driven.

## Sparse Checkout Rule

- LXC Git working copy must be restricted to stacks/<stack_name>/ only.
- Sparse scope should be re-applied during sync to avoid drift.

## GPU Passthrough Rule

- Actual passthrough is HOST responsibility.
- CLIENT can update app compose wiring and write hardware hints to lxc-compose.

## Documentation State

- Implemented use cases are stored under docs/usecases/implemented/.
- Remaining gaps are tracked under docs/usecases/pending/.
- Legacy refactor phase documents were retired.

## Delivery Model Updates

- HOST self-update is release-based (version/tag check), not push-based.
- LXC daemon image delivery is automated via GHCR workflow with path-based change gating.
- CLIENT stack wizard sets CPU/RAM/Disk defaults in `lxc-compose.yml` and keeps deploy disabled until explicit activation.
- CLIENT streams live deploy logs from the LXC daemon during sync actions.
- CLIENT app rows include a Git-managed config editor for Docker image updates.
- CLIENT can sync stack-owned DHCP reservations to OPNsense from `lxc-compose.yml` network intent.
