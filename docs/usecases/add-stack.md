# Use Case: Add Stack

**Tier:** CLIENT (wizard) → HOST (LXC provisioning) → LXC (bootstrap & GitOps)  
**Replaces:** `create-new-stack.sh`, `create-new-app.sh`, `bootstrap-lxc.sh`, `node-sync.sh` (initial sync)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

A "stack" is a logical grouping of Docker Compose applications that share a single unprivileged LXC container on the Proxmox host. Creating a stack means:

1. Collecting all configuration through a CLIENT TUI wizard.
2. Generating a declarative `lxc-compose.yml` that fully describes the LXC.
3. Scaffolding the Git directory structure and all `docker-compose.yml` files.
4. Sending the `lxc-compose.yml` to the HOST daemon to provision the LXC.
5. The LXC daemon bootstrapping itself (Docker, unattended-upgrades, sparse checkout).
6. An immediate GitOps sync triggered by the CLIENT over the LXC HTTP Push API.

Every step is streamed back to the CLIENT's deployment modal in real time over SSE.

---

## 2. Preconditions

| Condition | Owner | How Verified |
|---|---|---|
| CLIENT is authenticated to HOST daemon | CLIENT | Bearer token in `~/.config/homelab/client.toml` |
| A Debian 12 LXC template exists on Proxmox local storage | HOST | HOST returns available templates via `GET /api/templates` |
| `/opt/appdata/<stack>` does not yet exist on host NVMe | HOST | HOST checks path before provisioning |
| Stack name does not already exist in `stacks/` in Git | CLIENT | CLIENT checks local Git working tree |
| Sufficient Proxmox resources (cores, RAM, disk) available | HOST | HOST returns node resource summary via `GET /api/node/resources` |

---

## 3. Step-by-Step Flow

### Phase 0 — CLIENT: Open "Add Stack" Wizard

**Trigger:** User presses `n` on the Stacks tab of the CLIENT TUI, or selects "Add Stack" from the command palette.

