# Use Case: Static IP Automation

**Tier:** CLIENT + external network API
**Status:** Implemented

## Behavior

- Stack MAC addresses are deterministic by default, derived from stack identity.
- Stack config stores a `network.reserved_ipv4` intent alongside `ip_mode`.
- CLIENT can upsert DHCP reservations in OPNsense Kea for stack-owned entries.
- Conflicts with unrelated/manual reservations fail closed instead of being overwritten.
- Existing stack-owned reservations can be updated or replaced safely when hostname, IP, or MAC changes.

## Implemented In

- client-app/src/scaffold.rs
- client-app/src/events.rs
- client-app/src/opnsense.rs
- docs/lxc-compose-format.md