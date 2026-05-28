# Use Case: Individual Backup Restore

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

A per-stack restore action is now available in the Backups tab.

Trigger:

- [i] in Backups tab

Behavior:

- Targets currently selected stack
- Opens operation progress modal with restore phase context
- Updates backup status line for restore preparation

---

## Files

- client-app/src/events.rs
- client-app/src/blast_radius.rs
- client-app/src/ui.rs

This creates the reusable CLIENT restore interaction path for single-stack recovery.