**Actions:**
1. CLIENT opens a full-screen wizard modal (Ratatui `Popup` widget with rounded border, Cyan accent, drop-shadow layer).
2. CLIENT calls `GET /api/templates` on the HOST daemon to populate the OS template picker.
3. CLIENT calls `GET /api/node/resources` on the HOST daemon to show live resource availability.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=wizard msg="Add Stack wizard opened"
```

---

### Phase 1 — CLIENT: Wizard Step 1 — Stack Identity

**Inputs collected (one field per screen, navigated with Tab/Enter):**

| Field | Description | Validation |
|---|---|---|
| `stack_name` | Lowercase, hyphen-separated name (e.g. `media`) | Regex `^[a-z][a-z0-9-]{1,28}$`; must not exist in `stacks/` |
| `hostname` | LXC hostname shown in Proxmox (defaults to `stack_name`) | Same regex |
| `vmid` | Proxmox VMID (101–9999; CLIENT suggests next free ID by querying HOST) | Integer, unique, confirmed available |
| `description` | Free-text description stored in `lxc-compose.yml` | Optional, max 120 chars |

**Validation errors** render inline beneath the field in Red with a `✗` prefix. Focus does not advance until the field passes.

---

### Phase 2 — CLIENT: Wizard Step 2 — Hardware Resources

| Field | Default | Notes |
|---|---|---|
| `cores` | 2 | 1–32 |
| `memory_mb` | 2048 | In MiB; minimum 512 |
| `swap_mb` | 512 | In MiB; 0 to disable |
| `rootfs_size_gb` | 32 | In GiB; minimum 8 |
| `rootfs_storage` | `local-lvm` | CLIENT queries HOST for available storage pools |
| `os_template` | Latest Debian 12 `.tar.zst` | Populated from HOST `GET /api/templates` response |

---

### Phase 3 — CLIENT: Wizard Step 3 — Networking & MAC Address

**Actions:**
1. CLIENT generates a random Locally Administered MAC address:
   - Byte 0: set bit 1 (locally administered), clear bit 0 (unicast). Example: `0xAA`.
   - Bytes 1–5: cryptographically random (`rand::random::<[u8; 5]>()`).
   - Formatted as `AA:BB:CC:DD:EE:FF`.
2. CLIENT displays the generated MAC and an "Regenerate" button (`r` key).
3. User is shown a reminder notice: *"Register this MAC in OPNsense DHCP before activating the stack."*

| Field | Value | Notes |
|---|---|---|
| `hwaddr` | Generated MAC | Stored in `lxc-compose.yml`; used for static DHCP in OPNsense |
| `bridge` | `vmbr0` | Editable dropdown populated from HOST network list |
| `ip` | `dhcp` | Always DHCP; static IP is enforced at the router layer |

---

### Phase 4 — CLIENT: Wizard Step 4 — App Loop

The wizard enters an **app loop**: the user defines one or more Docker Compose applications that will live in this stack.

**For each app, the following fields are collected:**

| Field | Description | Notes |
|---|---|---|
| `app_name` | Lowercase, hyphen-separated (e.g. `jellyfin`) | Regex `^[a-z][a-z0-9-]{1,28}$` |
| `image` | Docker image reference (e.g. `linuxserver/jellyfin:latest`) | Must be non-empty |
| `internal_port` | Container port the app listens on | Integer 1–65535 |
| `external_port` | Host-side port mapping (optional; omit for Traefik-only) | Optional |
| `traefik_enabled` | Toggle Traefik reverse proxy labels | Boolean; default true |
| `traefik_subdomain` | Subdomain for Traefik routing (e.g. `jellyfin`) | Required if `traefik_enabled` |
| `traefik_entrypoint` | `websecure` (default) or `web` | Dropdown |
| `vpn_kill_switch` | Attach to a VPN container via `network_mode: service:<vpn_app>` | Optional; shows app picker |
| `healthcheck_cmd` | Docker healthcheck command | Optional |
| `healthcheck_interval` | e.g. `30s` | Optional; default `30s` |
| `capabilities` | `cap_add` entries (e.g. `NET_ADMIN`) | Optional multiselect |
| `restart_policy` | `unless-stopped` / `always` / `on-failure` | Dropdown; default `unless-stopped` |
| `extra_env_vars` | Key=Value pairs injected as `environment:` entries | Optional; loaded from ephemeral secrets at runtime |

After completing each app, a confirmation row appears. The user can press `a` to add another app or `Enter`/`→` to proceed.

**Core app injection (automatic, always added at the end of the loop):**

The wizard automatically appends the following core service stubs if they are not already present among the user-defined apps:

| Core App | Condition |
|---|---|
| `promtail` | Always added; pre-configured with the stack name as a Loki label |
| `watchtower` | Always added; configured with `com.centurylinklabs.watchtower.enable=true` |
| `traefik` | Only if no Traefik instance is detected in any other stack on this host |

The user sees a multiselect confirmation popup listing these injections and may deselect individual ones.

---

### Phase 5 — CLIENT: Wizard Step 5 — Mounts & Storage

For each app that has a persistent config directory, the wizard shows a mount table:

| Mount Point | Host Path | Container Path |
|---|---|---|
| AppData config | `$stack_name/$app_name-config` | `/config` |
| Media data (optional) | `/mnt/data/18TB` or `/mnt/data/12TB` | `/mnt/data/18TB` |
| Downloads (optional) | `/mnt/downloads` | `/mnt/downloads` |

The wizard renders an editable table. The user can add custom mount point pairs. Every mount becomes a `pct set` bind mount entry in `lxc-compose.yml` and a `volumes:` entry in the app's `docker-compose.yml`.

---

### Phase 6 — CLIENT: Generate & Write Artifacts

**Actions (all performed locally in the Git working tree before any push):**

#### 6a. Generate `lxc-compose.yml`

Written to: `stacks/<stack_name>/lxc-compose.yml`

```yaml
# AUTO-GENERATED by CLIENT — do not edit manually
vmid: <vmid>
hostname: <hostname>
description: "<description>"
ostemplate: local:vztmpl/<template_filename>
cores: <cores>
memory: <memory_mb>
swap: <swap_mb>
rootfs: <rootfs_storage>:<rootfs_size_gb>
unprivileged: true
features:
  - nesting=1
  - fuse=1
