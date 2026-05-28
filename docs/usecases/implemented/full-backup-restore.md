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
- Builds per-stack DR plan entries
- Displays phase and summary for provision + restore + sync orchestration path

---

## Files

- client-app/src/events.rs
- client-app/src/blast_radius.rs
- client-app/src/ui.rs

This implementation establishes the CLIENT orchestration surface for full restore workflows.
