# Use Case: Activate Stack

**Tier:** CLIENT (state change) → HOST (LXC start) → LXC (resume sync)  
**Replaces:** Manual `pct start`, ad-hoc host commands  
**Status:** Specification — not yet implemented  

---

## 1. Overview

"Activating" a stack transitions it from `INACTIVE` or `PROVISIONED_STOPPED` state to `ACTIVE`. An active stack has a running LXC container, a running LXC daemon, and participates in the 30-minute fallback GitOps sync cycle. This use case covers the scenario where:

- A stack was previously **deactivated** (LXC stopped, data preserved).
- A stack was **provisioned** (LXC created, bootstrapped) but never started.
- A stack was manually stopped on the Proxmox host and needs to be re-enrolled in GitOps management.

This is a non-destructive operation. No data is created or removed. The Git directory and `/opt/appdata/<stack_name>` already exist.

---

## 2. Stack States

```
SCAFFOLDED    → Git dir exists; lxc-compose.yml written; LXC not yet provisioned
PROVISIONED   → LXC created and bootstrapped; LXC daemon installed; never started
ACTIVE        → LXC running; LXC daemon running; participating in GitOps sync
INACTIVE      → LXC stopped; data preserved; no sync occurring
ERROR         → LXC or daemon in a failed state; requires manual inspection
```

This use case transitions: `INACTIVE | PROVISIONED → ACTIVE`

---

## 3. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| Stack directory exists in `stacks/<stack_name>/` | CLIENT | Local Git working tree check |
| `lxc-compose.yml` is present and valid | CLIENT | `serde_yaml` parse check |
| VMID recorded in `lxc-compose.yml` exists on Proxmox host | HOST | `GET /api/lxc/<vmid>/status` |
| `/opt/appdata/<stack_name>` exists on host NVMe | HOST | `GET /api/lxc/<vmid>/appdata/status` |
| LXC is currently stopped (not already running) | HOST | Status check from above |

---

## 4. Step-by-Step Flow

### Phase 1 — CLIENT: Select Stack to Activate

**Trigger:** User selects an `INACTIVE` stack in the Stacks tab and presses `Enter` or `a`, or selects "Activate" from the context menu.

**Actions:**
1. CLIENT reads `stacks/<stack_name>/lxc-compose.yml` to retrieve `vmid`, `hostname`, and `hwaddr`.
2. CLIENT calls `GET /api/lxc/<vmid>/status` on HOST:
   - If `running` → show info modal: "Stack is already active." No-op.
   - If `stopped` → proceed to Phase 2.
   - If `404` (VMID not found) → show error: "LXC not found on host. Use Deploy to re-provision." Redirect to `deploy-active-stacks.md` flow.
3. CLIENT calls `GET /api/lxc/<vmid>/appdata/status` on HOST:
   - If appdata directory missing → show error modal: "AppData directory not found at `/opt/appdata/<stack_name>`. Activation aborted." No further steps.
4. CLIENT shows a confirmation modal (non-destructive; standard Cyan border):
   ```
   Activate Stack: <stack_name>
   LXC VMID:  <vmid>
   MAC:       <hwaddr>
   AppData:   /opt/appdata/<stack_name>  ✓ present
   
   This will start the LXC and resume GitOps sync.
   [ Cancel ]  [ Activate ]
   ```

---

### Phase 2 — CLIENT → HOST: Start LXC

CLIENT sends:
```
POST /api/lxc/start
Authorization: Bearer <host_token>
Content-Type: application/json

{ "vmid": <vmid>, "stack_name": "<stack_name>" }
```

**HOST daemon actions:**

| Step | Action | Fail Behaviour |
|---|---|---|
| Verify LXC exists | `pct status <vmid>` | Return `404` if missing |
| Verify LXC is stopped | Status check | Return `409 Conflict` if already running |
| Start LXC | `pct start <vmid>` | Emit `level=error` SSE; return `500` |
| Wait for network | Poll `pct exec <vmid> -- hostname` with 30s timeout | Return `504 Gateway Timeout` |

**SSE events from HOST:**
```
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="pct start invoked"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="LXC started; waiting for network"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="LXC network reachable"
```

---

### Phase 3 — CLIENT: Wait for LXC Daemon Health

After the HOST confirms the LXC is network-reachable, CLIENT polls the LXC daemon directly:

```
GET http://<lxc_ip>:8080/health
```

- Polls every 5 seconds, up to 60 seconds total.
- A `200 OK` response means the LXC daemon is running and healthy.
- If the daemon is not healthy after 60 seconds, CLIENT shows an error:  
  "LXC is running but the daemon is unreachable. Check `docker ps` inside the container."

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=client stack=<stack_name> msg="polling LXC daemon health"
ts=<ISO8601> level=info component=client stack=<stack_name> msg="LXC daemon healthy"
```

---

### Phase 4 — CLIENT → LXC: Trigger Immediate Sync

Once healthy, CLIENT triggers an immediate GitOps sync to ensure the stack is fully up-to-date:

```
POST http://<lxc_ip>:8080/api/sync
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{ "force": true, "stack": "<stack_name>" }
```

LXC daemon performs the full sync flow (see `add-stack.md` Phase 10):
- `setup.sh` hook.
- Git sparse checkout (pulls latest from `main`).
- Ephemeral secrets injection.
- `docker compose pull -q && docker compose up -d --remove-orphans` for each app.

**LXC SSE events forwarded to CLIENT:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="sync triggered on activation"
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="sync complete" apps=<N>
```

---

### Phase 5 — CLIENT: State Update and Completion

1. CLIENT updates the stack state to `ACTIVE` in its local state store.
2. Stacks tab refreshes; the stack entry shows a green `● ACTIVE` indicator.
3. A brief info toast is shown: "Stack <stack_name> is now active."

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=client stack=<stack_name> vmid=<vmid> msg="stack activated"
```

---

## 5. Fallback Sync Enrollment

Once the LXC daemon is active, it automatically enrolls in the 30-minute fallback cron loop. No additional action is required. The daemon's internal `tokio::time::interval(Duration::from_secs(1800))` loop begins immediately on startup.

---

## 6. Idempotency

- Activating an already-active stack returns `409 Conflict` from the HOST; CLIENT shows an info notice without modifying any state.
- Repeated activation calls are safe and result in at most one running LXC and one sync cycle.

---

## 7. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `deactivate-stack.md` | Inverse operation |
| `deploy-active-stacks.md` | Batch activation + provisioning for stacks in SCAFFOLDED state |
| `add-stack.md` | Full stack creation ending in the same ACTIVE state |
| `pre-sync-hooks.md` | `setup.sh` execution in Phase 4 |
| `tui-deployment-modal-progress.md` | Progress modal for Phase 2–4 |