net0: name=eth0,bridge=<bridge>,ip=dhcp,hwaddr=<hwaddr>
mounts:
  - mp: mp0
    source: /opt/appdata/<stack_name>
    target: /appdata
    options: rw
  # Additional mounts appended per Phase 5
tags:
  - gitops
  - <stack_name>
```

#### 6b. Scaffold Git directory structure

For each app (including core apps):

```
stacks/<stack_name>/
  lxc-compose.yml           ← generated above
  setup.sh                  ← pre-deploy hook scaffold (chmod +x)
  <app_name>/
    docker-compose.yml      ← full service definition (see 6c)
  <app_name>-config/        ← empty dir; bind-mounted from host NVMe
    .gitkeep
```

`stacks/<stack_name>/<app_name>-config/.gitkeep` ensures the directory is committed to Git even when empty.

#### 6c. Generate `docker-compose.yml` per app

Each `docker-compose.yml` is generated with the following sections:

```yaml
# AUTO-GENERATED by CLIENT homelab scaffold
# Stack: <stack_name> | App: <app_name>
services:
  <app_name>:
    image: <image>
    container_name: <app_name>
    restart: <restart_policy>
    # Healthcheck (if configured)
    healthcheck:
      test: ["CMD", <healthcheck_cmd>]
      interval: <healthcheck_interval>
      timeout: 10s
      retries: 3
    # Capabilities (if configured)
    cap_add:
      - <capability>
    # VPN kill-switch (if configured)
    network_mode: "service:<vpn_app>"
    ports:
      - "<external_port>:<internal_port>"   # omitted if Traefik-only
    volumes:
      - ../<app_name>-config:/config
      # Additional mounts from Phase 5
    environment:
      - <KEY>=<VALUE>
    labels:
      # Watchtower
      com.centurylinklabs.watchtower.enable: "true"
      # Traefik (if traefik_enabled)
      traefik.enable: "true"
      traefik.http.routers.<app_name>.rule: "Host(`<traefik_subdomain>.<domain>`)"
      traefik.http.routers.<app_name>.entrypoints: "<traefik_entrypoint>"
      traefik.http.routers.<app_name>.tls: "true"
      traefik.http.routers.<app_name>.middlewares: "crowdsec@file"
      traefik.http.services.<app_name>.loadbalancer.server.port: "<internal_port>"
```

#### 6d. Generate `promtail` service

Written to `stacks/<stack_name>/promtail/docker-compose.yml` with:
- `stack` label hardcoded to `<stack_name>`.
- Loki URL injected via `${LOKI_URL}` env var (runtime-injected by ephemeral secrets container).
- Log scrape job targeting `/var/log/docker/<stack_name>-*.log`.

#### 6e. Generate `watchtower` service

Written to `stacks/<stack_name>/watchtower/docker-compose.yml` with:
- `--label-enable` flag so only labelled containers are updated.
- `--cleanup` to remove old images.
- `--schedule "0 3 * * *"` for nightly updates.

#### 6f. Pre-flight YAML validation

CLIENT lints every generated `docker-compose.yml` using `serde_yaml`:
- All required fields present.
- No duplicate service names across the stack.
- Traefik router names are globally unique across all stacks.
- Mounts reference paths that will exist on the host after provisioning.

If any validation fails, CLIENT renders an inline error modal and **does not proceed** until fixed.

**Logfmt emitted by CLIENT (per artifact written):**
```
ts=<ISO8601> level=info component=scaffold stack=<stack_name> app=<app_name> msg="docker-compose.yml written"
ts=<ISO8601> level=info component=scaffold stack=<stack_name> msg="lxc-compose.yml written"
ts=<ISO8601> level=info component=scaffold stack=<stack_name> msg="pre-flight lint passed"
```

---

### Phase 7 — CLIENT: Git Commit

**Actions:**
1. CLIENT stages all new files under `stacks/<stack_name>/`.
2. CLIENT creates a commit: `feat(scaffold): add stack <stack_name> with <N> apps`.
3. CLIENT pushes to `main`.
4. Commit SHA is stored in CLIENT state for correlation with upcoming deployment logs.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=git stack=<stack_name> sha=<sha> msg="stack committed and pushed to main"
```

