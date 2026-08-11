# Use Case: Unattended Upgrades

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

Unattended-upgrades status surface is available via Backups tab action.

Trigger:

- [u] in Backups tab

Behavior:

- Builds per-stack policy status entries
- Shows operation progress modal with unattended-upgrades context
- Updates status line after refresh

---

## Files

- client-app/src/events.rs
- client-app/src/blast_radius.rs
- client-app/src/ui.rs

This adds a reusable policy visibility path across stacks.
