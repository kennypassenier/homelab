# Use Case: Deactivate Stack

**Tier:** CLIENT (state change) → LXC (graceful shutdown) → HOST (LXC stop)  
**Replaces:** Manual `pct stop`, ad-hoc host commands  
**Status:** Specification — not yet implemented  

---

## 1. Overview

"Deactivating" a stack is a **non-destructive** operation that:
- Stops all Docker containers inside the LXC cleanly.
- Stops the LXC container on the Proxmox host.
- Preserves all persistent data at `/opt/appdata/<stack_name>`.
- Preserves the Git directory `stacks/<stack_name>/`.
- Removes the stack from the active GitOps sync cycle.

Deactivation is the correct operation when a stack needs to be taken offline for maintenance, resource reclamation, or seasonal hibernation — without losing configuration or data.

Transitions the stack state: `ACTIVE → INACTIVE`

---

## 2. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| Stack is in `ACTIVE` state | CLIENT | Local state + `GET /api/lxc/<vmid>/status` confirms `running` |
| No active Restic backup is running for this stack | HOST | `GET /api/backup/status` returns `idle` for this stack |
| CLIENT is authenticated to HOST daemon | CLIENT | Bearer token present |

---

## 3. Step-by-Step Flow

### Phase 1 — CLIENT: Confirmation Modal

**Trigger:** User selects an `ACTIVE` stack in the Stacks tab and presses `x`, or selects "Deactivate" from the context menu.

**Actions:**
1. CLIENT shows a Ratatui confirmation modal (Yellow/Amber border — warning but not destructive):
   ```
   Deactivate Stack: <stack_name>
   LXC VMID:  <vmid>
   Running Apps: <app1>, <app2>, ...
   
   The LXC will be stopped. All data is preserved.
   The stack will not sync until reactivated.
   
   [ Cancel ]  [ Deactivate ]
   ```
2. Default focus is on `[ Cancel ]`. User must move to `[ Deactivate ]` and press Enter.
3. No exact-name confirmation is required (non-destructive).

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=client stack=<stack_name> msg="deactivation confirmed by user"
```

---

### Phase 2 — CLIENT → LXC: Graceful Application Shutdown

CLIENT sends a shutdown signal to the LXC daemon:

```
POST http://<lxc_ip>:8080/api/shutdown
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{
  "reason": "deactivation",
  "grace_period_seconds": 60
}
```

**LXC daemon actions:**
1. Stops the 30-minute fallback sync loop immediately (no new syncs are started).
2. Acquires `/tmp/gitops.lock` to ensure no in-flight sync is interrupted.
3. For each app in the stack, runs `docker compose down --timeout 60` in reverse dependency order (apps declared with `depends_on` are stopped first).
4. Releases lock.
5. Responds `200 OK` with:
   ```json
   { "status": "stopped", "apps_stopped": ["app1", "app2", "promtail", "watchtower"] }
   ```

**If LXC is unreachable (timeout 45s):** CLIENT logs `level=warn` and proceeds to Phase 3 with forced HOST stop. The modal shows an amber warning: "LXC daemon was unreachable; containers may not have stopped cleanly."

**LXC logfmt events:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="shutdown initiated; reason=deactivation"
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=<app> msg="docker compose down complete"
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="all apps stopped; ready for LXC stop"
```

---

### Phase 3 — CLIENT → HOST: Stop LXC

```
POST /api/lxc/stop
Authorization: Bearer <host_token>
Content-Type: application/json

{ "vmid": <vmid>, "stack_name": "<stack_name>", "timeout": 30 }
```

**HOST daemon actions:**

| Step | Action | Fail Behaviour |
|---|---|---|
| Verify LXC is running | `pct status <vmid>` | If already stopped, return `200` (idempotent) |
| Stop LXC | `pct stop <vmid> --timeout 30` | Emit `level=error` SSE event; return `500` |
| Confirm stopped | `pct status <vmid>` → `stopped` | If still running after timeout, force-kill and emit warning |

**SSE events from HOST:**
```
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="pct stop invoked"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="LXC stopped"
```

---

### Phase 4 — CLIENT: State Update and Completion

1. CLIENT updates local stack state to `INACTIVE`.
2. Stacks tab refreshes; stack entry shows a grey `○ INACTIVE` indicator.
3. Toast: "Stack <stack_name> deactivated. Data is preserved."

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=client stack=<stack_name> vmid=<vmid> msg="stack deactivated"
```

---

## 4. Maintenance Window Deactivation

For planned maintenance where the stack will be reactivated within a short window, the CLIENT shows an optional field: "Remind me to reactivate after: [  ] hours". If set, a background task in the CLIENT TUI emits an amber notification after the specified duration: "Stack <stack_name> has been inactive for X hours. Reactivate?"

---

## 5. Idempotency

- Deactivating an already-inactive stack is safe: HOST returns `200` (LXC already stopped), CLIENT updates state without error.
- Repeated deactivation calls never stop a container twice or leave the system in an inconsistent state.

---

## 6. Backup Interaction

If a Restic backup is in progress for this stack when deactivation is triggered, CLIENT shows a blocking warning:

> "A backup is currently running for this stack. Deactivation is blocked until the backup completes."

The deactivation modal shows a live progress line for the running backup. The `[ Deactivate ]` button remains disabled until `GET /api/backup/status` returns `idle` for this stack.

---

## 7. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `activate-stack.md` | Inverse operation — resumes the stack |
| `delete-stack.md` | Destructive alternative that also purges data |
| `manual-backup-all.md` | Run a backup before deactivating (optional flow) |
| `tui-deployment-modal-progress.md` | Progress modal used during Phases 2–3 |
