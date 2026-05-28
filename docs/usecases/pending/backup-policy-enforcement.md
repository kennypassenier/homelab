# Pending Use Case: Backup Policy Enforcement Service

**Tier:** CLIENT + HOST
**Status:** Pending

## Missing Behavior

Backup scheduling is currently an editable CLIENT policy surface, but there is no continuous HOST service consuming and enforcing that policy.

Expected behavior:

- HOST reads persisted backup policy
- backups run continuously/service-driven rather than cron-defined in feature logic
- retention and notification settings are enforced centrally
- CLIENT receives status for next/last execution

## Candidate Files

- client-app/src/backup_schedule.rs
- host-daemon/src/main.rs
- host-daemon/src/backup.rs
