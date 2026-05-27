# Use Case: GPU Passthrough

**Tier:** CLIENT (configuration wizard) → HOST (atomic lxc.conf write)  
**Replaces:** `enable-gpu.sh`  
**Status:** Specification — not yet implemented  

---

## 1. Overview

GPU passthrough grants a specific LXC container access to a GPU device node on the Proxmox host (e.g., for Jellyfin hardware transcoding). This is implemented **exclusively on the HOST daemon tier** using an atomic configuration file update pattern.

The CLIENT provides the wizard interface. The HOST daemon executes the atomic write using a `rename(2)` syscall to prevent lxc.conf corruption if the process is interrupted.

GPU passthrough on Proxmox for unprivileged LXCs requires:
1. Adding `lxc.cgroup2.devices.allow` rules for the GPU device.
2. Adding `lxc.mount.entry` lines to bind-mount the device nodes into the LXC.
3. Setting the correct UID/GID mappings for the `render` and `video` groups.

---

## 2. GPU Device Types Supported

| GPU Type | Device Nodes | Notes |
|---|---|---|
| Intel iGPU (VAAPI) | `/dev/dri/renderD128`, `/dev/dri/card0` | Most common homelab GPU |
| NVIDIA (NVENC/NVDEC) | `/dev/nvidia0`, `/dev/nvidiactl`, `/dev/nvidia-uvm`, `/dev/nvidia-uvm-tools` | Requires NVIDIA drivers on host |
| AMD | `/dev/dri/renderD128`, `/dev/dri/card0` | Same device nodes as Intel iGPU |

---

## 3. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| GPU device nodes exist on the Proxmox host | HOST | `GET /api/hardware/gpu/list` enumerates `/dev/dri/*` and `/dev/nvidia*` |
| Target LXC is `INACTIVE` or `ACTIVE` (not `PROVISIONED`/`SCAFFOLDED`) | CLIENT | State check |
| CLIENT is authenticated to HOST daemon | CLIENT | Bearer token present |

> **Note:** GPU passthrough can be configured while the LXC is running, but takes effect only after an LXC restart. The HOST always restarts the LXC after writing the config.

---

## 4. Step-by-Step Flow

### Phase 1 — CLIENT: GPU Configuration Wizard

**Trigger:** User selects a stack, navigates to the "Hardware" tab, and presses `g` (GPU), or selects "Configure GPU Passthrough" from the context menu.

**Actions:**
1. CLIENT calls `GET /api/hardware/gpu/list` on HOST:
   - Returns all detected GPU device nodes, their major:minor numbers, and the owning host group IDs.
   - Example response:
     ```json
     {
       "gpus": [
         { "type": "intel_igpu", "nodes": ["/dev/dri/renderD128", "/dev/dri/card0"],
           "render_gid": 104, "video_gid": 44 }
       ]
     }
     ```
2. CLIENT shows GPU picker:
   ```
   Configure GPU Passthrough
   Stack: <stack_name>  (VMID: <vmid>)
   
   Detected GPUs:
   ● Intel iGPU — /dev/dri/renderD128, /dev/dri/card0
   
   Target app for transcoding: [ jellyfin ▼ ]
   
   [ Cancel ]  [ Apply ]
   ```
3. The "Target app" dropdown lets the user note which app will use the GPU (informational; written as a comment in the config).

---

### Phase 2 — CLIENT → HOST: Apply GPU Passthrough

CLIENT sends:

```
POST /api/lxc/hardware/gpu
Authorization: Bearer <host_token>
Content-Type: application/json

{
  "vmid": <vmid>,
  "stack_name": "<stack_name>",
  "gpu_type": "intel_igpu",
  "device_nodes": ["/dev/dri/renderD128", "/dev/dri/card0"],
  "render_gid": 104,
  "video_gid": 44
}
```

**HOST daemon actions (atomic config write):**

