# Use Case: Individual Backup Restore

**Tier:** CLIENT (selects snapshot + triggers) → HOST (executes Restic restore) → LXC (pauses containers)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

An individual stack restore replaces the appdata for a single stack with the contents of a selected Restic snapshot. The LXC running that stack is paused (Docker containers stopped) during restore, then restarted when Restic finishes.

This is used to:
- Roll back a stack's persistent data after a bad software update.
- Recover a corrupted database.
- Migrate data from a snapshot taken on a previous node.

---

## 2. Trigger

CLIENT: "Backups" tab → select a stack → "Restore From Snapshot…"

The CLIENT opens a snapshot picker modal:

```
╔══════════════════════════════════════════════════════╗
║  Restore Stack: paperless                             ║
╠══════════════════════════════════════════════════════╣
║  Select snapshot:                                     ║
║                                                       ║
║  ○ abc1234f  2026-05-29 03:06  manual    +1.2 GB     ║
║  ● bcd5678e  2026-05-28 03:06  scheduled  +0.3 GB    ║
║  ○ cde9012d  2026-05-27 03:06  scheduled  +0.2 GB    ║
║  ○ def3456c  2026-05-26 03:06  scheduled  +0.5 GB    ║
║                                                       ║
║  ⚠ This will overwrite all current appdata for       ║
║    'paperless'. This cannot be undone.               ║
║                                                       ║
║  [ Cancel ]                    [ Restore ]           ║
╚══════════════════════════════════════════════════════╝
```

Snapshot list is fetched from `GET /api/backup/snapshots?stack=paperless` (HOST filters by path prefix `/opt/appdata/paperless/`).

---

## 3. Confirmation Gate

Before proceeding, the CLIENT shows a red confirmation modal:

```
⚠  DESTRUCTIVE: Restore paperless

Restoring snapshot bcd5678e will permanently overwrite all
current data in /opt/appdata/paperless/.

Type the stack name to confirm:   [ paperless          ]

[ Cancel ]                        [ Restore Now ]
```

"Restore Now" is only enabled when the typed name exactly matches the stack name.

---

## 4. Full Restore Flow

```
Phase 1: CLIENT → HOST: POST /api/backup/restore
         Body: { snapshot_id: "bcd5678e", stack_name: "paperless" }

Phase 2: HOST → LXC (paperless): POST /api/backup/pause
         LXC daemon stops all labeled containers
         LXC daemon returns 200 with paused container list

Phase 3: HOST executes Restic restore
         restic -r <repo> restore bcd5678e \
           --target / \
           --include /opt/appdata/paperless/
         Streams JSON output to CLIENT via SSE

Phase 4: HOST fixes ownership (UID remapping)
         chown -R 100000:100000 /opt/appdata/paperless/
         (UID 100000 = root inside unprivileged LXC)

Phase 5: HOST → LXC (paperless): POST /api/backup/resume
         LXC daemon restarts all paused containers
         LXC daemon returns 200

Phase 6: HOST waits for LXC health (poll /health, 60s timeout)

Phase 7: HOST emits completion event to CLIENT SSE stream
         { snapshot_id, stack_name, duration_ms, files_restored }
```

**DROP GUARD:** The HOST creates a `RestoreGuard` before calling `restic restore`. The guard holds the list of paused containers and sends `POST /api/backup/resume` on drop (even on panic).

---

## 5. API Payload

```
POST https://<host_ip>:8443/api/backup/restore
Authorization: Bearer <host_token>
Content-Type: application/json

{
  "snapshot_id": "bcd5678e",
  "stack_name": "paperless",
  "target_path": "/opt/appdata/paperless"
}
```

Response: `202 Accepted` immediately; progress streamed via SSE.

---

## 6. Error States

| Failure Condition | Response |
|---|---|
| Snapshot ID not found in repository | `404 Not Found`; no restore attempted |
| LXC for stack is INACTIVE (not running) | Proceed without pause/resume (containers already stopped) |
| LXC for stack is PROVISIONED (never deployed) | Reject: `409 Conflict` — "stack has never been deployed; no LXC is running" |
| LXC pause fails (container stuck) | Abort restore; emit error; no data overwritten |
| `restic restore` exits non-zero | Drop guard fires (resume); return `500` to CLIENT |
| `chown` fails | Log warning; resume containers; return partial success |
| LXC daemon fails to come back online after resume (60s timeout) | Emit error event; CLIENT shows manual recovery steps |

---

## 7. Post-Restore State

After a successful restore:
- All Docker containers for the stack are running with the restored data.
- The next GitOps sync (`docker compose up -d`) will run against the restored data with no changes (Compose state matches Git state).
- If the restored snapshot predates a compose schema change, the migration will run on next deploy.

**Logfmt events:**
```
ts=<ISO8601> level=info component=host msg="restore started" snapshot=bcd5678e stack=paperless
ts=<ISO8601> level=info component=host msg="restore complete" snapshot=bcd5678e stack=paperless files=12445 duration_ms=184000
ts=<ISO8601> level=error component=host msg="restore failed" snapshot=bcd5678e stack=paperless error="<detail>"
```

---

## 8. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `manual-backup-all.md` | Creates the snapshots restored here |
| `backup-scheduling.md` | Creates the snapshots restored here; lists available snapshots |
| `full-backup-restore.md` | Calls this flow N times for full disaster recovery |
| `error-handling-fail-closed.md` | Abort rules when LXC pause fails |
