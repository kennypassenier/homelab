# Use Case: Full Backup Restore (Disaster Recovery)

**Tier:** CLIENT (orchestrates) → HOST (provisions + restores) → LXC (deploys stacks)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

A full restore is used when the Proxmox host has been lost entirely (hardware failure, corruption, or migration to a new machine). The goal is to bring the entire homelab back to the last backed-up state on a fresh Proxmox VE installation.

**Prerequisites before starting a full restore:**
1. Proxmox VE freshly installed on new or restored hardware.
2. Host daemon bootstrap script run (`host.sh bootstrap`) — installs Rust toolchain, builds and starts the HOST daemon.
3. Git repository cloned on CLIENT: `git clone <repo_url> ~/Projects/homelab`.
4. Restic repository accessible (same remote backend or restored local repo).
5. `RESTIC_REPOSITORY` and `RESTIC_PASSWORD` configured in HOST daemon environment.
6. All MAC-based DHCP reservations re-applied in OPNsense (or stacks will receive wrong IPs).

---

## 2. Recovery Strategy

Full restore is **not** a single API call. It is a sequential multi-step process orchestrated by the CLIENT. The process:

1. Re-provision all LXC containers from Git state (stacks in `stacks/` directory).
2. Restore each stack's appdata from the latest Restic snapshot.
3. Trigger initial sync on each LXC to deploy Docker containers.

Steps 1–3 reuse existing use cases: `deploy-active-stacks.md` (step 1), `individual-backup-restore.md` (step 2), and `update-active-stacks.md` (step 3).

---

## 3. Full Restore Phases

```
Phase 0: Pre-flight checks (CLIENT)
  → Git repo up-to-date (git pull)
  → HOST daemon reachable (GET /api/health)
  → Restic repo accessible (GET /api/backup/snapshots → must return ≥1 result)
  → CLIENT shows summary: N stacks to provision, latest snapshot date

Phase 1: Provision all stacks (CLIENT → HOST)
  → Identical to deploy-active-stacks.md
  → For each stack in stacks/ directory:
      POST /api/lxc/provision → HOST creates LXC + NVMe dir + bootstrap
  → Serial execution (max 1 concurrent provision to avoid Proxmox API race conditions)

Phase 2: Wait for all LXC daemons to be online
  → Poll GET /health on each new LXC (5s interval, 120s max per LXC)
  → All must be online before starting restore

Phase 3: Restore appdata from latest snapshot (CLIENT → HOST)
  → For each stack, call POST /api/backup/restore { stack_name, snapshot_id: "latest" }
  → Serial execution (Restic restore is I/O bound; parallel would be slower)
  → "latest" is resolved by HOST as: snapshots sorted by time descending, first match for stack

Phase 4: Initial sync on all LXCs (CLIENT → each LXC)
  → POST /api/sync { force: true } to each LXC daemon
  → Each LXC pulls Git, runs setup.sh, and runs docker compose up -d
  → Parallel execution (max 3 concurrent syncs) using tokio semaphore

Phase 5: Completion report (CLIENT)
  → Display table: stack name | provision status | restore status | sync status
  → Any failures listed with actionable next steps
```

---

## 4. "Latest Snapshot" Resolution

When `snapshot_id: "latest"` is passed to `POST /api/backup/restore`, the HOST resolves it to the most recent snapshot that includes the stack's path:

```rust
async fn resolve_latest_snapshot(stack_name: &str) -> Result<String> {
    let output = Command::new("restic")
        .args(["snapshots", "--json", "--path", &format!("/opt/appdata/{}", stack_name)])
        .output()
        .await?;
    let snapshots: Vec<Snapshot> = serde_json::from_slice(&output.stdout)?;
    snapshots
        .into_iter()
        .max_by_key(|s| s.time)
        .map(|s| s.id)
        .ok_or(Error::NoSnapshotFound(stack_name.to_string()))
}
```

If no snapshot exists for a stack (e.g., a brand-new stack that was never backed up), Phase 3 skips that stack and logs an info event: `"no snapshot found for <stack_name>; skipping restore"`.

---

## 5. CLIENT: Disaster Recovery Wizard

"Backups" tab → "Disaster Recovery" button opens the DR wizard:

```
╔══════════════════════════════════════════════════════╗
║  Disaster Recovery Wizard                             ║
╠══════════════════════════════════════════════════════╣
║  Step 1: Pre-flight checks                            ║
║                                                       ║
║  ✓ Git repository up to date (HEAD: a1b2c3d)         ║
║  ✓ HOST daemon online (Proxmox 8.2, 2 CPU, 16 GB)   ║
║  ✓ Restic repository accessible (231 snapshots)       ║
║  ✓ Latest snapshot: 2026-05-29 03:06 (1.1 TB)        ║
║                                                       ║
║  Stacks to recover: 7                                 ║
║  (cloudflared, downloader, gateway, media,            ║
║   monitoring, paperless, vikunja)                     ║
║                                                       ║
║  [ Cancel ]              [ Begin Recovery ]           ║
╚══════════════════════════════════════════════════════╝
```

Progress display uses the same multi-pane TUI modal as `deploy-active-stacks.md` with an additional "Restore" column.

---

## 6. Error Isolation

Each stack is treated independently:
- Provision failure for one stack does not block others from being provisioned.
- Restore failure for one stack does not block others from being restored.
- Sync failure for one stack does not block others from syncing.

After all phases complete, the CLIENT shows a summary report:

| Stack | Provision | Restore | Sync |
|---|---|---|---|
| cloudflared | ✓ | ✓ | ✓ |
| downloader | ✓ | ✓ | ✓ |
| media | ✓ | ⚠ no snapshot | ✓ |
| paperless | ✓ | ✓ | ✗ setup.sh failed |

Failures include a "Retry" button that re-runs only the failed stacks.

---

## 7. Post-Recovery Checklist

The CLIENT wizard shows a final checklist reminding the user of tasks that cannot be automated:

```
Post-Recovery Manual Checklist:
  □ Verify OPNsense DHCP reservations are correct
  □ Verify Traefik Let's Encrypt certificates renewed (check /appdata/traefik/acme.json)
  □ Verify CrowdSec bouncer re-enrolled (check gateway stack logs)
  □ Verify Cloudflare Tunnel token is active
  □ Check Grafana dashboards for alert gaps
  □ Run a manual backup after first full day to start fresh retention chain
```

---

## 8. Logfmt Events

```
ts=<ISO8601> level=info component=client msg="disaster recovery started" stacks=7
ts=<ISO8601> level=info component=client msg="provision complete" stack=<name>
ts=<ISO8601> level=info component=client msg="restore complete" stack=<name> snapshot=<id>
ts=<ISO8601> level=info component=client msg="sync complete" stack=<name>
ts=<ISO8601> level=info component=client msg="disaster recovery complete" stacks_ok=6 stacks_failed=1
```

---

## 9. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `deploy-active-stacks.md` | Phase 1 reuses this entire flow |
| `individual-backup-restore.md` | Phase 3 calls this per-stack |
| `update-active-stacks.md` | Phase 4 reuses the sync trigger |
| `backup-scheduling.md` | Creates the snapshots this restore relies on |
| `add-stack.md` | Original provisioning flow; reused in Phase 1 |
