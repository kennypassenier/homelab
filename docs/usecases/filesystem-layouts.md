# Use Case: Filesystem Layouts

**Tier:** CLIENT (enforces during scaffold) → HOST (creates and owns host directories) → LXC (validates paths at runtime)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

This document defines the **canonical, authoritative** directory structure for every tier of the homelab system. All scaffolding, provisioning, and runtime validation code must derive its paths from these layouts. Deviations are considered bugs.

---

## 2. Git Repository Layout

```
homelab/                              ← Git repository root
  stacks/
    <stack_name>/
      lxc-compose.yml                 ← Declarative LXC spec (CLIENT-generated)
      setup.sh                        ← Pre-deploy hook (optional, chmod +x)
      <app_name>/
        docker-compose.yml            ← App service definition (CLIENT-generated)
      <app_name>-config/
        .gitkeep                      ← Placeholder; actual data lives on host NVMe
      promtail/
        docker-compose.yml
      promtail-config/
        config.yml
        .gitkeep
      watchtower/
        docker-compose.yml
      traefik/                        ← Only in one stack per Proxmox node
        docker-compose.yml
      traefik-config/
        traefik.yml
        acme/
          .gitkeep
  docs/
    usecases/                         ← Atomic use case specifications
    architecture.md
    client-features.md
    host-features.md
    lxc-features.md
    LLM_CONTEXT.md
  client-app/                         ← Rust source: CLIENT TUI
  host-daemon/                        ← Rust source: HOST daemon
  lxc-daemon/                         ← Rust source: LXC daemon (packaged as Docker image)
  .github/
    workflows/
      client.yml
      host.yml
      lxc.yml
```

**Rules:**
- `<app_name>` directories contain **only** GitOps-managed files (compose, scripts, config templates).
- `<app_name>-config` directories contain **only** `.gitkeep`; all actual runtime data is on the host NVMe.
- No secrets, `.env` files, or runtime state are ever committed to Git.

---

## 3. Proxmox Host Filesystem Layout

```
/opt/appdata/                         ← Root of all stack appdata (fast NVMe SSD)
  <stack_name>/                       ← Created by HOST daemon on provisioning
    <app_name>-config/                ← Persistent config/database data for the app
      ...                             ← App runtime data (DB files, config, certs)
    promtail-config/
      config.yml
      positions.yaml
    traefik-config/
      traefik.yml
      acme/
        acme.json                     ← Let's Encrypt certificates (chmod 600)
    watchtower-config/                ← (Watchtower has no persistent state; dir may be empty)

/mnt/                                 ← Media storage (spinning disk arrays — NOT backed up)
  data/
    18TB/                             ← 18TB array: media library (movies, shows)
    12TB/                             ← 12TB array: archive / overflow
  downloads/                          ← Download staging area (qBittorrent output)
```

**Ownership rules:**
- `/opt/appdata/<stack_name>/` and all subdirectories: owned by `100000:100000` (maps to root inside the unprivileged LXC).
- `/mnt/data/` and `/mnt/downloads/`: owned by the system user/group that manages the media server; Docker containers access via group permissions.
- HOST daemon never writes inside `/mnt/data/` or `/mnt/downloads/`; those paths are managed exclusively by apps inside the LXC.

**HOST daemon creation sequence (on stack provisioning):**
```rust
let stack_appdata = format!("/opt/appdata/{}", stack_name);
fs::create_dir_all(&stack_appdata)?;
// Create per-app config directories from lxc-compose.yml mounts
for app in &apps {
    let app_config = format!("{}/{}-config", stack_appdata, app.name);
    fs::create_dir_all(&app_config)?;
    // Set ownership to UID 100000 (unprivileged LXC root mapping)
    chown(&app_config, Some(100000), Some(100000))?;
}
```

---

## 4. LXC Container Filesystem Layout

