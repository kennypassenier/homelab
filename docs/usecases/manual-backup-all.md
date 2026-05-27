# Use Case: Manual Backup — All Stacks

**Tier:** CLIENT (triggers) → HOST (orchestrates Restic) → LXC (pauses containers during backup)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

A manual "backup all" operation triggers a full Restic backup of `/opt/appdata/` on the Proxmox host. Before the backup runs, the HOST daemon asks each active LXC to pause containers that are labeled `com.homelab.backup.pause=true`. This ensures data consistency for databases and document stores that write continuously.

After Restic completes (or fails), the HOST daemon resumes all paused containers. A Rust `Drop` guard guarantees the resume always happens, even if the HOST daemon panics mid-backup.

---

## 2. What Is Backed Up

| Path | Backed up | Notes |
|---|---|---|
| `/opt/appdata/` | Yes | All persistent stack data |
| `/mnt/data/18TB` | No | Media — too large; Plex/Jellyfin can re-index |
| `/mnt/data/12TB` | No | Media overflow |
| `/mnt/downloads` | No | Transient download queue |

---

## 3. Containers That Require Pausing

Containers are paused if they have the label:
```yaml
labels:
  com.homelab.backup.pause: "true"
```

Standard candidates:
- PostgreSQL databases (`db` containers in paperless, vikunja stacks).
- Loki storage backend.
- Any key-value store (Redis, etc.).

Containers **without** this label continue running during backup (stateless apps, proxies, monitoring agents).

---

## 4. Trigger

CLIENT: "Backups" tab → press `b` or click "Backup All Now"

API call to HOST:
```
POST https://<host_ip>:8443/api/backup/run
Authorization: Bearer <host_token>
Content-Type: application/json

{
  "scope": "all",
  "dry_run": false
}
```

---

## 5. Full Backup Flow

```
Phase 1: HOST builds pause list
   → GET /api/containers (from each active LXC daemon)
   → Filter containers where label com.homelab.backup.pause=true
   → Build: [(lxc_ip, container_name), ...]

Phase 2: HOST pauses each LXC (sequential, not parallel)
   → POST http://<lxc_ip>:8080/api/backup/pause to each LXC daemon
   → LXC daemon: docker stop <container_name> (graceful, 30s timeout)
   → LXC daemon returns 200 when all labeled containers are stopped
   → On any 500 or timeout: HOST aborts backup (Drop Guard fires → resume all)

Phase 3: HOST acquires Rust Drop Guard
   → struct BackupGuard { paused_lxcs: Vec<LxcEndpoint> }
   → impl Drop { sends POST /api/backup/resume to all paused LXCs }
   → BackupGuard is created; Drop fires on scope exit OR panic

Phase 4: Restic backup
   → restic -r <restic_repo> backup /opt/appdata/ --tag manual --tag homelab
   → Streams Restic JSON output to CLIENT via SSE
   → On exit code 0: proceed to Phase 5
   → On non-zero exit: BackupGuard fires (resume all), return error to CLIENT

Phase 5: HOST resumes all paused LXCs
   → POST http://<lxc_ip>:8080/api/backup/resume to each paused LXC
   → LXC daemon: docker start <container_name> (for each paused container, in reverse order)
   → BackupGuard.forget() — disables Drop so it doesn't fire twice

Phase 6: HOST emits completion event
   → SSE event: backup_complete { snapshot_id, duration_ms, files_processed, data_added }
   → Logfmt: ts=... level=info component=host msg="backup complete" snapshot=<id>
```

---

## 6. LXC Daemon: Pause and Resume API

### `POST /api/backup/pause`

Stops all containers labeled `com.homelab.backup.pause=true`. Returns `200` only when all are stopped.

```json
// Response
{
  "paused": ["paperless-db", "paperless-broker"],
  "already_stopped": [],
  "failed": []
}
```

If `failed` is non-empty, returns `500`. The HOST aborts the backup.

### `POST /api/backup/resume`

Restarts containers that were paused by the most recent `/api/backup/pause` call. Containers are started in the **reverse order** they were stopped.

```json
// Response
{
  "resumed": ["paperless-broker", "paperless-db"],
  "failed": []
}
```

---

## 7. Restic Repository Configuration

The Restic repository is configured in the HOST daemon at startup. Supported backends:

| Backend | Env Var | Example |
|---|---|---|
| Local path | `RESTIC_REPOSITORY` | `/mnt/nas/restic-repo` |
| REST server | `RESTIC_REPOSITORY` | `rest:http://192.168.1.20:8000/` |
| S3-compatible | `RESTIC_REPOSITORY` | `s3:http://minio.local:9000/backups` |
| Backblaze B2 | `RESTIC_REPOSITORY` | `b2:homelab-backups:/restic` |

`RESTIC_PASSWORD` is injected from the HOST daemon's secrets (never in Git).

---

## 8. CLIENT Progress Modal During Backup

```
╔══════════════════════════════════════════════════╗
║          Backup All Stacks                        ║
╠══════════════════════════════════════════════════╣
║ Phase: Pausing containers...                      ║
║                                                   ║
║ Paused:                                           ║
║   ✓ paperless-db (paperless LXC)                 ║
║   ✓ paperless-broker (paperless LXC)             ║
║   ✓ loki (monitoring LXC)                        ║
║                                                   ║
║ Restic progress:                                  ║
║   Files: 12,445 / 98,102                         ║
║   Data:  1.4 GB / 8.7 GB                         ║
║   [████████░░░░░░░░░░░░] 14%                     ║
║                                                   ║
║ Elapsed: 00:04:23                                 ║
╚══════════════════════════════════════════════════╝
```

On completion:
```
╔══════════════════════════════════════════════════╗
║  ✓  Backup Complete                               ║
║                                                   ║
║  Snapshot: abc1234f                               ║
║  Size:     8.7 GB (deduped: 1.2 GB new)          ║
║  Duration: 00:06:41                               ║
║  All containers resumed.                          ║
╚══════════════════════════════════════════════════╝
```

---

## 9. Logfmt Events

```
ts=<ISO8601> level=info component=host msg="backup started" scope=all
ts=<ISO8601> level=info component=host msg="pausing containers" lxc=<ip> containers="paperless-db,paperless-broker"
ts=<ISO8601> level=info component=host msg="all containers paused" count=3 duration_ms=4200
ts=<ISO8601> level=info component=host msg="restic backup running" repo=<type>
ts=<ISO8601> level=info component=host msg="backup complete" snapshot=abc1234f files=98102 data_added_bytes=1342177280 duration_ms=401000
ts=<ISO8601> level=info component=host msg="resuming containers" count=3
ts=<ISO8601> level=info component=host msg="all containers resumed"
```

---

## 10. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `backup-scheduling.md` | Automated version of this flow; same API call |
| `individual-backup-restore.md` | Restore a single stack from a snapshot created here |
| `full-backup-restore.md` | Full disaster recovery using snapshots created here |
| `error-handling-fail-closed.md` | DROP GUARD ensures resume always fires |
| `tui-deployment-modal-progress.md` | Restic SSE stream rendered in progress modal |
