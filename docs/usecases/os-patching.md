# Use Case: OS Patching

**Tier:** CLIENT (triggers) → HOST (executes on Proxmox node) + LXC (executes inside containers)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

OS patching covers two distinct scopes:
1. **LXC OS patching** — controlled `apt upgrade` of all packages (including Docker) inside each LXC container, triggered on-demand or on a schedule.
2. **Proxmox host OS patching** — controlled `apt upgrade` of Proxmox VE packages on the bare-metal node, triggered on-demand only.

Unattended security-only patches are handled separately by `unattended-upgrades.md`. This use case handles full package upgrades including Docker version updates, kernel upgrades, and Proxmox PVE updates.

---

## 2. LXC OS Patch — Individual or All

### Trigger

CLIENT: "OS" tab → "Patch All LXCs" or right-click a stack → "Patch OS"

API call (per LXC):
```
POST http://<lxc_ip>:8080/api/os/patch
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{
  "scope": "all",              // "security" | "all" (default: "all")
  "include_docker": true,      // whether to upgrade Docker packages
  "reboot_if_required": true   // reboot if kernel update was applied
}
```

### Flow Inside LXC Daemon

```
1. LXC daemon receives POST /api/os/patch
2. Check if sync lock is held → if yes, return 423 Locked (patch waits for sync to finish)
3. Acquire patch lock (/run/lxc-daemon/patch.lock)
4. Run: apt-get update -q
5. If include_docker=false → pin Docker packages in apt hold before upgrade
6. Run: DEBIAN_FRONTEND=noninteractive apt-get upgrade -y
7. If include_docker=false → remove apt hold on Docker packages
8. Check: /var/run/reboot-required exists?
   → if yes and reboot_if_required=true → schedule reboot via systemd-run --on-active=30s /sbin/reboot
9. Release patch lock
10. Stream completion event via SSE
```

**Logfmt events:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="OS patch started" scope=all include_docker=true
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="OS patch complete" packages_upgraded=<N> reboot_required=true
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="OS reboot scheduled in 30s"
```

---

## 3. LXC Patch — Batch ("Patch All")

The CLIENT sends patch requests to all active LXC stacks in sequence (not parallel) to avoid:
- Multiple simultaneous LXC reboots disrupting services.
- Overloading the Proxmox host with concurrent Docker upgrades.

```
For each stack in (active stacks, ordered by priority):
    1. POST /api/os/patch to LXC
    2. Wait for SSE completion event (or timeout 300s)
    3. If reboot_required: wait for LXC daemon to come back online (poll /health, 120s timeout)
    4. Continue to next stack
```

**CLIENT progress modal:** Displays a vertical list of stacks with status indicators:
- `⟳ Patching` — currently upgrading
- `✓ Up to date` — no packages upgraded
- `✓ Patched` — packages upgraded, no reboot
- `↻ Rebooting` — reboot in progress, waiting for LXC daemon to reconnect
- `✗ Failed` — error response or timeout

---

## 4. Docker Version Updates

Docker package upgrades are excluded from unattended upgrades by default (see `unattended-upgrades.md`). When `include_docker=true` in the patch request:
- `docker-ce`, `docker-ce-cli`, `docker-compose-plugin`, and `containerd.io` are upgraded to the latest available version from the Docker apt repository.
- After Docker upgrade, the LXC daemon verifies Docker is running: `docker info` must return successfully.
- All Docker containers with `restart: unless-stopped` restart automatically after the Docker daemon restarts.

**Fail-closed:** If `docker info` fails after upgrade, the LXC daemon emits `level=error` and the patch is marked as failed.

---

## 5. Proxmox Host OS Patch

**Trigger:** CLIENT HOST TUI → "Patch Proxmox" button (requires explicit confirmation).

API call:
```
POST https://<host_ip>:8443/api/os/patch
Authorization: Bearer <host_token>
Content-Type: application/json

{
  "scope": "pve",              // "security" | "pve" | "all"
  "reboot_if_required": false  // Proxmox reboot is NEVER automatic — always manual
}
```

**HOST daemon behaviour:**
1. Run `apt-get update && apt-get dist-upgrade -y` (or `pveupgrade` for PVE packages).
2. Stream progress via SSE.
3. Check `/var/run/reboot-required` — if present, emit a warning event.
4. **Never reboot automatically.** A Proxmox host reboot stops ALL LXC containers; it must always be a conscious manual decision.

**CLIENT warning modal before Proxmox patch:**
```
⚠  Proxmox Host Patch

This will run apt dist-upgrade on the Proxmox VE host.
If a kernel patch is applied, a manual reboot will be required.
All LXC containers will be stopped during the host reboot.

Recommended: take a full backup first.

[ Cancel ]   [ Proceed ]
```

---

## 6. Patch Status Visibility

The LXC daemon's `GET /api/health` response includes:

```json
{
  "os_patch": {
    "last_full_upgrade": "2026-05-20T03:00:00Z",
    "upgradable_packages": 3,
    "reboot_required": false
  }
}
```

The CLIENT Stacks tab displays:
- `OS: ✓` — fewer than 5 upgradable packages, no reboot required.
- `OS: ⚠ 3 pending` — 3 upgradable packages.
- `OS: ⚠ reboot required` — kernel was patched.

The HOST daemon's `GET /api/health` includes equivalent host-level upgrade status.

---

## 7. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `unattended-upgrades.md` | Handles security-only patches; runs daily without triggering this flow |
| `manual-backup-all.md` | Recommended before Proxmox host patching |
| `error-handling-fail-closed.md` | Docker upgrade failure → fail-closed |
| `tui-deployment-modal-progress.md` | Batch patch progress rendered in CLIENT modal |
