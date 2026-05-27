# Use Case: Unattended Upgrades

**Tier:** HOST (installs during bootstrap exec) → LXC (OS inside container is patched automatically)  
**Replaces:** Manual `apt upgrade` inside containers  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Every LXC container runs a Debian 12 (or Ubuntu LTS) OS. Without automatic OS patching, the underlying OS accumulates unpatched CVEs even though Docker images are updated via Watchtower.

`unattended-upgrades` is installed and configured during the HOST bootstrap exec phase (`add-stack.md` Phase 9). Once configured, the Debian `unattended-upgrades` daemon applies OS security patches automatically inside the LXC, without human intervention or GitOps involvement.

This is an OS-layer concern, not a Docker-layer concern. It patches:
- The Debian OS packages inside the LXC (kernel, libc, openssl, curl, etc.).
- **Not** Docker images (that's Watchtower's job).
- **Not** the Proxmox host OS (see `os-patching.md`).

---

## 2. Installation (HOST Bootstrap Exec)

During `add-stack.md` Phase 9, after Docker is installed, the HOST daemon runs:

```bash
# Install unattended-upgrades and required dependencies
apt-get install -y unattended-upgrades apt-listchanges

# Enable automatic security upgrades via debconf
echo "unattended-upgrades unattended-upgrades/enable_auto_updates boolean true" \
  | debconf-set-selections
dpkg-reconfigure -pmedium unattended-upgrades
```

**Fail-closed:** If this step exits non-zero, the entire bootstrap is aborted and the LXC is destroyed (see `error-handling-fail-closed.md`).

---

## 3. Configuration

The HOST bootstrap exec writes the following configuration files inside the LXC:

### `/etc/apt/apt.conf.d/50unattended-upgrades`

```apt
Unattended-Upgrade::Allowed-Origins {
    "${distro_id}:${distro_codename}-security";
    "${distro_id}ESMApps:${distro_codename}-apps-security";
    "${distro_id}ESM:${distro_codename}-infra-security";
};

// Auto-remove unused dependencies after upgrade
Unattended-Upgrade::Remove-Unused-Dependencies "true";

// Reboot automatically after kernel patches (at 3am)
Unattended-Upgrade::Automatic-Reboot "true";
Unattended-Upgrade::Automatic-Reboot-Time "03:00";

// Email notifications (optional, requires mailutils)
// Unattended-Upgrade::Mail "root";

// Only security updates — not regular upgrades
Unattended-Upgrade::Origins-Pattern {
    "origin=Debian,codename=${distro_codename},label=Debian-Security";
};
```

### `/etc/apt/apt.conf.d/20auto-upgrades`

```apt
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT::Periodic::AutocleanInterval "7";
```

These settings result in:
- Daily apt list update.
- Daily security patch application.
- Weekly apt cache cleanup.

---

## 4. LXC Restart After Kernel Patches

When a kernel patch is applied, `unattended-upgrades` schedules an automatic reboot at 03:00 LXC local time (configurable via `Automatic-Reboot-Time`).

**LXC restart behaviour:**
1. The Debian `unattended-upgrades` daemon reboots the LXC OS.
2. The LXC container restarts (Proxmox automatically restarts LXC containers with `onboot: true`).
3. Docker daemon starts automatically (`systemctl enable docker`).
4. All Docker containers with `restart: unless-stopped` or `restart: always` restart automatically.
5. The LXC daemon container (`lxc-daemon`) also has `restart: unless-stopped` and comes back online.
6. On startup, the LXC daemon performs the full startup validation (`filesystem-layouts.md` Section 7) and reconnects to the 30-minute sync cycle.

**There is no CLIENT notification for routine OS reboots.** The LXC daemon's reconnection to the SSE stream after a reboot naturally appears in the CLIENT's live log view.

---

## 5. Verification

The LXC daemon periodically checks the `unattended-upgrades` status as part of its health reporting:

```bash
# Check last successful run
systemctl status unattended-upgrades
cat /var/log/unattended-upgrades/unattended-upgrades.log | tail -5
```

The result is included in `GET /api/health` response from the LXC daemon:

```json
{
  "status": "healthy",
  "unattended_upgrades": {
    "last_run": "2026-05-28T03:00:00Z",
    "packages_upgraded": 0,
    "status": "up_to_date"
  }
}
```

The CLIENT's stack detail view shows a small indicator: `OS: ✓ patched 2026-05-28` or `OS: ⚠ never run` if `unattended-upgrades` has never completed a cycle.

**Logfmt event (emitted by LXC daemon on health check):**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="OS patch status" last_run=2026-05-28T03:00:00Z packages_upgraded=0
```

---

## 6. Exclusions

The following packages are pinned and **excluded** from unattended upgrades to prevent Docker from being automatically updated to a version incompatible with the pinned Compose plugin:

```apt
Unattended-Upgrade::Package-Blacklist {
    "docker-ce";
    "docker-ce-cli";
    "docker-buildx-plugin";
    "docker-compose-plugin";
    "containerd.io";
};
```

Docker version updates are managed via the `os-patching.md` flow (manual, controlled).

---

## 7. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-stack.md` | Installed during Phase 9 bootstrap |
| `os-patching.md` | Manual and scheduled OS patch cycles; Docker version updates |
| `error-handling-fail-closed.md` | Bootstrap fails closed if `unattended-upgrades` installation fails |
