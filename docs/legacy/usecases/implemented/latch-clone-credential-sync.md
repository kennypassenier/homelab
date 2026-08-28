# Use Case: Latch Clone Credential Sync

**Tier:** CLIENT + LXC  
**Status:** Implemented

---

## Implemented Scope

Secure credential migration is now implemented with command execution on both sides:

- CLIENT executes local commands with stdin/timeouts via shell module
- LXC daemon exposes `POST /api/exec` for remote command execution
- CLIENT orchestrates full latch clone flow: offer -> create -> apply
- LXC daemon exposes keyring readiness status endpoint (`GET /api/secrets/keyring`)
- command execution endpoint can be bearer-token protected via `LXC_API_TOKEN`

This enables encrypted, automation-friendly credential sync without plaintext temp files.

---

## Files

- client-app/src/shell.rs
- client-app/src/latch.rs
- lxc-daemon/src/api.rs