---

### Phase 8 — CLIENT → HOST: Provision LXC

**Actions:**
1. CLIENT opens the "Deploying Stack" progress modal (Ratatui `Popup`, full-screen overlay, Cyan header, live log pane scrolling at bottom).
2. CLIENT sends:
   ```
   POST /api/lxc/provision
   Authorization: Bearer <host_token>
   Content-Type: application/json
   
   {
     "lxc_compose_path": "stacks/<stack_name>/lxc-compose.yml",
     "stack_name": "<stack_name>",
     "appdata_path": "/opt/appdata/<stack_name>"
   }
   ```
3. CLIENT simultaneously opens an SSE connection to `GET /api/events/stream` on HOST to receive live provisioning logs.

**HOST daemon actions (sequential, fail-closed):**

| Step | Command | Fail behaviour |
|---|---|---|
| Parse `lxc-compose.yml` | Deserialize via `serde_yaml` | Abort; return `400 Bad Request` with error detail |
| Check VMID availability | `GET /api/node/vms` | Abort if VMID in use |
| Create `/opt/appdata/<stack_name>` on host NVMe | `std::fs::create_dir_all` | Abort if path already exists |
| Create LXC | `pct create <vmid> <template> --cores <c> --memory <m> --swap <s> --rootfs <r> --net0 <n> --unprivileged 1 --features nesting=1,fuse=1 --hostname <h>` | Abort; emit `level=error` SSE event |
| Set bind mounts | `pct set <vmid> -mp0 /opt/appdata/<stack_name>,mp=/appdata` (one `pct set` per mount) | Abort; attempt to delete LXC and NVMe dir |
| Start LXC | `pct start <vmid>` | Abort; emit `level=error` SSE event |
| Wait for LXC to be reachable (network up) | Poll `pct exec <vmid> -- hostname` with 30s timeout | Abort |
| Run bootstrap exec | `pct exec <vmid> -- /bin/bash -c "..."` (see Phase 9) | Abort; emit `level=error` SSE event |

**Every action is preceded and followed by a structured SSE log event:**
```
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="pct create invoked"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="LXC created successfully"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="bind mount mp0 configured"
data: ts=<ISO8601> level=info component=host stack=<stack_name> vmid=<vmid> msg="LXC started"
```

---

### Phase 9 — HOST: Bootstrap Exec Inside LXC

Immediately after the LXC starts, the HOST daemon runs a bootstrap sequence via the Proxmox Exec API (`pct exec`). This replaces the legacy `bootstrap-lxc.sh`.

**Bootstrap steps (run inside the LXC as root, in order):**

1. **System update:**
   ```bash
   apt-get update -qq && apt-get upgrade -y -qq
   ```
2. **Install `unattended-upgrades`:**
   ```bash
   apt-get install -y unattended-upgrades apt-listchanges
   dpkg-reconfigure -pmedium unattended-upgrades
   ```
3. **Install Docker Engine (official apt repository, pinned):**
   ```bash
   apt-get install -y ca-certificates curl gnupg
   install -m 0755 -d /etc/apt/keyrings
   curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
   chmod a+r /etc/apt/keyrings/docker.gpg
   echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] \
     https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" \
     > /etc/apt/sources.list.d/docker.list
   apt-get update -qq
   apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
   systemctl enable --now docker
   ```
4. **Pull and start the LXC daemon container:**
   ```bash
   docker pull ghcr.io/<org>/lxc-daemon:latest
   docker run -d \
     --name lxc-daemon \
     --restart unless-stopped \
     -v /var/run/docker.sock:/var/run/docker.sock \
     -v /opt/homelab:/opt/homelab \
     -e GITOPS_REPO_URL=https://<GITHUB_USERNAME>:<GITHUB_PAT>@github.com/<org>/homelab.git \
     -e GITOPS_STACK=<stack_name> \
     -e LXC_API_TOKEN=<generated_token> \
     -p 8080:8080 \
     ghcr.io/<org>/lxc-daemon:latest
   ```
