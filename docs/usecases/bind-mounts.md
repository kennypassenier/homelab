# Use Case: Bind Mounts

**Tier:** CLIENT (declares in lxc-compose.yml) → HOST (configures via pct set) → LXC (validates device IDs at runtime)  
**Replaces:** Manual `/etc/pve/lxc/<vmid>.conf` editing, `sparse-setup.sh`  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Unprivileged LXC containers cannot access the host filesystem directly. Bind mounts are the mechanism that exposes specific host directories to the LXC at well-known paths. This document defines:
1. How bind mounts are declared (CLIENT, in `lxc-compose.yml`).
2. How they are configured on the Proxmox host (HOST daemon, `pct set`).
3. How correct mount presence is verified inside the LXC at runtime (LXC daemon).
4. The full set of supported mount categories and their canonical paths.

---

## 2. Mount Categories

### Category A — AppData (mandatory for every app with persistent storage)

| Host Path | LXC Container Path | Purpose |
|---|---|---|
| `/opt/appdata/<stack_name>` | `/appdata` | Root appdata directory for the stack |

Docker Compose files then reference subdirectories:
```yaml
volumes:
  - /appdata/<app_name>-config:/config
```

The top-level `/opt/appdata/<stack_name>` → `/appdata` bind mount is the **only** appdata mount entry in `lxc-compose.yml`. All per-app config directories are subdirectories accessed via Docker volume binds within the container.

### Category B — Media Storage (optional; large replaceable files)

| Host Path | LXC Container Path | Purpose |
|---|---|---|
| `/mnt/data/18TB` | `/mnt/data/18TB` | 18TB spinning disk array (media library) |
| `/mnt/data/12TB` | `/mnt/data/12TB` | 12TB spinning disk array (archive) |
| `/mnt/downloads` | `/mnt/downloads` | Download staging directory |

Media mounts are **never** included in Restic backups. They are excluded via Restic's `--exclude` flag.

### Category C — Docker Socket (mandatory for Watchtower, Promtail, Traefik)

The Docker socket is mounted as a volume inside Docker Compose, not as an LXC bind mount:
```yaml
volumes:
  - /var/run/docker.sock:/var/run/docker.sock:ro
```
This does not appear in `lxc-compose.yml`.

### Category D — Custom Mounts (user-defined)

Any additional mount defined by the user in the CLIENT wizard is stored as a custom `mp` entry in `lxc-compose.yml`.

---

## 3. lxc-compose.yml Mount Schema

```yaml
mounts:
  - mp: mp0                              # Proxmox mount point ID (sequential: mp0, mp1, ...)
    source: /opt/appdata/<stack_name>   # Absolute path on Proxmox host
    target: /appdata                    # Absolute path inside LXC
    options: rw                         # rw or ro
  - mp: mp1
    source: /mnt/data/18TB
    target: /mnt/data/18TB
    options: ro
```

**Rules:**
- Mount point IDs (`mp0`, `mp1`, ...) must be sequential with no gaps.
- `source` must be an absolute path beginning with `/opt/appdata/` or `/mnt/`.
- `target` must be an absolute path within the LXC.
- No two entries may share the same `mp` ID, `source`, or `target`.

---

## 4. HOST: Applying Bind Mounts via pct set

### Initial Application (during LXC provisioning — `add-stack.md` Phase 8)

For each mount in `lxc-compose.yml`, HOST executes:

```bash
# Create host directory if it doesn't exist
mkdir -p <source>

# Apply mount to LXC config
pct set <vmid> -<mp> <source>,mp=<target>
```

Example:
```bash
mkdir -p /opt/appdata/media
pct set 102 -mp0 /opt/appdata/media,mp=/appdata

mkdir -p /mnt/data/18TB
pct set 102 -mp1 /mnt/data/18TB,mp=/mnt/data/18TB
```

After all mounts are set, HOST starts the LXC: `pct start <vmid>`.

### Incremental Mount Update (during `update-active-stacks.md` Phase 5)

When `lxc-compose.yml` changes (new app added a new mount):

