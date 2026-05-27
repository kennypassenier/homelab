# Use Case: Delete Core App from Stack

**Tier:** CLIENT (safety checks + Blast Radius) → LXC (graceful stop + GC) → HOST (no action required)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Core apps (`promtail`, `watchtower`, `traefik`) receive heightened protection compared to regular apps. Removing them has system-wide implications:
- Removing **Promtail** means logs from this stack are no longer shipped to Loki. Grafana dashboards for this stack go dark.
- Removing **Watchtower** means containers in this stack stop receiving automatic image updates.
- Removing **Traefik** means all web services in this stack lose their reverse proxy and TLS termination. **This is a site-affecting action if Traefik serves multiple stacks.**

Each core app removal triggers its own tailored impact assessment modal.

---

## 2. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| The core app directory exists in `stacks/<stack_name>/` | CLIENT | Directory check |
| For Traefik: no other apps on this HOST are routing through this instance | CLIENT | Scans all stacks for Traefik labels pointing to this instance |

---

## 3. Step-by-Step Flow

### Phase 1 — CLIENT: Impact Assessment Modal

**Trigger:** User selects `promtail`, `watchtower`, or `traefik` in the app list and presses `d`.

CLIENT detects the core app type and renders a tailored impact modal (Red border, 3D drop-shadow):

**Promtail impact modal:**
```
⚠  REMOVE CORE APP — promtail
Stack: <stack_name>

Impact:
  • All Docker logs from stack <stack_name> will stop flowing to Loki.
  • Grafana dashboards for this stack will show NO DATA.
  • CrowdSec access log analysis (if used) will be interrupted.

[ Cancel ]  [ Remove Promtail ]
```

**Watchtower impact modal:**
```
⚠  REMOVE CORE APP — watchtower
Stack: <stack_name>

Impact:
  • Containers in this stack will NO LONGER receive automatic image updates.
  • Images will only update when you manually trigger a sync.

[ Cancel ]  [ Remove Watchtower ]
```

**Traefik impact modal (most severe):**
```
⚠  REMOVE CORE APP — traefik
Stack: <stack_name>

Impact:
  !! ALL services routing through this Traefik instance will become
     UNREACHABLE over HTTPS. This affects:
     
     Stack: <stack_name>    Apps: <app1>, <app2>
     Stack: <other_stack>   Apps: <app3>   ← CROSS-STACK IMPACT

  Ensure you have an alternative reverse proxy before proceeding.
  
  This action requires DOUBLE CONFIRMATION.

[ Cancel ]  [ I understand — Continue ]
```

---

### Phase 2 — CLIENT: Exact-Name Confirmation (Second Gate, always)

User must type the exact core app name (e.g., `promtail`, `watchtower`, `traefik`) to enable the "Delete" button. This gate is always present for core apps regardless of the tier of impact.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=warn component=blast-radius stack=<stack_name> app=<core_app> msg="core app delete confirmed by user"
```

---

### Phase 3 — Traefik Only: Verify No Active Routing Depends on It

For Traefik, CLIENT performs a pre-delete routing audit:
1. Scans all `stacks/*/*/docker-compose.yml` for `traefik.http.routers.*` labels.
2. If any routers point to the Traefik instance being deleted, CLIENT shows a blocking error:
   ```
   Cannot remove Traefik: the following apps are still routing through it.
   Remove Traefik labels from these apps first:
     • media/jellyfin — traefik.http.routers.jellyfin
     • media/sonarr   — traefik.http.routers.sonarr
   ```
3. Deletion is blocked until all dependent Traefik labels are removed.

---

### Phase 4 — Conditional: LXC Graceful Stop (if stack is ACTIVE)

CLIENT sends a targeted stop signal to the LXC daemon:

```
POST http://<lxc_ip>:8080/api/app/stop
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{ "app_name": "<core_app>", "grace_period_seconds": 30 }
```

LXC daemon runs `docker compose down --timeout 30` for the core app's compose project.

**LXC logfmt event:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=<core_app> msg="core app stopped gracefully"
```

---

### Phase 5 — CLIENT: Git Removal and Push

1. `std::fs::remove_dir_all("stacks/<stack_name>/<core_app>/")`.
2. `std::fs::remove_dir_all("stacks/<stack_name>/<core_app>-config/")`.
3. Stage and commit: `chore(scaffold): remove core app <core_app> from stack <stack_name>`.
4. Push to `main`.

---

### Phase 6 — Conditional: LXC GC Sync (if stack is ACTIVE)

```
POST http://<lxc_ip>:8080/api/sync
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{ "force": true, "stack": "<stack_name>", "gc": true }
```

LXC daemon's GC step detects the absent core app directory and removes any lingering containers/images.

---

### Phase 7 — CLIENT: Completion and Warnings

- Toast shows removal confirmation.
- If Promtail was removed: persistent amber banner in the CLIENT TUI on the stack detail view: "⚠ Logging disabled — this stack has no Promtail instance."
- If Watchtower was removed: amber banner: "⚠ Auto-updates disabled — this stack has no Watchtower instance."

---

## 4. Idempotency

Same guarantees as `delete-app-from-stack.md`: each phase checks existence before acting.

---

## 5. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-core-app-to-stack.md` | Inverse: restore a core app |
| `delete-app-from-stack.md` | Same flow for non-core apps |
| `error-warning-logging.md` | Impact of Promtail removal on Loki observability |
