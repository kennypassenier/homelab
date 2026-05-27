# Use Case: Delete Stack

**Tier:** CLIENT (Blast Radius modal) → HOST (LXC destroy + NVMe purge) → LXC (graceful shutdown)  
**Replaces:** `remove-stack.sh`, `reset-stack.sh`  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Deleting a stack permanently removes:
1. The Git directory `stacks/<stack_name>/` (committed and pushed).
2. The running LXC container on the Proxmox host.
3. All persistent data at `/opt/appdata/<stack_name>` on the host NVMe.
4. The SSH alias in `~/.ssh/config`.

This action is **irreversible**. The CLIENT enforces a two-stage confirmation with an exact-name typed confirmation before any destructive step is taken. No host or LXC-side changes are made until the user passes both confirmation gates.

---

## 2. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| Stack exists in `stacks/` in the local Git working tree | CLIENT | Directory check |
| CLIENT is authenticated to HOST daemon | CLIENT | Bearer token in `~/.config/homelab/client.toml` |
| No active Restic backup is in progress for this stack | HOST | `GET /api/backup/status` returns `idle` for this stack |

---

## 3. Step-by-Step Flow

### Phase 1 — CLIENT: Blast Radius Modal (First Gate)

**Trigger:** User selects a stack in the Stacks tab and presses `d`, or selects "Delete Stack" from the context menu.

**Actions:**
1. CLIENT renders a full-screen floating modal with:
   - **Red double border** with a 3D drop-shadow layer (two overlapping `Block` widgets offset by 1 row/col).
   - **Header:** `⚠  DESTRUCTIVE ACTION — DELETE STACK` in bold Red.
   - **Body text** listing exactly what will be destroyed:
     ```
     Stack:     <stack_name>
     LXC VMID:  <vmid>
     Apps:      <app1>, <app2>, <app3>, ...
     Data:      /opt/appdata/<stack_name>  (PERMANENT — no recovery)
     ```
   - **Warning:** "This will destroy the LXC container and all persistent data on the host NVMe. This cannot be undone."
2. Two buttons: `[ Cancel ]` (default focus, green) and `[ I understand, continue ]` (red).
3. User must explicitly move focus to the red button and press Enter to proceed. Pressing `Escape` or `q` cancels.

---

### Phase 2 — CLIENT: Exact-Name Confirmation (Second Gate)

If the user passes Phase 1:

1. Modal transitions to a text input prompt:
   ```
   Type the stack name to confirm deletion:
   > [                    ]
   ```
2. The "Delete" button remains disabled until the input field contains the **exact** stack name (case-sensitive, byte-for-byte match).
3. Any mismatch shows an inline Red label: `✗ Name does not match`.
4. On exact match the button becomes active (Red, bold).

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=warn component=blast-radius stack=<stack_name> msg="delete confirmed by user"
```

---

### Phase 3 — CLIENT: Pre-Delete Backup Prompt

Before executing deletion, CLIENT presents an optional prompt:

> "Would you like to take a final backup of `/opt/appdata/<stack_name>` before deletion?"

- **Yes** → triggers `manual-backup-all.md` flow scoped to this single stack; deletion proceeds after backup completes.
- **No** → deletion proceeds immediately.
- **Cancel** → entire delete flow is aborted.

---

### Phase 4 — CLIENT → LXC: Graceful Shutdown Signal

CLIENT sends a graceful-stop signal to the LXC daemon **before** destroying the container, allowing Docker to stop containers cleanly:

```
POST http://<lxc_ip>:8080/api/shutdown
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{
  "reason": "stack_deletion",
  "grace_period_seconds": 30
}
```

**LXC daemon actions:**
1. For each running app, runs `docker compose down --timeout 30`.
2. Emits a final logfmt SSE flush.
3. Responds `200 OK` with `{ "status": "stopped" }` once all containers are stopped.
4. If CLIENT receives no response within 45 seconds (LXC unreachable), it logs a warning and proceeds with forced HOST-side destroy.

**Logfmt from LXC:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="graceful shutdown initiated"
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=<app> msg="docker compose down complete"
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="all containers stopped; ready for destroy"
```

---

### Phase 5 — CLIENT → HOST: Destroy LXC and Purge NVMe

CLIENT opens the deployment progress modal (Ratatui, same as `add-stack.md` Phase 8) and sends:

```
POST /api/lxc/destroy
Authorization: Bearer <host_token>
Content-Type: application/json

{
  "vmid": <vmid>,
  "stack_name": "<stack_name>",
  "purge_appdata": true
}
```