1. **Read current config:** `std::fs::read_to_string("/etc/pve/lxc/<vmid>.conf")`.
2. **Remove existing GPU entries** (idempotent): strip any lines matching `lxc.cgroup2.devices.allow`, `lxc.mount.entry.*dri*`, and `lxc.mount.entry.*nvidia*` from the in-memory string.
3. **Append new GPU entries** to the in-memory string:
   ```
   # GPU Passthrough — Intel iGPU — managed by homelab CLIENT
   lxc.cgroup2.devices.allow = c 226:0 rwm
   lxc.cgroup2.devices.allow = c 226:128 rwm
   lxc.mount.entry = /dev/dri/card0 dev/dri/card0 none bind,optional,create=file
   lxc.mount.entry = /dev/dri/renderD128 dev/dri/renderD128 none bind,optional,create=file
   ```
4. **Atomic write:**
   - Write the updated content to `/etc/pve/lxc/<vmid>.conf.tmp`.
   - `rename("/etc/pve/lxc/<vmid>.conf.tmp", "/etc/pve/lxc/<vmid>.conf")` — atomic on Linux; no partial-write risk.
5. **Restart LXC:** `pct restart <vmid>` (required for mount entries to take effect).
6. **Wait for LXC to come back online** (same poll as Phase 2 of `activate-stack.md`).
7. **Verify device nodes inside LXC:** `pct exec <vmid> -- ls -la /dev/dri/` — if device nodes not present, emit `level=error` and surface the issue.

**SSE events from HOST:**
```
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="GPU config written atomically"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="LXC restarted for GPU passthrough"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="GPU device nodes verified inside LXC"
```

---

### Phase 3 — CLIENT: Docker Compose Update for Target App

The GPU passthrough is only useful if the target app's `docker-compose.yml` references the device nodes. CLIENT automatically updates the app's compose file:

For Jellyfin (Intel VAAPI example), CLIENT adds:
```yaml
devices:
  - /dev/dri/renderD128:/dev/dri/renderD128
  - /dev/dri/card0:/dev/dri/card0
environment:
  DOCKER_MODS: linuxserver/mods:jellyfin-opencl-intel
```
And updates the group mapping via:
```yaml
group_add:
  - "104"  # render group GID
  - "44"   # video group GID
```

CLIENT:
1. Updates `stacks/<stack_name>/<target_app>/docker-compose.yml` with the device entries.
2. Pre-flight lints the updated file.
3. Commits: `feat(hardware): enable GPU passthrough for <target_app> in <stack_name>`.
4. Pushes to `main`.
5. Triggers a sync on the LXC to redeploy the app with the new device config.

---

### Phase 4 — CLIENT: Completion

Modal shows:
- Green confirmation: "GPU passthrough enabled for <stack_name>."
- Device nodes listed: `/dev/dri/renderD128`, `/dev/dri/card0`.
- Reminder: "Set the hardware acceleration codec in the <target_app> web UI settings."

---

## 5. Remove GPU Passthrough

**Trigger:** User selects "Remove GPU Passthrough" from the Hardware tab.

**Reverse flow:**
1. CLIENT sends `DELETE /api/lxc/hardware/gpu` to HOST with `{ "vmid": <vmid> }`.
2. HOST strips GPU lines from `/etc/pve/lxc/<vmid>.conf` atomically (same write pattern).
3. HOST restarts LXC.
4. CLIENT removes `devices:` and `group_add:` entries from the target app's compose file.
5. Git commit + push + LXC sync.

---

## 6. Idempotency

- Applying GPU passthrough to an LXC that already has it configured strips the old entries first (Phase 2 step 2), ensuring no duplicate `lxc.cgroup2.devices.allow` lines.
- Removing GPU passthrough from an LXC that has no GPU config is a no-op.

---

## 7. Security Constraints

- The HOST daemon validates that `device_nodes` paths begin with `/dev/dri/` or `/dev/nvidia` before writing them to the config, preventing path traversal into arbitrary device injection.
- The `226` (DRI) and `195` (NVIDIA) device major numbers are validated against a hardcoded allowlist in the HOST daemon.

---

## 8. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `bind-mounts.md` | General HOST-side lxc.conf manipulation pattern |
| `add-stack.md` | Stack creation does not automatically add GPU; configured post-creation |
| `transactional-actions.md` | Rollback if LXC restart fails after GPU config write |
