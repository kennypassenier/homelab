# Use Case: Delete App from Stack

**Tier:** CLIENT (Blast Radius modal) → LXC (GC: stop + remove container) → HOST (optional: mount removal warning)  
**Replaces:** `remove-app.sh`  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Deleting an app from a stack permanently removes:
1. `stacks/<stack_name>/<app_name>/` from Git.
2. The running Docker container and its network entries inside the LXC (garbage collection).
3. Optionally (with explicit confirmation): the persistent data at `/opt/appdata/<stack_name>/<app_name>-config` on the host NVMe.

The Git directory `<app_name>-config/` is **always removed** from Git (it only contains `.gitkeep`). The actual bind-mounted data on the host NVMe is only deleted if the user explicitly opts in with a second confirmation.

---

## 2. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| App directory exists in `stacks/<stack_name>/<app_name>/` | CLIENT | Directory check |
| App is not a protected core app (Promtail, Watchtower, Traefik) | CLIENT | Name check against core app list; use `delete-core-app-from-stack.md` for those |
| Stack state is known | CLIENT | `GET /api/lxc/<vmid>/status` |

---

## 3. Step-by-Step Flow

### Phase 1 — CLIENT: Blast Radius Modal (First Gate)

**Trigger:** User selects an app in the App list within a stack, presses `d`, or selects "Delete App" from the context menu.

**Actions:**
1. CLIENT renders a Red-bordered floating modal:
   ```
   ⚠  DELETE APP — <app_name>
   Stack:      <stack_name>
   Image:      <image>
   Config dir: /opt/appdata/<stack_name>/<app_name>-config

   ┌─ Data Retention ──────────────────────────────────────┐
   │  ( ) Keep persistent data on host NVMe  (recommended) │
   │  ( ) DELETE persistent data from host NVMe            │
   └───────────────────────────────────────────────────────┘

   [ Cancel ]  [ Continue ]
   ```
2. "Keep data" is the default selection. User must explicitly switch to "DELETE data" to trigger NVMe purge.

---

### Phase 2 — CLIENT: Exact-Name Confirmation (Second Gate)

Same as `delete-stack.md` Phase 2: user must type the exact app name to enable the "Delete" button.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=warn component=blast-radius stack=<stack_name> app=<app_name> purge_data=<bool> msg="app delete confirmed by user"
```

---

### Phase 3 — CLIENT: Git Removal

1. `std::fs::remove_dir_all("stacks/<stack_name>/<app_name>/")`.
2. `std::fs::remove_dir_all("stacks/<stack_name>/<app_name>-config/")`.
3. Stage: `git rm -r --cached stacks/<stack_name>/<app_name>/ stacks/<stack_name>/<app_name>-config/`.
4. Commit: `chore(scaffold): delete app <app_name> from stack <stack_name>`.
5. Push to `main`.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=git stack=<stack_name> app=<app_name> sha=<sha> msg="app directory removed and pushed"
```

---

### Phase 4 — CLIENT → LXC: Garbage Collection Trigger (if stack is ACTIVE)

CLIENT triggers a sync with GC enabled so the LXC daemon removes the orphaned container:

```
POST http://<lxc_ip>:8080/api/sync
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{
  "force": true,
  "stack": "<stack_name>",
  "gc": true
}
```

**LXC daemon GC actions:**
1. Git pull detects that `stacks/<stack_name>/<app_name>/` no longer exists.
2. LXC daemon runs `docker compose down` in the now-absent app directory's last known state (using the compose project name derived from `<stack_name>_<app_name>`).
3. Runs `docker image prune -f --filter label=com.centurylinklabs.watchtower.enable=true` to clean up pulled images if no longer needed.
4. Emits GC log event.

**LXC logfmt events:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=<app_name> gc=true msg="orphaned container stopped and removed"
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=<app_name> gc=true msg="image pruned"
```

---

### Phase 5 — Conditional: HOST NVMe Data Purge

If the user selected "DELETE persistent data" in Phase 1:

```
POST /api/appdata/purge
Authorization: Bearer <host_token>
Content-Type: application/json

{
  "stack_name": "<stack_name>",
  "app_name": "<app_name>",
  "path": "/opt/appdata/<stack_name>/<app_name>-config"
}
```

**HOST actions:**
1. Verify path is within `/opt/appdata/` (path traversal guard).
2. `std::fs::remove_dir_all("/opt/appdata/<stack_name>/<app_name>-config")`.
3. Emit SSE confirmation.

If data retention was chosen (default), the directory remains on the NVMe. The CLIENT shows a note: "Config data retained at `/opt/appdata/<stack_name>/<app_name>-config` on the Proxmox host."

**HOST SSE event:**
```
data: ts=<ISO8601> level=info component=host stack=<stack_name> app=<app_name> msg="appdata purged from NVMe"
```

---

### Phase 6 — CLIENT: Completion

1. App entry removed from the stack's app list in the CLIENT TUI.
2. Toast: "App <app_name> deleted from stack <stack_name>."

---

## 4. Protection Against Core App Deletion

If the user attempts to delete `promtail`, `watchtower`, or `traefik`, CLIENT shows an info modal:

> "This is a core app. Use 'Remove Core App' to remove it safely with additional checks."

The delete flow is aborted; the user is redirected to `delete-core-app-from-stack.md`.

---

## 5. Idempotency

- Deleting an app that has already been removed from Git (directory missing) skips the Git removal step.
- GC sync on LXC is safe to run multiple times; `docker compose down` on a non-existent project is a no-op.
- HOST purge endpoint checks path existence before `remove_dir_all`.

---

## 6. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `delete-core-app-from-stack.md` | For protected core apps (Promtail, Watchtower, Traefik) |
| `delete-stack.md` | Delete the entire stack (all apps + LXC) |
| `update-active-stacks.md` | GC of orphaned apps on standard sync |
| `error-handling-fail-closed.md` | LXC GC failure handling |
