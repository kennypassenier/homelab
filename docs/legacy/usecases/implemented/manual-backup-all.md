# Use Case: Manual Backup - All Stacks

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

Manual backup-all orchestration entrypoint is now available in Backups tab.

Trigger:

- [b] in Backups tab

Behavior:

- Builds operation entries for all discovered stacks
- Opens operation progress modal
- Updates backup status line and keeps operation summary visible

---

## Files

- client-app/src/events.rs
- client-app/src/blast_radius.rs
- client-app/src/ui.rs
