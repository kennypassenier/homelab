# Use Case: Full Backup Restore

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

A disaster-recovery planning flow is now available in the Backups tab.

Trigger:

- [r] in Backups tab

Behavior:

- Opens operation progress modal
- Queues per-stack restore dispatch through LXC `POST /api/restore`
- Displays backend phase events and final state in operation progress modal

---

## Files

- client-app/src/events.rs
- client-app/src/blast_radius.rs
- client-app/src/ui.rs

This implementation establishes the CLIENT orchestration surface for full restore workflows.
