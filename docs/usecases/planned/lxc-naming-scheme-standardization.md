# Planned Use Case: Uniform LXC Naming Scheme

**Tier:** CLIENT + HOST + DHCP integration
**Status:** Planned

## Goal

Adopt one deterministic naming convention visible across Proxmox, SSH aliases, and DHCP reservations.

Proposed pattern:

- `vmid-(app|infra)-<stack>`

Examples:

- `104-app-media`
- `110-infra-monitoring`

## Why Planned

- VMID ownership and lifecycle are still partly external to CLIENT provisioning
- migration needs a safe rename strategy to avoid breaking existing hostnames/reservations

## Migration Considerations

- keep old aliases during transition
- update DHCP reservations and SSH host entries atomically
- provide dry-run and blast-radius summary before applying