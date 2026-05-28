# Use Case: Backup Scheduling

**Tier:** CLIENT (policy editing) + HOST/LXC continuous services
**Status:** Implemented (CLIENT policy layer)

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

No cron expression is used.

---

## 3. Shared Module

Implemented via client-app/src/backup_schedule.rs:

- BackupSchedule struct
- load_or_default()
- save()

This module is reusable for future HOST API integration.

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
