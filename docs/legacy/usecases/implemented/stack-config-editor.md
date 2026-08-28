# Use Case: Stack Config Editor

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

A stack-level config editor is now available from the Scaffolding tab action list.

Trigger:

- Select a stack
- Move to Actions
- Choose `stack config`
- Press `Enter`

Behavior:

- Reads stack `lxc-compose.yml`
- Edits deploy activation state
- Edits CPU / memory / disk defaults
- Edits hostname and MAC address
- Edits network `ip_mode`
- Preserves unrelated config blocks while saving
- Commits and pushes the updated stack config through the existing GitOps path

---

## Files

- client-app/src/blast_radius.rs
- client-app/src/events.rs
- client-app/src/scaffold.rs
- client-app/src/ui.rs

---

## Notes

The editor writes the normalized `network` and `resources` blocks in `lxc-compose.yml` and keeps deploy activation GitOps-controlled.
