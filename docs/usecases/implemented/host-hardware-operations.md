# Use Case: Host Hardware Operations

**Tier:** HOST  
**Status:** Implemented

---

## Implemented Scope

HOST now exposes a unified runtime surface for hardware operations:

- checks host GPU readiness (IOMMU + detected devices)
- checks host TUN readiness (`/dev/net/tun`)
- discovers per-stack hardware intent from `stacks/<stack>/lxc-compose.yml`
- reconciles stack intent against host readiness and renders pass/fail outcomes in HOST UI

This closes the gap between CLIENT hardware hints and host-side execution/readiness visibility.

---

## Files

- host-daemon/src/hardware.rs
- host-daemon/src/main.rs
