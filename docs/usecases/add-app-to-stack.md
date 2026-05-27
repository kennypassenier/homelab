# Use Case: Add App to Stack

**Tier:** CLIENT (wizard) → HOST (optional: add mount) → LXC (sync and deploy)  
**Replaces:** `create-new-app.sh`  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Adding an app to an existing, provisioned stack means:
1. Running a mini-wizard in the CLIENT to collect app configuration.
2. Scaffolding a new `stacks/<stack_name>/<app_name>/docker-compose.yml` and `<app_name>-config/` directory.
3. Optionally updating `lxc-compose.yml` with a new bind mount if the app needs persistent config storage.
4. Committing and pushing to Git.
5. If the stack is `ACTIVE`: triggering an immediate sync on the LXC daemon.
6. If the stack has a new mount: instructing HOST to add the bind mount and restart the LXC.

---

## 2. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| Target stack exists in `stacks/<stack_name>/` | CLIENT | Directory check |
| App name does not already exist in `stacks/<stack_name>/` | CLIENT | Directory check |
| Stack state is known | CLIENT | `GET /api/lxc/<vmid>/status` from HOST |

---

## 3. Step-by-Step Flow

### Phase 1 — CLIENT: Open "Add App" Mini-Wizard

**Trigger:** User selects a stack in the Stacks tab, presses `a`, or selects "Add App" from the context menu.

**Actions:**
1. CLIENT opens a focused mini-wizard modal (not full-screen; centred popup, Cyan border, rounded).
2. Modal header: `Add App to Stack: <stack_name>`.
3. Wizard collects the same app fields as `add-stack.md` Phase 4 (all app fields: name, image, ports, Traefik, healthcheck, VPN, capabilities, restart policy, env vars, mounts).
4. After field entry, a review screen shows the full generated `docker-compose.yml` preview.

---

### Phase 2 — CLIENT: Generate Artifacts

**Actions (same as `add-stack.md` Phase 6, scoped to the single new app):**

1. Create `stacks/<stack_name>/<app_name>/docker-compose.yml`.
2. Create `stacks/<stack_name>/<app_name>-config/.gitkeep`.
3. Pre-flight lint the new compose file via `serde_yaml`.
4. If the app requires a new bind mount: update `stacks/<stack_name>/lxc-compose.yml` to add the new `mp` entry with the next available mount index.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=scaffold stack=<stack_name> app=<app_name> msg="app scaffold written"
ts=<ISO8601> level=info component=scaffold stack=<stack_name> msg="lxc-compose.yml updated with new mount"
ts=<ISO8601> level=info component=scaffold stack=<stack_name> app=<app_name> msg="pre-flight lint passed"
```

---

### Phase 3 — CLIENT: Git Commit and Push

1. Stage all new/modified files under `stacks/<stack_name>/`.
2. Commit: `feat(scaffold): add app <app_name> to stack <stack_name>`.
3. Push to `main`.

---

### Phase 4 — Conditional: HOST Mount Update (if new mount needed)

If `lxc-compose.yml` was updated with a new bind mount:

```
POST /api/lxc/mounts/sync
Authorization: Bearer <host_token>
Content-Type: application/json

{
  "vmid": <vmid>,
  "stack_name": "<stack_name>",
  "lxc_compose_path": "stacks/<stack_name>/lxc-compose.yml"
}
```

HOST creates `/opt/appdata/<stack_name>/<app_name>-config` on the NVMe, runs `pct set <vmid> -mpN ...`, and restarts the LXC (required for Proxmox to apply the new mount).

---

### Phase 5 — Conditional: CLIENT → LXC Sync (if stack is ACTIVE)

If the stack is `ACTIVE` (and Phase 4 LXC restart, if any, is complete):

```
POST http://<lxc_ip>:8080/api/sync
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{ "force": true, "stack": "<stack_name>" }
```

LXC daemon detects the new app directory, pulls the image, and runs `docker compose up -d`.

**LXC logfmt event:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=<app_name> msg="new app deployed"
```

---

### Phase 6 — CLIENT: Completion

If stack is `ACTIVE`: Modal shows "App deployed." with running status.  
If stack is `INACTIVE`/`SCAFFOLDED`: Modal shows "App scaffolded. Deploy the stack to activate it."

---

## 4. Idempotency

- Creating an app that already exists in Git returns an inline error in the wizard. No files are written.
- Syncing an already-running app is a no-op (`docker compose up -d` with no changes).

---

## 5. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-stack.md` | Full stack creation; app loop (Phase 4) uses this same wizard logic |
| `delete-app-from-stack.md` | Inverse operation |
| `add-core-app-to-stack.md` | Adds predefined core apps (Promtail, Watchtower) |
| `bind-mounts.md` | Mount addition details in Phase 4 |
| `update-active-stacks.md` | Batch sync that handles newly detected apps |
