# Use Case: Backup Scheduling

**Tier:** HOST (runs Restic on schedule) + CLIENT (configures schedule, views schedule status)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Automated Restic backups run on a configurable schedule managed by the HOST daemon's internal scheduler (tokio cron). The CLIENT is the only way to configure the schedule — it sends `POST /api/backup/schedule` to HOST. No cron jobs are written to the Proxmox host's crontab.

The scheduled backup uses the same flow as `manual-backup-all.md` — it calls the identical internal backup function, including the LXC pause/resume cycle.

---

## 2. Schedule Configuration

### CLIENT UI

"Backups" tab → "Backup Schedule" section → edit schedule fields:

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `true` | Enable/disable scheduled backups |
| `cron_expression` | string | `"0 3 * * *"` | Standard 5-field cron (min hr dom mon dow) |
| `retention_daily` | int | `7` | Keep last N daily snapshots |
| `retention_weekly` | int | `4` | Keep last N weekly snapshots |
| `retention_monthly` | int | `3` | Keep last N monthly snapshots |
| `notify_on_success` | bool | `false` | Send webhook alert on successful backup |
| `notify_on_failure` | bool | `true` | Send webhook alert on failure |

API call to HOST:
```
POST https://<host_ip>:8443/api/backup/schedule
Authorization: Bearer <host_token>
Content-Type: application/json

{
  "enabled": true,
  "cron_expression": "0 3 * * *",
  "retention_daily": 7,
  "retention_weekly": 4,
  "retention_monthly": 3,
  "notify_on_success": false,
  "notify_on_failure": true
}
```

The HOST daemon persists this schedule to `/etc/homelab/backup-schedule.json` and immediately reloads the cron scheduler.

---

## 3. Schedule Storage (HOST)

```
/etc/homelab/
└── backup-schedule.json   — persists the current schedule config
```

On HOST daemon startup, `/etc/homelab/backup-schedule.json` is read and the scheduler is initialized. If the file is missing, the HOST daemon starts with `enabled: false` (no scheduled backup).

---

## 4. HOST Daemon Scheduler

```rust
// HOST daemon: main loop
let schedule: BackupSchedule = load_schedule_or_default();
if schedule.enabled {
    let cron = cron::Schedule::from_str(&schedule.cron_expression)?;
    let mut cron_stream = cron.upcoming(Utc);
    loop {
        let next_run = cron_stream.next().unwrap();
        let wait = next_run - Utc::now();
        tokio::time::sleep(wait.to_std()?).await;
        run_backup(&schedule).await;  // Same as POST /api/backup/run
    }
}
```

---

## 5. Retention Policy Enforcement

After each successful backup, the HOST daemon runs `restic forget` with the configured retention policy:

```bash
restic -r <repo> forget \
  --keep-daily  7  \
  --keep-weekly 4  \
  --keep-monthly 3 \
  --prune
```

`--prune` removes the underlying data files immediately, not just the snapshot metadata. This keeps the repository size bounded.

**Logfmt events:**
```
ts=<ISO8601> level=info component=host msg="retention policy applied" kept=<N> removed=<M> freed_bytes=<B>
```

---

## 6. Next Run Display in CLIENT

The CLIENT queries the HOST for schedule status:

```
GET https://<host_ip>:8443/api/backup/schedule
Authorization: Bearer <host_token>
```

Response:
```json
{
  "enabled": true,
  "cron_expression": "0 3 * * *",
  "next_run": "2026-05-30T03:00:00Z",
  "last_run": {
    "started_at": "2026-05-29T03:00:00Z",
    "completed_at": "2026-05-29T03:06:41Z",
    "snapshot_id": "abc1234f",
    "status": "success",
    "data_added_bytes": 1342177280
  },
  "retention_daily": 7,
  "retention_weekly": 4,
  "retention_monthly": 3
}
```

The CLIENT "Backups" tab displays this as:
```
Last Backup:   ✓ 2026-05-29 03:06:41  (+1.2 GB)  snapshot: abc1234f
Next Backup:   2026-05-30 03:00:00  (in 14h 32m)
```

---

## 7. Failure Notifications

When a scheduled backup fails and `notify_on_failure: true`:

```rust
let alert = Alert {
    title: "Scheduled Backup Failed".to_string(),
    body: format!(
        "Restic backup failed at {}.\nError: {}\nNext attempt: {}",
        Utc::now(), error_detail, next_scheduled_run
    ),
    priority: AlertPriority::High,
};
send_alert(&alert, &config.alert_webhook_url).await;
```

The next scheduled run proceeds normally (failure does not disable the schedule).

---

## 8. Snapshot List

The CLIENT can fetch all available snapshots for display or restore selection:

```
GET https://<host_ip>:8443/api/backup/snapshots
Authorization: Bearer <host_token>
```

Response: Restic `snapshots --json` output, filtered and sorted by date descending. Used by `individual-backup-restore.md` and `full-backup-restore.md` to present a snapshot picker.

---

## 9. Logfmt Events

```
ts=<ISO8601> level=info component=host msg="scheduled backup triggered" cron="0 3 * * *"
ts=<ISO8601> level=info component=host msg="schedule updated" enabled=true cron="0 3 * * *"
ts=<ISO8601> level=error component=host msg="scheduled backup failed" error="<detail>"
ts=<ISO8601> level=info component=host msg="retention policy applied" kept=11 removed=2
```

---

## 10. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `manual-backup-all.md` | Identical internal flow; this use case just automates the trigger |
| `individual-backup-restore.md` | Uses snapshots created by scheduled backups |
| `full-backup-restore.md` | Uses snapshots created by scheduled backups |
| `error-handling-fail-closed.md` | Backup failure handling and DROP GUARD |