1. HOST reads current Proxmox config: `pct config <vmid>` → parses existing `mpN` entries.
2. HOST computes diff between desired mounts (from YAML) and current mounts (from pct config).
3. **New mounts:** `mkdir -p <source>`, then `pct set <vmid> -mpN ...` using the next free mount index.
4. **Removed mounts:** HOST emits `level=warn` and does **not** remove the mount automatically (data safety). An amber notice is shown in CLIENT: "Mount `<source>` is no longer declared but was not removed from LXC `<vmid>`. Remove manually if desired."
5. **Changed mounts:** Not supported. A change to `source` or `target` on an existing `mpN` requires manual intervention (deactivate stack, edit pct config, reactivate).
6. If any mount was added: HOST restarts the LXC (`pct restart <vmid>`).

---

## 5. LXC: Runtime Mount Validation

The LXC daemon continuously validates that all declared bind mounts are actually mounted (not accidentally missing due to a host reboot that re-created the directory structure without mounting).

**Validation logic (runs every 10 seconds in a `tokio` interval task):**

```rust
// For each expected mount target (e.g., /appdata, /mnt/data/18TB):
let mount_stat = fs::metadata("/appdata")?;
let root_stat = fs::metadata("/")?;
if mount_stat.dev() == root_stat.dev() {
    // Same device ID = NOT a bind mount = host directory is missing
    emit_warn("mount /appdata appears to be unbounded; st_dev matches root");
} else {
    // Different device ID = real bind mount = healthy
}
```

**On validation failure:**
- LXC daemon emits `level=error` logfmt event to the SSE stream.
- The Secrets/Mounts tab in the LXC TUI shows a Red `✗ UNBOUND` status for the affected mount.
- The LXC daemon does **not** shut down or stop containers automatically (it logs and alerts; the operator decides).
- The alert is visible in the CLIENT TUI via the SSE stream.

**Logfmt events:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> path=/appdata msg="mount validated — bound"
ts=<ISO8601> level=error component=lxc stack=<stack_name> path=/appdata msg="mount NOT bound — st_dev matches root"
```

---

## 6. TUN Device Passthrough (VPN stacks)

Stacks that use a VPN kill-switch (gluetun + qBittorrent pattern) require the `/dev/net/tun` device inside the LXC.

**Detection:** CLIENT automatically detects `network_mode: service:<vpn_app>` in any `docker-compose.yml` within the stack. If detected, CLIENT adds a `tun` feature flag to `lxc-compose.yml`:

```yaml
features:
  - nesting=1
  - fuse=1
  - tun=1        # added automatically if VPN kill-switch is detected
```

**HOST provisioning:** When `tun=1` is present in the `features` list, `pct create` and `pct set` include `--features nesting=1,fuse=1,tun=1`. This adds the following to `/etc/pve/lxc/<vmid>.conf`:
```
lxc.cgroup2.devices.allow: c 10:200 rwm
lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file
```

**LXC validation:** The LXC daemon additionally validates `/dev/net/tun` existence for any stack containing VPN containers:
```
ts=<ISO8601> level=info component=lxc stack=downloader path=/dev/net/tun msg="TUN device present"
ts=<ISO8601> level=error component=lxc stack=downloader path=/dev/net/tun msg="TUN device missing — VPN containers will fail"
```

---

## 7. Unprivileged Container UID/GID Mapping

Unprivileged LXC containers remap UIDs and GIDs by default (root inside LXC = UID 100000 on host). This means:
- `/opt/appdata/<stack_name>` on the host must be owned by UID 100000 (which maps to UID 0 / root inside the LXC).
- HOST daemon sets ownership after `mkdir -p`: `chown -R 100000:100000 /opt/appdata/<stack_name>`.
- For media mounts (`/mnt/data/18TB`), ownership is left as-is (Docker containers run as non-root and access via group permissions).

---

## 8. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-stack.md` | Bind mounts declared in Phase 5; applied in Phase 8 |
| `update-active-stacks.md` | Incremental mount update in Phase 5 |
| `filesystem-layouts.md` | Canonical directory structure that bind mounts expose |
| `gpu-passthrough.md` | Uses same atomic lxc.conf write pattern for device bind mounts |
| `error-handling-fail-closed.md` | Fail-closed on missing mounts during deployment |
