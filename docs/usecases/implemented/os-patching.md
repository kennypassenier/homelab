# Use Case: OS Patching

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

Patch-all orchestration path is now available in Backups tab.

Trigger:

- [p] in Backups tab

Behavior:

- Generates per-stack patch queue entries
- Opens operation progress modal for patch orchestration visibility
- Updates backup status line to reflect patch plan

---

## Files

- client-app/src/events.rs
- client-app/src/blast_radius.rs
- client-app/src/ui.rs

This delivers the CLIENT interaction path for batch patch workflows.
