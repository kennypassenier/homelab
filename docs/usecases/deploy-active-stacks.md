# Use Case: Deploy Active Stacks

**Tier:** CLIENT (orchestration loop) → HOST (LXC provision per stack) → LXC (bootstrap + initial sync)  
**Replaces:** Manual `bootstrap-lxc.sh` runs, manual `pct create` sequences  
**Status:** Specification — not yet implemented  

---

## 1. Overview

This use case handles **batch deployment** of all stacks that are in the `SCAFFOLDED` state — meaning their Git directory and `lxc-compose.yml` exist but no LXC has been provisioned yet. This is the primary flow after:

- A fresh repository clone on a new Proxmox node.
- Restoring from a full backup (see `full-backup-restore.md`).
- Adding multiple stacks via the wizard and then deploying them all at once.

The CLIENT iterates through all `SCAFFOLDED` stacks, provisions them on the HOST in **configurable parallelism** (default: sequential, max: 3 concurrent), and streams live progress for all of them into a multi-pane deployment modal.

---

## 2. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| At least one stack in `SCAFFOLDED` state exists | CLIENT | Scans `stacks/*/lxc-compose.yml`; cross-references with `GET /api/lxc/list` on HOST |
| HOST daemon is reachable | CLIENT | `GET /api/health` on HOST |
| Sufficient Proxmox resources for all stacks to be deployed | HOST | `GET /api/node/resources` — CLIENT warns if projected usage exceeds 80% of available RAM/storage |
| All `lxc-compose.yml` files are valid | CLIENT | Pre-flight `serde_yaml` lint of all files before any provisioning starts |

---

## 3. Step-by-Step Flow

### Phase 1 — CLIENT: Discover Deployable Stacks

**Trigger:** User presses `D` (capital) on the Stacks tab, or selects "Deploy All Scaffolded" from the command palette.

**Actions:**
1. CLIENT scans `stacks/*/lxc-compose.yml` to build a candidate list.
2. CLIENT calls `GET /api/lxc/list` on HOST to get all provisioned VMIDs.
3. CLIENT computes the diff: stacks with `lxc-compose.yml` but no matching VMID on HOST → `SCAFFOLDED`.
4. CLIENT runs pre-flight lint on every `lxc-compose.yml` in the candidate list using `serde_yaml`. Any parse failure is shown as a blocking error and that stack is excluded from the deployment queue with a `level=error` entry.
5. CLIENT calls `GET /api/node/resources` to show a resource projection table:
   ```
   Stack       Cores  RAM (MiB)  Disk (GiB)
   ─────────────────────────────────────────
   media         2      2048       32
   paperless     2      2048       32
   monitoring    2      1024       16
   ─────────────────────────────────────────
   TOTAL         6      5120       80
   Available    16     24576      500
   ```

---

### Phase 2 — CLIENT: Deployment Queue Confirmation

CLIENT renders a pre-deployment review modal:
- List of stacks queued for deployment (checkboxes, all pre-selected).
- User can deselect individual stacks to exclude them.
- Parallelism selector: `Sequential (1)` / `2 concurrent` / `3 concurrent`.
- `[ Cancel ]` and `[ Deploy <N> Stacks ]` buttons.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=client msg="deployment queue confirmed" stacks=<N> parallelism=<P>
```

---

### Phase 3 — CLIENT: Multi-Pane Deployment Modal

CLIENT opens the multi-pane deployment progress modal:
- The screen is divided into `N` vertical panes (one per stack being actively deployed, up to the parallelism limit).
- Each pane shows: stack name, current phase label, last 10 logfmt lines (auto-scroll).
- A queue pane on the right shows stacks waiting to start.
- A summary row at the bottom shows: `Deployed: X / Failed: Y / Queued: Z`.

---

### Phase 4 — CLIENT: Per-Stack Deployment (Iterated)

For each stack in the queue, CLIENT executes the full provisioning sequence from `add-stack.md` Phases 8–11:

1. `POST /api/lxc/provision` → HOST (provision LXC, configure mounts, start it).
2. Bootstrap exec inside LXC via HOST (Docker, unattended-upgrades, lxc-daemon).
3. `POST /api/sync` → LXC daemon (first GitOps sync).
4. `GET /api/lxc/<vmid>/ip` → HOST (resolve IP for SSH alias).
5. Idempotent SSH config update.

Each step streams SSE events to the corresponding pane in the modal.

**Parallelism control:**  
A semaphore (`tokio::sync::Semaphore`) with `parallelism` permits controls how many stacks are provisioned concurrently. A stack's permit is released when its provisioning completes (success or failure), unblocking the next queued stack.

---

### Phase 5 — CLIENT: Per-Stack Result Handling

**On success:**
- Pane border turns Green.
- Stack state updated to `ACTIVE`.
- Pane shows summary: duration, apps deployed, IP assigned.

**On failure:**
- Pane border turns Red.
- Error message and last log lines shown.
- Stack state set to `ERROR`.
- Failure does **not** abort remaining stacks in the queue.
- After all stacks complete (success or failure), a summary modal shows the full results.

**Logfmt emitted by CLIENT (per stack):**
```
ts=<ISO8601> level=info component=client stack=<stack_name> msg="deployment started"
ts=<ISO8601> level=info component=client stack=<stack_name> duration_ms=<ms> msg="deployment succeeded"
ts=<ISO8601> level=error component=client stack=<stack_name> msg="deployment failed" error="<reason>"
```

---

### Phase 6 — CLIENT: Final Summary

After all deployments complete:
1. Modal shows a summary table:
   ```
   Stack        Status    Duration    IP              Apps
   ──────────────────────────────────────────────────────
   media        ✓ OK      4m 12s      10.0.1.102      6
   paperless    ✓ OK      3m 48s      10.0.1.103      5
   monitoring   ✗ FAIL    1m 02s      —               —
   ```
2. For any failed stacks, a "Retry Failed" button is available to re-run only the failing stacks.
3. Full session log written to `~/.local/share/homelab/logs/deploy-all-<timestamp>.log`.

---

## 4. Idempotency

- A stack that is already `ACTIVE` or `PROVISIONED` is skipped automatically.
- If provisioning was partially completed (e.g., LXC created but bootstrap failed), HOST's `POST /api/lxc/provision` detects the existing VMID and returns `409 Conflict`. CLIENT shows an error and marks that stack `ERROR`, directing the user to delete and re-scaffold if needed.

---

## 5. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-stack.md` | Single-stack version of this flow |
| `activate-stack.md` | Activates an already-provisioned but stopped stack |
| `update-active-stacks.md` | Updates already-deployed stacks |
| `full-backup-restore.md` | This flow is triggered as part of a full restore |
| `tui-deployment-modal-progress.md` | Multi-pane modal implementation |
| `error-handling-fail-closed.md` | Per-stack failure isolation rules |
