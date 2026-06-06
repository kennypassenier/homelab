# Architecture (Current)

Last updated: 2026-06-06

## System Model

- CLIENT is the only interactive UI and orchestrator.
- HOST and LXC are headless daemons.
- CLIENT calls HOST and CLIENT calls LXC.
- CLIENT may also call external control-plane APIs for Git-managed infrastructure intent, such as OPNsense DHCP reservation automation.
- HOST and LXC do not call each other directly.
- On the Proxmox host, the canonical local repo location for HOST is `~/homelab` (typically `/root/homelab`).
- HOST auto-loads `config/.env` from the cloned repo and runs headless when started without a TTY (for example under systemd).

## GitOps and Deployment Scope

- Source of truth is Git.
- LXC sparse checkout is strictly stack-scoped: stacks/<stack_name>/.
- Sparse scope is re-applied on each sync run to prevent drift.

## Storage and Runtime

- Persistent app data lives on host under /opt/appdata/<stack_name>.
- LXC consumes host data via bind mounts.
- Compose and hook files are managed from Git stack folders.

## Hardware Passthrough

- GPU passthrough is host-owned because it requires host-level LXC config and device cgroup/mount operations.
- CLIENT writes hardware.gpu hints in stack lxc-compose for orchestration intent and app-level compose wiring.

## Use Case Status

- Implemented references are in docs/usecases/implemented/.
- Remaining feature gaps are tracked explicitly in docs/usecases/pending/.
- Longer-horizon ideas are tracked in docs/usecases/planned/.

## Release and Delivery Model

- HOST binary updates are release-version driven (GitHub Releases), not direct push-triggered runtime updates.
- LXC daemon image publication to GHCR is CI-based and path-gated to daemon/workflow changes.
- LXC deploy telemetry is streamed back to CLIENT over the daemon WebSocket API during sync operations.
- CLIENT sends HOST heartbeat over websocket RPC with HTTP fallback, and HOST failsafe windows use that API-level heartbeat state (not SSH-written files).
