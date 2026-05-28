# Architecture (Current)

Last updated: 2026-05-28

## System Model

- CLIENT is the only interactive UI and orchestrator.
- HOST and LXC are headless daemons.
- CLIENT calls HOST and CLIENT calls LXC.
- HOST and LXC do not call each other directly.

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

- All use cases in docs/usecases are implemented.
- Implemented references are in docs/usecases/implemented/.

## Release and Delivery Model

- HOST binary updates are release-version driven (GitHub Releases), not direct push-triggered runtime updates.
- LXC daemon image publication to GHCR is CI-based and path-gated to daemon/workflow changes.
