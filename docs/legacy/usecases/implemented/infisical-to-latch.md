# Use Case: Replace Infisical with Latch for Secret Management

**Tier:** CLIENT + HOST + LXC
**Status:** Implemented

---

## Implemented Scope

Latch now replaces the Infisical-dependent secret workflow across the homelab:

- HOST bootstrap injects `LATCH_*` values instead of Infisical variables
- stack generation and deployment docs point at Latch pull/clone flows
- the shared env bundle and per-service env templates now carry Latch credentials and the corrected LXC IPs
- Latch Clone is documented as the machine-transfer path for credential state during LXC deployment

This keeps the repository aligned around one secret workflow for local editing, host bootstrap, and LXC provisioning.

---

## Files

- host-daemon/src/bootstrap.rs
- scripts/shared/sync-env-bundle.sh
- config/.env.example
- config/env.bundle.example
- docs/deployment.md
- docs/latch-clone-setup.md
- docs/usecases/pending/infisical-to-latch.md