5. **Verify daemon is healthy:**
   ```bash
   # Poll until HTTP 200 on /health, max 60s
   for i in $(seq 1 12); do
     curl -sf http://localhost:8080/health && break || sleep 5
   done
   ```

**If any step returns non-zero:** HOST emits `level=error` SSE event, deletes the LXC (`pct destroy <vmid> --purge`), deletes `/opt/appdata/<stack_name>`, and returns `500` to CLIENT.

**Logfmt SSE events emitted by HOST (examples):**
```
data: ts=<ISO8601> level=info component=host phase=bootstrap stack=<stack_name> msg="apt upgrade complete"
data: ts=<ISO8601> level=info component=host phase=bootstrap stack=<stack_name> msg="Docker installed"
data: ts=<ISO8601> level=info component=host phase=bootstrap stack=<stack_name> msg="lxc-daemon container started"
data: ts=<ISO8601> level=info component=host phase=bootstrap stack=<stack_name> msg="lxc-daemon health check passed"
```

---

### Phase 10 — LXC: First GitOps Sync

After the LXC daemon becomes healthy, the CLIENT immediately triggers the first GitOps sync:

**CLIENT sends:**
```
POST http://<lxc_ip>:8080/api/sync
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{
  "force": true,
  "stack": "<stack_name>"
}
```

**LXC daemon actions:**
1. Acquires `/tmp/gitops.lock` (aborts if already locked).
2. Runs `setup.sh` if present at `stacks/<stack_name>/setup.sh` (pre-deploy hook).
3. Performs Git sparse checkout: pulls only `stacks/<stack_name>/` from the repo.
4. Spawns ephemeral secrets container to write `.env` file(s) for each app. Halts if secrets container exits non-zero.
5. For each app directory under `stacks/<stack_name>/`:
   - Runs `docker compose pull -q`.
   - Runs `docker compose up -d --remove-orphans`.
6. Releases `/tmp/gitops.lock`.
7. Emits completion SSE event.

**LXC logfmt events (forwarded to CLIENT SSE stream):**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="sync lock acquired"
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="setup.sh executed"
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="git sparse-checkout complete" sha=<sha>
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=jellyfin msg="docker compose pull complete"
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=jellyfin msg="docker compose up complete"
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="sync complete" apps=<N>
```

---

### Phase 11 — CLIENT: SSH Config Update

After receiving the sync-complete SSE event, the CLIENT:
1. Queries `GET /api/lxc/<vmid>/ip` from HOST to obtain the DHCP-assigned IP.
2. Idempotently writes an SSH alias to `~/.ssh/config`:
   ```
   Host <stack_name>
       HostName <ip>
       User root
       IdentityFile ~/.ssh/id_ed25519
   ```
3. If the alias already exists with a different IP, CLIENT updates the `HostName` line in-place without altering any other block.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=ssh stack=<stack_name> ip=<ip> msg="SSH alias written to ~/.ssh/config"
```

---

### Phase 12 — CLIENT: Wizard Completion

1. The deployment modal transitions to a "Success" state:
   - Green border.
   - Summary table: stack name, VMID, MAC, IP, apps deployed, duration.
   - Reminder banner: *"Register MAC `<hwaddr>` in OPNsense for a permanent static IP."*
