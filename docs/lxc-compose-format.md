# lxc-compose.yml Format

This document defines the stack-level lxc-compose contract used by CLIENT, HOST, and future features.

## Required Keys

- version: integer schema version.
- stack_name: stack directory name under stacks/.
- vmid: Proxmox VMID (0 means not provisioned yet).
- hostname: LXC hostname.
- hwaddr: MAC address used for DHCP reservation.
- deploy.enabled: activation gate for deploy command.

## Optional Keys

- hardware.gpu.enabled: boolean host passthrough hint.
- hardware.gpu.profile: GPU profile identifier (for example intel_igpu).
- hardware.gpu.target_app: app folder name that should receive GPU compose wiring.

## Activation Contract

- deploy.enabled=false: stack is inactive for deploy trigger in CLIENT.
- deploy.enabled=true: stack is eligible for deploy trigger in CLIENT.
- deploy.activated_at: optional timestamp-like field set by CLIENT when activated.

## Example

See docs/examples/lxc-compose.example.yml.

## Notes For Future Features

- Keep this file stack-scoped and idempotent.
- Add new fields in backward-compatible manner.
- Do not remove deploy.enabled, because it is now the activation source of truth.

## Sparse Checkout Scope

- Every LXC daemon is constrained to `stacks/<stack_name>/` only.
- Sparse scope is re-applied on each sync run to prevent scope drift.