```
/                                     ← LXC root filesystem (ephemeral; on local-lvm)
  appdata/                            ← Bind-mounted from /opt/appdata/<stack_name>
    <app_name>-config/                ← Accessed by Docker as /appdata/<app_name>-config
    promtail-config/
    traefik-config/
  mnt/                                ← Media mounts (if declared in lxc-compose.yml)
    data/
      18TB/
      12TB/
    downloads/
  opt/
    homelab/                          ← Git sparse checkout root
      stacks/
        <stack_name>/                 ← Only this stack's directory is checked out
          <app_name>/
            docker-compose.yml
          setup.sh
  var/
    run/
      docker.sock                     ← Docker daemon socket
  run/
    lxc-daemon/
      gitops.lock                     ← Sync lock file (prevents concurrent syncs)
      secrets/
        .env                          ← Written by ephemeral secrets container (chmod 600)
```

**LXC filesystem rules:**
- The LXC root filesystem is **ephemeral** — never used for persistent storage. All app data references `/appdata/`.
- `/opt/homelab` is the Git sparse checkout working directory.
- `/run/lxc-daemon/secrets/.env` is recreated on every sync cycle by the ephemeral secrets container; it is never persisted.
- The Git sparse checkout at `/opt/homelab` only contains `stacks/<stack_name>/` — no other stacks' files are ever present.

---

## 5. Docker Volume Path Conventions

All generated `docker-compose.yml` files follow this volume binding convention:

```yaml
volumes:
  # Config/data — bind from host NVMe via LXC appdata mount
  - /appdata/<app_name>-config:/config

  # Media — bind from spinning disk (media stacks only)
  - /mnt/data/18TB:/media
  - /mnt/downloads:/downloads

  # Docker socket — only for Watchtower, Promtail, Traefik
  - /var/run/docker.sock:/var/run/docker.sock:ro
```

**Prohibited patterns:**
```yaml
# WRONG — do not use named volumes for persistent data
volumes:
  app_data: {}
services:
  app:
    volumes:
      - app_data:/config

# WRONG — do not reference /opt/appdata directly in compose files
volumes:
  - /opt/appdata/media/jellyfin-config:/config
```

The only correct pattern is to reference the bind-mounted path inside the LXC: `/appdata/<app_name>-config:/config`.

---

## 6. CLIENT Scaffold Validation Rules

During `add-stack.md` Phase 6 and `add-app-to-stack.md` Phase 2, the CLIENT enforces these layout rules before writing any files:

| Rule | Check |
|---|---|
| App name must not collide with its config directory name | `<app_name>` ≠ `<app_name>-config` (always true by convention) |
| `<app_name>-config/` must always be created alongside `<app_name>/` | Both directories scaffolded together |
| Volume binds must use `/appdata/` prefix | Regex check on all `volumes:` entries |
| Named volumes are forbidden | Presence of top-level `volumes:` block rejected |
| `lxc-compose.yml` must declare the appdata mount as `mp0` | First mount entry validated |
| Mount point IDs must be sequential (no gaps) | `mp0, mp1, mp2...` with no skipped numbers |

---

## 7. LXC Daemon Startup Validation

On daemon startup, the LXC daemon validates the filesystem layout before accepting any API requests:

1. Assert `/appdata` is a real bind mount (device ID differs from `/`).
2. Assert `/opt/homelab` exists and is a valid Git repository.
3. Assert `/run/lxc-daemon/` is writable (create if not exists).
4. Assert Docker socket at `/var/run/docker.sock` is accessible.

Any assertion failure emits `level=error` and causes the daemon to exit with a non-zero code, triggering a Docker container restart. The daemon **never starts in a degraded state**.

**Logfmt events:**
```
ts=<ISO8601> level=info component=lxc msg="filesystem layout validated"
ts=<ISO8601> level=error component=lxc msg="startup validation failed" reason="appdata not bound"
```

---

## 8. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `bind-mounts.md` | How the paths declared here are mounted into the LXC |
| `add-stack.md` | Scaffold creates all directories in this layout |
| `error-handling-fail-closed.md` | Daemon exits on layout validation failure |
| `unattended-upgrades.md` | Does not affect filesystem layout |
| `manual-backup-all.md` | Restic backs up `/opt/appdata/`; excludes `/mnt/` |
