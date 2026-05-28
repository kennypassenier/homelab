# lxc-compose.yml Format

This document defines the stack-level lxc-compose contract used by CLIENT, HOST, and future features.

## Required Keys

- version: integer schema version.
- stack_name: stack directory name under stacks/.
- vmid: Proxmox VMID (0 means not provisioned yet).
- hostname: LXC hostname (preferred canonical format `vmid-app-<stack>`; legacy `lxc-<stack>` still tolerated).
- hwaddr: MAC address used for DHCP reservation.
- deploy.enabled: activation gate for deploy command.
- network.bridge: Proxmox bridge name used by the stack.
- network.ip_mode: current network intent (for example `dhcp-reserved` or `manual`).
- network.reserved_ipv4: desired DHCP reservation address when `ip_mode=dhcp-reserved`.
- boot.autostart: whether the LXC should auto-start on host boot (default true).
- boot.order: startup order hint; higher values start later (default 90).
- resources.cores: CPU core allocation hint.
- resources.memory_mb: memory allocation hint in MiB.
- resources.disk_gb: root disk allocation hint in GiB.

## Optional Keys

- hardware.gpu.enabled: boolean host passthrough hint.
- hardware.gpu.profile: GPU profile identifier (for example intel_igpu).
- hardware.gpu.target_app: app folder name that should receive GPU compose wiring.
- host_management.managed: when false, HOST reconciliation skips this container for boot/resource applies.

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
- CLIENT stack editing now normalizes resource values into the `resources` block.
- Deterministic MAC addresses are derived from stack identity unless explicitly overridden.
- Legacy top-level resource keys are tolerated on read and normalized on save.

## Sparse Checkout Scope

- Every LXC daemon is constrained to `stacks/<stack_name>/` only.
- Sparse scope is re-applied on each sync run to prevent scope drift.
