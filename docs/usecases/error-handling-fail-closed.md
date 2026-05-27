# Use Case: Error Handling — Fail-Closed

**Tier:** All tiers — CLIENT, HOST, LXC  
**Status:** Specification — not yet implemented  

---

## 1. Overview

"Fail-closed" means: **when in doubt, abort and do nothing**. The system never continues into an uncertain or partially-configured state. A container that starts without its secrets is worse than a container that doesn't start at all. An LXC that boots without a working bind mount is worse than one that doesn't boot.

This document defines the exact conditions that trigger a fail-closed abort at each tier, and the behaviour that follows.

---

## 2. Fail-Closed Conditions by Tier

### CLIENT Tier

| Condition | Fail-Closed Response |
|---|---|
| `lxc-compose.yml` fails `serde_yaml` parse | Abort scaffold; show inline error in wizard; no files written |
| `docker-compose.yml` fails pre-flight lint | Abort scaffold; show inline error; no Git commit |
| HOST daemon unreachable at provision time | Abort operation; show modal with "HOST unreachable at <host_ip>:8443" |
| Git push fails (network error or rejected) | Abort operation; local scaffold files remain; retry available |
| Transaction phase ledger detects in-progress state at startup | Force user to resolve (retry or rollback) before allowing new operations |
| LXC IP not resolvable from HOST | Abort sync trigger; show error: "LXC IP not yet assigned — register MAC in OPNsense and retry" |

### HOST Tier

| Condition | Fail-Closed Response |
|---|---|
| `lxc-compose.yml` parse failure on `POST /api/lxc/provision` | Return `400 Bad Request`; no LXC created; no NVMe directory created |
| VMID already exists on Proxmox | Return `409 Conflict`; no action taken |
| `pct create` exits non-zero | Delete NVMe appdata dir; return `500`; emit SSE error event |
| Bootstrap exec (`apt`, Docker install) exits non-zero | `pct destroy <vmid> --purge`; delete NVMe appdata dir; return `500` |
| LXC fails to become network-reachable within 30s | Same as above: `pct destroy --purge` |
| Mount source path does not exist and cannot be created | Abort mount configuration; return `500`; LXC is stopped |
| Atomic config write for GPU passthrough: `rename()` fails | Leave original config untouched; return `500`; emit SSE error |
| Restic backup exits non-zero | Drop guard fires: `POST /api/backup/resume` to all paused LXCs; return error to CLIENT |
| Device major number not in allowlist during GPU passthrough | Return `422 Unprocessable Entity`; no config file modified |

### LXC Tier

| Condition | Fail-Closed Response |
|---|---|
| Startup: `/appdata` not a real bind mount | Daemon exits with code 1; Docker restarts it; emits `level=error` |
| Startup: Docker socket not accessible | Daemon exits with code 1 |
| Startup: `/opt/homelab` not a valid Git repo | Daemon exits with code 1 |
| Sync: `git pull` fails (diverged, auth error, network) | Abort sync; release lock; emit `level=error`; 30-min retry |
| Sync: Ephemeral secrets container exits non-zero | Abort sync; do NOT start any containers; emit `level=error` |
| Sync: `setup.sh` exits non-zero | Abort sync; do NOT run `docker compose up`; emit `level=error` |
| Sync: `docker compose pull` fails for one app | Skip that app for this cycle; continue with other apps; emit `level=warn` |
| Sync: `docker compose up` fails for one app | Log error; continue with remaining apps; trigger post-deploy rollback for failed app |
| Post-deploy: Container crashes within 10s observation window | Rollback to previous image; emit `level=warn`; send webhook alert if unattended |
| `POST /api/sync` received while lock is held | Return `423 Locked`; do not start a second sync |
| `POST /api/backup/pause` fails to stop a container | Return `500`; Restic backup is NOT started by HOST (DROP GUARD pattern) |

---

## 3. Ephemeral Secrets — Strictest Fail-Closed Rule

The ephemeral secrets container is the most critical fail-closed point in the entire system.

**Rule:** If the secrets container fails to produce a `.env` file for any reason, **the sync cycle is aborted immediately**. No containers are started, updated, or restarted.

Rationale: Starting a container without secrets means it boots in a degraded or insecure state (empty passwords, missing API keys, publicly exposed endpoints). This is strictly worse than the container not running at all.

```rust
// LXC daemon sync loop
let secrets_result = run_ephemeral_secrets_container(&stack_name).await;
match secrets_result {
    Ok(env_path) => {
        // Proceed to setup.sh and docker compose up
    }
    Err(e) => {
        emit_log(Level::Error, format!("secrets container failed: {}", e));
        // Abort sync. Do NOT call docker compose up under any circumstances.
        return Err(SyncError::SecretsFailed(e));
    }
}
```

---

## 4. Fail-Closed vs. Fail-Open Decision Table

The system uses "fail-open" only in the narrow case where **failing closed would cause more harm than the degraded state**:

| Scenario | Strategy | Rationale |
|---|---|---|
| `docker compose pull` network error for one app | Fail-open (skip app, continue others) | Other apps should not lose updates because one image pull failed |
| Container health check stuck in `starting` state | Fail-open (wait grace period, then warn) | App may be legitimately slow to start |
| Webhook alert send fails | Fail-open (log and continue) | Alert failure must not prevent the deployment from completing |
| LXC daemon gets `SIGTERM` during sync | Fail-closed (release lock, abort sync; do not half-deploy) | Partial deploy is worse than no deploy |
| HOST daemon cannot reach Ntfy for alert | Fail-open | Core operation (backup) must not be blocked by alerting infrastructure |

---

## 5. Observability of Failures

All fail-closed events emit at `level=error` in the structured logfmt stream. The CLIENT TUI displays them with a Red background in the log pane. The Stacks tab shows:
- Red `✗ ERROR` badge on any stack that is in a failed state.
- An info icon that, when selected, shows the last 20 error log lines for that stack.

**Logfmt pattern for all fail-closed events:**
```
ts=<ISO8601> level=error component=<tier> stack=<stack_name> phase=<phase> msg="<human-readable description>" error="<technical detail>"
```

---

## 6. Retry vs. Abort Decision

After a fail-closed abort:

| Tier | Automatic Retry | Manual Retry |
|---|---|---|
| CLIENT | Never auto-retries destructive operations | Retry button in modal |
| HOST provision | Never auto-retries; LXC is destroyed | User must trigger via CLIENT |
| LXC sync (30-min cron) | Retries on next 30-min interval | `POST /api/sync` from CLIENT |
| LXC sync (CLIENT-triggered) | Up to 3 automatic retries with 5s backoff | Retry button in modal after all 3 fail |
| Post-deploy rollback | Runs automatically on crash detection | Not manually triggerable |

---

## 7. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `transactional-actions.md` | Compensation steps executed after fail-closed abort |
| `pre-sync-hooks.md` | `setup.sh` failure triggers fail-closed |
| `post-deploy-actions.md` | Crash detection triggers fail-closed rollback |
| `error-warning-logging.md` | All fail-closed events are emitted as `level=error` logfmt |
| `tui-deployment-modal-progress.md` | Red state rendering in CLIENT modal |
