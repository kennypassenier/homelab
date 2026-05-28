# Pending Use Case: Host Hardware Operations

**Tier:** HOST
**Status:** Pending

## Missing Behavior

GPU/TUN passthrough is recognized in architecture and partially scaffolded from CLIENT hints, but HOST still lacks a unified execution surface for reviewing/applying hardware operations.

Expected behavior:

- inspect per-stack hardware intent from `lxc-compose.yml`
- show host-side readiness for GPU/TUN passthrough
- apply or reconcile host configuration safely
- report status/events back to CLIENT

## Candidate Files

- host-daemon/src/main.rs
- host-daemon/src/self_update.rs
- scripts/host/enable-gpu.sh
- scripts/host/enable-tun.sh
