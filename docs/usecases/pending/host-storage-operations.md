# Pending Use Case: Host Storage Operations

**Tier:** HOST
**Status:** Pending

## Missing Behavior

HOST documentation claims storage operations as a responsibility, but there is not yet a real execution/API surface for them.

Expected behavior:

- inspect stack storage health on host paths
- validate bind mount prerequisites before deploy/restore flows
- expose storage actions/status to CLIENT
- keep operations idempotent and GitOps-compatible

## Candidate Files

- host-daemon/src/main.rs
- host-daemon/src/app.rs
- scripts/host/
