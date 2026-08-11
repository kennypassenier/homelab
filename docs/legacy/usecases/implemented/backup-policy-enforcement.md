# Use Case: Backup Policy Enforcement Service

**Tier:** CLIENT + HOST
**Status:** Implemented

## Behavior

- HOST continuously reads persisted backup policy from `~/.config/homelab/backup-schedule.json`.
- Scheduled backup cycles run by interval when policy is enabled.
- Each successful backup run applies retention policy with restic `forget --prune`.
- Manual and scheduled cycles cannot overlap due to a guarded single-cycle lock.

## Implemented In

- client-app/src/backup_schedule.rs
- host-daemon/src/main.rs
- host-daemon/src/backup.rs