**HOST daemon actions (sequential, fail-closed):**

| Step | Action | Fail Behaviour |
|---|---|---|
| Verify VMID exists | `pct status <vmid>` | If missing, log warning and skip LXC steps |
| Stop LXC (if still running) | `pct stop <vmid> --timeout 30` | Force-kill after timeout |
| Destroy LXC | `pct destroy <vmid> --purge` | Emit `level=error` SSE event; abort NVMe purge |
| Purge NVMe appdata | `std::fs::remove_dir_all("/opt/appdata/<stack_name>")` | Emit `level=error` SSE event; report partial failure |

**SSE events from HOST:**
```
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="pct stop invoked"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="LXC stopped"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="pct destroy complete"
data: ts=<ISO8601> level=info component=host stack=<stack_name> msg="appdata purged from NVMe"
```

---

### Phase 6 — CLIENT: Git Removal and Push

After receiving HOST confirmation:

1. CLIENT deletes `stacks/<stack_name>/` from the local Git working tree (`std::fs::remove_dir_all`).
2. Stages the removal: `git rm -r --cached stacks/<stack_name>/`.
3. Commits: `chore(scaffold): delete stack <stack_name>`.
4. Pushes to `main`.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=git stack=<stack_name> sha=<sha> msg="stack directory removed and pushed"
```

---

### Phase 7 — CLIENT: SSH Config Cleanup

CLIENT idempotently removes the SSH alias block for `<stack_name>` from `~/.ssh/config`:
- Parses the full config AST.
- Locates the `Host <stack_name>` block.
- Removes it in-place; all other blocks are preserved byte-for-byte.
- Writes the updated config atomically (write to `.ssh/config.tmp`, `rename` over original).

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=ssh stack=<stack_name> msg="SSH alias removed from ~/.ssh/config"
```

---

### Phase 8 — CLIENT: Completion

1. Modal transitions to a "Deleted" state:
   - **Grey border** (stack no longer active).
   - Summary: `Stack <stack_name> (VMID <vmid>) has been permanently deleted.`
2. Stacks tab refreshes; the stack entry is removed.
3. Full logfmt session written to `~/.local/share/homelab/logs/<stack_name>-delete-<timestamp>.log`.

---

## 4. Rollback & Partial Failure Handling

| Phase | Failure | Behaviour |
|---|---|---|
| Phase 4 (LXC unreachable) | Timeout | Warning logged; HOST-side destroy proceeds; user notified that shutdown was forced |
| Phase 5 (pct destroy fails) | LXC destroy error | NVMe purge is skipped; user shown raw error; Git removal is NOT performed |
| Phase 5 (NVMe purge fails) | Filesystem error | LXC is already gone; error surfaced in modal; user advised to manually remove `/opt/appdata/<stack_name>` |
| Phase 6 (Git push fails) | Network error | Local tree already modified; retry push available as a button in the modal |
| Phase 7 (SSH parse error) | Config malformed | Original `~/.ssh/config` is restored from in-memory backup; error shown; user advised to edit manually |

**There is no automatic rollback after Phase 5.** Once `pct destroy` succeeds, the operation is considered committed.

---

## 5. Idempotency

- Running delete on an already-deleted stack (no directory in Git, no VMID on host) is safe: each phase checks existence before acting.
- `POST /api/lxc/destroy` with a non-existent VMID returns `404`; CLIENT logs a warning and skips to Git removal.
- Git removal of a non-existent path is a no-op (already-deleted).

---

## 6. Security Constraints

- The two-gate confirmation (modal + exact-name input) is mandatory and cannot be bypassed via CLI flags or API shortcuts.
- `purge_appdata: true` must be explicitly set in the HOST API call body; the HOST never auto-purges data without this flag.
- The HOST daemon validates that `vmid` matches the `stack_name` entry in its local registry before destroying, preventing accidental wrong-VMID destruction.

---

## 7. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `deactivate-stack.md` | Non-destructive alternative: stop syncing without deleting data |
| `individual-backup-restore.md` | Take a final backup in Phase 3 before deletion |
| `manual-backup-all.md` | Full backup flow optionally triggered in Phase 3 |
| `transactional-actions.md` | Partial failure handling rules |
| `error-handling-fail-closed.md` | Fail-closed rules for HOST destroy steps |
| `tui-deployment-modal-progress.md` | Progress modal used during Phases 4–5 |
