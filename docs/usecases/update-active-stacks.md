# Use Case: Update Active Stacks

**Tier:** CLIENT (orchestration) → LXC (pull + sync) → HOST (optional: mount re-sync)  
**Replaces:** Manual `docker compose pull`, manual `git pull` inside containers, Watchtower ad-hoc triggers  
**Status:** Specification — not yet implemented  

---

## 1. Overview

"Update active stacks" is a **GitOps-driven refresh** operation that:
1. Pulls the latest Git changes for all active stacks.
2. Pulls updated Docker images for all running apps.
3. Detects and adds **missing apps** that exist in Git but are not running in Docker.
4. Restarts only containers whose images have changed (no unnecessary downtime).
5. Streams live progress to the CLIENT modal.

This is the day-to-day operational workflow — distinct from initial deployment. Watchtower handles image-only updates on a schedule, but this use case handles **structural updates** (new apps, config changes, compose file edits) that require a full GitOps sync cycle.

---

## 2. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| At least one stack is in `ACTIVE` state | CLIENT | Local state + `GET /api/lxc/list` on HOST confirms running VMIDs |
| No active Restic backup is in progress | HOST | `GET /api/backup/status` returns `idle` for all target stacks |
| Git `main` branch is reachable | CLIENT | `git ls-remote` check before queuing |

---

## 3. Step-by-Step Flow

### Phase 1 — CLIENT: Discover Update Candidates

**Trigger:** User presses `u` on the Stacks tab, or selects "Update All Active" from the command palette. Can also be triggered per-stack with `u` from a stack detail view.

**Actions:**
1. CLIENT runs `git fetch origin main` locally to check for new commits.
2. CLIENT computes `git log HEAD..origin/main --name-only --pretty=format:` to get a list of changed paths.
3. CLIENT maps changed paths to stacks:
   - `stacks/media/**` → `media` stack needs update.
   - `stacks/paperless/**` → `paperless` stack needs update.
4. Only stacks with changes in their `stacks/<stack_name>/` directory are queued for a forced sync.
5. If no Git changes are detected, CLIENT shows an info notice: "All stacks are up to date with Git." However, the user can still force a sync via `F` (force flag).

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=client msg="git fetch complete" changed_stacks=<N>
```

---

### Phase 2 — CLIENT: Missing App Detection

For each candidate stack, CLIENT compares:
- Apps defined in `stacks/<stack_name>/` (subdirectories with `docker-compose.yml`).
- Apps running in Docker, obtained from the LXC daemon: `GET http://<lxc_ip>:8080/api/containers`.

**Missing app:** Exists in Git but has no running or stopped Docker Compose project.  
**Orphaned app:** Running in Docker but no longer exists in Git (marked for GC).

CLIENT shows a diff summary in the update confirmation modal:
```
Stack: media
  ✓ jellyfin      running  (image update available)
  ✓ sonarr        running  (up to date)
  + bazarr        MISSING  (will be deployed)
  - old-app       ORPHANED (will be garbage-collected)
```

---

### Phase 3 — CLIENT: Update Queue Confirmation

CLIENT renders a confirmation modal listing all stacks and their diffs:
- Checkboxes to include/exclude individual stacks.
- Toggle for GC (garbage collection of orphaned apps): default enabled.
- `[ Cancel ]` and `[ Update <N> Stacks ]` buttons.

---

### Phase 4 — CLIENT → LXC: Trigger Sync Per Stack

For each stack in the update queue, CLIENT sends:

```
POST http://<lxc_ip>:8080/api/sync
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{
  "force": false,
  "stack": "<stack_name>",
  "pull_images": true,
  "gc": true
}
```

**LXC daemon sync actions (in order):**

1. **Acquire lock:** `/tmp/gitops.lock` — abort if already locked (another sync in progress).
2. **Git pull:** `git -C /opt/homelab pull --ff-only origin main` within the sparse checkout scope. If pull fails (diverged, conflicts), emit `level=error` and abort.
3. **Run `setup.sh`** if present at `stacks/<stack_name>/setup.sh` (see `pre-sync-hooks.md`).
4. **Ephemeral secrets refresh:** Spin up secrets container to regenerate `.env` files if any secrets-relevant file changed.
5. **For each app directory (sorted by dependency order):**
   - `docker compose pull -q` — pulls new images silently.
   - `docker compose up -d --remove-orphans` — restarts only changed containers.
6. **Garbage collection (if `gc: true`):** For each running Compose project with no corresponding directory in Git, run `docker compose down --remove-orphans` and emit `level=info gc=true app=<app>` log.
7. **Release lock.**

**LXC SSE logfmt events (examples):**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="git pull complete" sha=<new_sha>
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=jellyfin msg="image pulled" image=linuxserver/jellyfin:latest
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=jellyfin msg="container restarted"
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=bazarr msg="new app deployed"
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=old-app gc=true msg="orphaned app removed"
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="sync complete" apps_updated=<N> apps_added=<M> apps_removed=<K>
```

---

### Phase 5 — HOST: Mount Re-Sync (if new apps have new mounts)

If the Git diff for a stack contains changes to `lxc-compose.yml` (e.g., a new app added a new bind mount), CLIENT sends a mount update request to HOST:

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

**HOST actions:**
1. Parse the updated `lxc-compose.yml`.
2. Compare declared mounts against `pct config <vmid>` current mounts.
3. For each new mount: `mkdir -p /opt/appdata/<stack_name>/<app>-config` on host NVMe, then `pct set <vmid> -mpN <source>,mp=<target>`.
4. For each removed mount: emit a warning but do **not** remove the mount automatically (data safety).
5. Restart the LXC if any mounts were changed (required for Proxmox to apply bind mount changes): `pct restart <vmid>`.
6. Wait for LXC to come back online; re-trigger sync.

**HOST SSE events:**
```
data: ts=<ISO8601> level=info component=host stack=<stack_name> msg="mount sync: added mp2 /opt/appdata/media/bazarr-config"
data: ts=<ISO8601> level=warn component=host stack=<stack_name> msg="mount sync: removed mounts are not auto-deleted; review manually"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="LXC restarted for mount changes"
```

---

### Phase 6 — CLIENT: Completion Summary

After all stacks complete:
1. Modal shows per-stack result summary (same format as `deploy-active-stacks.md` Phase 6).
2. Stacks tab refreshes; all updated apps show current image digests.
3. Full session log written to `~/.local/share/homelab/logs/update-<timestamp>.log`.

---

## 4. Watchtower vs. This Flow

| Scenario | Handled By |
|---|---|
| Image-only update (same compose, new tag) | Watchtower (scheduled, inside LXC) |
| Compose file changed (new env var, label, port) | This flow (GitOps sync via CLIENT) |
| New app added to stack | This flow (missing app detection) |
| App removed from stack | This flow (GC on sync) |
| `lxc-compose.yml` changed (new mount) | This flow (Phase 5, HOST mount re-sync) |

---

## 5. Idempotency

- Running an update on a stack with no changes (same Git SHA, same images) results in zero container restarts. `docker compose up -d` is idempotent.
- Missing app deployment is safe to re-run: `docker compose up -d` with an already-running service is a no-op.

---

## 6. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-app-to-stack.md` | Adding a single new app (triggers same sync path) |
| `delete-app-from-stack.md` | GC of orphaned apps described in Phase 4 |
| `pre-sync-hooks.md` | `setup.sh` execution in Phase 4 step 3 |
| `post-deploy-actions.md` | Post-sync health checks run after Phase 4 |
| `bind-mounts.md` | Mount re-sync details in Phase 5 |
| `tui-deployment-modal-progress.md` | Live SSE modal used throughout |
