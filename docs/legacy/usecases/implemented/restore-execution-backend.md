# Use Case: Restore Execution Backend

**Tier:** CLIENT + LXC  
**Status:** Implemented

---

## Implemented Scope

Restore now has a concrete backend workflow exposed by LXC:

- new `POST /api/restore` endpoint executes restore requests
- restore workflow phases: validate backup -> quiesce services -> restore storage -> sync applications
- storage restore copies backup payloads into target appdata paths using `rsync --delete`
- response includes granular phase events, progress, and fail-closed error state
- successful restore triggers follow-up GitOps sync request in daemon state
- CLIENT backup restore actions now dispatch real restore API calls and render backend events
- integration-style restore tests cover success path and failure resume safeguard path
- endpoint access is guarded by bearer token when `LXC_API_TOKEN` is configured

CLIENT restore surfaces can now target a real backend execution path.

---

## Files

- lxc-daemon/src/api.rs
- lxc-daemon/src/restore.rs
- client-app/src/events.rs
