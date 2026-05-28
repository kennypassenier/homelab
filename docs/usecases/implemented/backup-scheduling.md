# Use Case: Backup Scheduling

**Tier:** CLIENT + HOST + LXC continuous services
**Status:** Implemented

---

## 1. Continuous-Service Model

Backups are no longer modeled as cron jobs.

Policy is interval-based for continuously running services:

- enabled
- interval_minutes
- retention_daily
- retention_weekly
- retention_monthly
- notify_on_success
- notify_on_failure

---

## 2. Implemented Behavior

Implemented in CLIENT:

- Dedicated Backups tab.
- In-memory editable backup policy.
- Persist policy to local config file:
  - ~/.config/homelab/backup-schedule.json
- Hotkeys for editing and saving policy.

Implemented in HOST:

- Continuous policy enforcer loop that polls persisted policy.
- Interval-driven scheduled backup cycles (service-based, no cron logic).
- Restic retention enforcement from policy (`forget --keep-daily/weekly/monthly --prune`).
- Concurrency guard to avoid overlapping manual and scheduled backup cycles.

---

## 3. Shared Module

Implemented via client-app/src/backup_schedule.rs:

- BackupSchedule struct
- load_or_default()
- save()

This module is now consumed by HOST enforcement through compatible policy fields.

---

## 4. Keybinds

In Backups tab:

- e: toggle enabled
- +/-: adjust interval_minutes by 15
- d/D: daily retention +/-
- w/W: weekly retention +/-
- m/M: monthly retention +/-
- n: toggle notify_on_success
- f: toggle notify_on_failure
- s: save policy

## 5. Enforcement Notes

- Schedule notifications are currently policy fields only; delivery backends are planned under `docs/usecases/planned/notification-routing.md`.
- Scheduled cycle progress appears in HOST backup status output.
