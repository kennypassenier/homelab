# Pending Use Case: Restore Execution Backend

**Tier:** CLIENT + HOST + LXC
**Status:** Pending

## Missing Behavior

Restore actions currently exist as CLIENT orchestration surfaces, but not as a completed backend workflow.

Expected behavior:

- restore a selected stack or full environment from actual backup data
- coordinate HOST storage restore, LXC service quiescing, and post-restore sync
- emit granular progress and failure states
- stay fail-closed on partial restore errors

## Candidate Files

- client-app/src/events.rs
- host-daemon/src/backup.rs
- lxc-daemon/src/api.rs
- lxc-daemon/src/main.rs