2. The Stacks tab refreshes to show the new stack in an `ACTIVE` state.
3. All logfmt events generated during the session are written to `~/.local/share/homelab/logs/<stack_name>-<timestamp>.log`.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=wizard stack=<stack_name> vmid=<vmid> mac=<hwaddr> apps=<N> duration_ms=<ms> msg="stack creation complete"
```

---

## 4. Rollback & Error Handling

| Phase | Failure | Rollback Action |
|---|---|---|
| Phase 6 (YAML validation) | Lint error | No files written; wizard remains open with inline error |
| Phase 7 (Git push) | Push fails | Local files remain; wizard shows error; user can retry or abort |
| Phase 8 (LXC create) | `pct create` fails | HOST deletes NVMe dir; CLIENT shows error modal with raw pct output |
| Phase 9 (Bootstrap) | Any exec step fails | HOST runs `pct destroy --purge`, deletes NVMe dir; CLIENT shows error modal |
| Phase 10 (First sync) | Sync fails | LXC daemon retries up to 3× with exponential backoff; if all fail, CLIENT shows error with last log lines |
| Phase 11 (SSH config) | Parse error | CLIENT rolls back to original `~/.ssh/config` content via in-memory backup taken at phase start |

---

## 5. Idempotency Guarantees

- **CLIENT wizard:** Re-opening the wizard for an existing stack name shows an error inline — it does not overwrite any files.
- **HOST provisioning:** `POST /api/lxc/provision` checks VMID uniqueness before `pct create`. If `/opt/appdata/<stack_name>` already exists, the endpoint returns `409 Conflict`.
- **LXC bootstrap:** `docker run --name lxc-daemon` is idempotent: if the container already exists (e.g. after a partial retry), the daemon checks and removes the stopped container before re-running.
- **SSH config:** The CLIENT SSH module parses the full `~/.ssh/config` AST before writing; it never appends blindly.

---

## 6. Security Constraints

- The `GITHUB_PAT` is **never** passed as a CLI argument (visible in `ps aux`). It is injected via the HOST daemon's encrypted environment store and passed exclusively through `docker run -e` at runtime.
- The `LXC_API_TOKEN` is generated by CLIENT using `rand::random::<[u8; 32]>()`, base64-encoded, and written to `~/.config/homelab/client.toml` with `chmod 600`. It is passed to the LXC daemon via `docker run -e` and never committed to Git.
- All CLIENT ↔ HOST and CLIENT ↔ LXC HTTP calls use TLS (self-signed CA managed by HOST) and Bearer token authentication.
- Generated `docker-compose.yml` files never contain literal secrets; all runtime secrets are injected by the ephemeral secrets container into `.env` files at deploy time.

---

## 7. Data Flow Diagram

```
CLIENT TUI
  │
  ├─[Wizard phases 1-5]──────► Local Git working tree
  │                               stacks/<stack>/lxc-compose.yml
  │                               stacks/<stack>/<app>/docker-compose.yml
  │                               stacks/<stack>/<app>-config/.gitkeep
  │
  ├─[Phase 7] git push ──────► GitHub (main branch)
  │
  ├─[Phase 8] POST /api/lxc/provision ──────► HOST daemon
  │                                              │
  │                                              ├─ pct create
  │                                              ├─ pct set (mounts)
  │                                              ├─ pct start
  │                                              └─ bootstrap exec (Phase 9)
  │                                                   apt upgrade
  │                                                   Docker install
  │                                                   lxc-daemon start
  │
  ├─[Phase 10] POST /api/sync ──────────────► LXC daemon (:8080)
  │                                              │
  │                                              ├─ setup.sh hook
  │                                              ├─ git sparse-checkout
  │                                              ├─ ephemeral secrets container
  │                                              └─ docker compose up (per app)
  │
  ◄──────────── SSE /api/events/stream ──────── HOST & LXC (live logfmt events)
  │
  └─[Phase 11] GET /api/lxc/<vmid>/ip ──────► HOST daemon
                                               └─ ~/.ssh/config updated
```

---

## 8. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `activate-stack.md` | Mark an existing stack as active without full re-provisioning |
| `add-app-to-stack.md` | Add a new app to an already-provisioned stack |
| `add-core-app-to-stack.md` | Add Promtail/Watchtower/Traefik to an existing stack |
| `filesystem-layouts.md` | Canonical directory layout rules enforced during scaffold |
| `pre-sync-hooks.md` | Details on `setup.sh` hook execution |
| `tui-deployment-modal-progress.md` | Ratatui modal implementation for live SSE log display |
| `error-handling-fail-closed.md` | Fail-closed rules applied in Phases 8 and 9 |
| `transactional-actions.md` | Rollback steps if provisioning fails mid-phase |
| `bind-mounts.md` | How unprivileged bind mounts are configured on the HOST |
| `unattended-upgrades.md` | OS patching setup performed during Phase 9 bootstrap |
