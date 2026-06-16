# Docker Compose Strategy

Last updated: 2026-06-16

This document defines the canonical Docker Compose strategy for this repository and explains how the LXC daemon sync pipeline handles volumes, permissions, and secrets at runtime.

---

## 1. Scope and model

- GitOps-first: compose files are authored in this repo and applied by the LXC daemon sync flow.
- One app per directory: each app lives under `stacks/<stack>/<app>/docker-compose.yml`.
- Stack infrastructure apps are separate app directories: `promtail`, `watchtower`, `traefik`.
- Do not run a sidecar Promtail inside normal app compose files.
- Never use `version:` key (Compose v2+).

---

## 2. Directory and volume strategy

### 2.1 Repository layout

```
stacks/<stack>/                     ← stack root (sparse-checked out in LXC at /opt/gitops/stacks/<stack>/)
stacks/<stack>/<app>/               ← app root
stacks/<stack>/<app>/docker-compose.yml
stacks/<stack>/<app>-config/        ← app config and data directory (also in git)
```

### 2.2 Volume path convention

**All bind-mount sources use the git checkout path directly:**

```
/opt/gitops/stacks/<stack>/<app>-config
```

This path always exists inside the LXC because the sparse checkout already brought it in from git. There is no separate `/appdata` layer.

Examples:
- `/opt/gitops/stacks/todo/vikunja-config:/config`
- `/opt/gitops/stacks/todo/vikunja-config/files:/app/vikunja/files`
- `/opt/gitops/stacks/todo/traefik-config:/etc/traefik`
- `/opt/gitops/stacks/todo/promtail-config/config.yml:/etc/promtail/config.yml:ro`

**Do NOT use `/appdata/...` paths** — that convention has been retired.

### 2.3 Read-only vs writable mounts

| Annotation | When to use | LXC prep behaviour |
|---|---|---|
| `:ro` | Config files sourced from git (promtail config, traefik static config) | Skipped entirely — git checkout already owns the file |
| _(no flag)_ | Data directories the container writes to at runtime | `mkdir -p` + `chown UID:GID` if `user:` is set on the service |

### 2.4 The `user:` field and automatic chown (step 5)

The LXC daemon sync **step 5** ("Prepare bind-mounted files from compose manifests") scans every `docker-compose.yml` before `docker compose up`. For each writable bind-mount directory it:

1. Runs `mkdir -p <source-path>` if the directory does not exist.
2. If the service declares `user: "UID:GID"`, runs `chown UID:GID <source-path>`.

**Rules:**

- **Set `user: "UID:GID"`** when the container process runs as a non-root user **and** writes to its bind-mounted directories. Example: Vikunja runs as uid 1000 and writes to `files/` and `db/`.
- **Omit `user:`** when the container runs as root (traefik, watchtower, promtail, cloudflared). Root-owned directories are created by prep without any chown.
- **`:ro` mounts are always skipped** — they come from the git checkout and are never chowned.
- System paths (`/var/run/docker.sock`, `/var/lib/docker/containers`, `/proc`, `/sys`, `/dev`) are never touched by prep.

```yaml
# Example: non-root container — set user:
services:
  vikunja:
    user: "1000:1000"   # ← prep will chown vikunja-config/files and vikunja-config/db
    volumes:
      - /opt/gitops/stacks/todo/vikunja-config/files:/app/vikunja/files  # writable → mkdir + chown
      - /opt/gitops/stacks/todo/vikunja-config/db:/db                    # writable → mkdir + chown

# Example: root container — omit user:
services:
  traefik:
    # no user: → prep creates dirs as root, which is correct
    volumes:
      - /opt/gitops/stacks/todo/traefik-config:/etc/traefik              # writable → mkdir only
      - /opt/gitops/stacks/todo/traefik-config/acme:/acme                # writable → mkdir only
      - /var/run/docker.sock:/var/run/docker.sock:ro                     # system   → skipped
```

---

## 3. App compose baseline

Default generated template for a non-infrastructure app:

```yaml
services:
  <app>:
    # Use :latest — Watchtower handles rolling updates automatically.
    # Pin to a specific tag only when you need to lock the version.
    image: <image>:latest

    # container_name is set explicitly so logs and docker ps output are readable.
    container_name: <app>

    # user: "UID:GID"
    # Set this only when the container runs as a non-root user and writes to
    # its bind-mounted volumes. The LXC daemon reads this field and runs
    # chown UID:GID on every writable bind mount before docker compose up.
    # Omit entirely for containers that run as root (traefik, watchtower, etc.).

    # env_file is populated at runtime by the latch secret sync step.
    # Never commit real credentials — use .env.example for documentation.
    env_file:
      - .env

    environment:
      # TZ must match your Proxmox host timezone to avoid log timestamp skew.
      - TZ=Europe/Brussels

    # unless-stopped: restart on crash but respect manual docker stop.
    restart: unless-stopped

    volumes:
      # Config directory — bind-mounted from the git checkout.
      # LXC prep (step 5) creates this dir if missing, and chowns it if user: is set.
      - /opt/gitops/stacks/<stack>/<app>-config:/config

    labels:
      # Watchtower only auto-updates containers that carry this label.
      # Controlled by WATCHTOWER_LABEL_ENABLE=true in the watchtower compose.
      - "com.centurylinklabs.watchtower.enable=true"
      # Backup pause: the backup agent stops this container before snapshotting
      # its bind-mount directories to avoid partial writes in the backup.
      - "com.homelab.backup.pause=true"
```

---

## 4. Labels policy

### 4.1 Normal app containers (required)

- `com.centurylinklabs.watchtower.enable=true`
- `com.homelab.backup.pause=true`

### 4.2 Infrastructure containers (watchtower, promtail, traefik)

- `com.centurylinklabs.watchtower.enable=true` only — these are not stateful apps and do not need the backup pause label.

### 4.3 Traefik routing labels (optional, per app)

Only add these when the app should be externally routed through Traefik:

```yaml
- "traefik.enable=true"
- "traefik.http.routers.<app>.rule=Host(\"<subdomain>.<domain>\")"
- "traefik.http.services.<app>.loadbalancer.server.port=<port>"
```

---

## 5. Traefik strategy

Traefik lives in `stacks/<stack>/traefik/docker-compose.yml`. It reads Docker labels from other containers and builds routing rules dynamically — no static config changes are needed when adding or removing app services.

Config files live in `stacks/<stack>/traefik-config/` (in git). The `acme/` subdirectory for Let's Encrypt certificates is a writable directory created by the LXC prep step (root-owned, no chown needed).

---

## 6. Watchtower strategy

Watchtower lives in `stacks/<stack>/watchtower/docker-compose.yml`. Every new stack gets it automatically.

| Variable | Value | Meaning |
|---|---|---|
| `WATCHTOWER_LABEL_ENABLE` | `true` | Only update labelled containers |
| `WATCHTOWER_CLEANUP` | `true` | Remove old images after update |
| `WATCHTOWER_POLL_INTERVAL` | `86400` | Check every 24 hours |
| `WATCHTOWER_ROLLING_RESTART` | `true` | Restart one container at a time |

---

## 7. Promtail strategy

Promtail lives in `stacks/<stack>/promtail/docker-compose.yml`. Its static config lives in `stacks/<stack>/promtail-config/config.yml` (in git, mounted `:ro`).

- Do **not** embed a `promtail` service in any normal app compose.
- Promtail runtime `.env` (providing `LOKI_URL`) is sourced via the latch secret sync step.
- The config file is mounted `:ro` — the LXC prep step does **not** create or chown it; it already exists from the git checkout.

---

## 8. Secrets and env strategy

- Sensitive values go in app `.env` files (never hardcoded in compose).
- Keep stable non-secret defaults in `environment:` when appropriate.
- `env_file: .env` should be present on all app services.
- `.env` files are populated at runtime by `latch pull` during sync step 4. Only committed `.env.example` files belong in git.

---

## 9. Optional feature wiring

### 9.1 GPU passthrough

When GPU is enabled for an app, compose is augmented with:

```yaml
devices:
  - /dev/dri/renderD128
  - /dev/dri/card0
group_add:
  - "104"
  - "44"
```

A host hint is also written in `lxc-compose.yml` under `hardware.gpu`.

### 9.2 VPN/Gluetun

If an app uses VPN namespace wiring, keep VPN metadata and labels explicit in that app compose. Do not merge into global defaults; apply per app that actually needs VPN isolation.

---

## 10. Conversion guide for legacy compose files

1. Move each service into `stacks/<stack>/<app>/docker-compose.yml`.
2. Remove top-level `version:` key.
3. Ensure image uses valid upstream tag (`:latest` or pinned).
4. Ensure service includes: `container_name`, `restart: unless-stopped`, `env_file: .env`, `TZ=Europe/Brussels`.
5. Convert all bind-mount sources to `/opt/gitops/stacks/<stack>/<app>-config/...` — no `/appdata/` paths.
6. Add `:ro` to config file mounts that come from git and should never be written at runtime.
7. If the container runs as a non-root user, add `user: "UID:GID"` — prep will chown writable mounts.
8. Add required labels: `com.centurylinklabs.watchtower.enable=true` + `com.homelab.backup.pause=true` for app containers.
9. Remove embedded `promtail` service from app compose files; it lives in its own directory.
10. Remove explicit `networks:` block unless there is a non-default network requirement.
11. Put watchtower/promtail/traefik in their own stack app directories if missing.
12. Do NOT add `setup.sh` or `pre-sync.sh` for routine storage bootstrap — step 5 handles that automatically.

---

## 11. Canonical examples

### 11.1 Normal app with Traefik (Vikunja)

```yaml
services:
  vikunja:
    image: vikunja/vikunja:latest
    container_name: vikunja
    # Vikunja runs as uid 1000. Prep chowns vikunja-config/files and db to 1000:1000.
    user: "1000:1000"
    env_file:
      - .env
    environment:
      - TZ=Europe/Brussels
    restart: unless-stopped
    ports:
      - "3456:3456"
    volumes:
      # All writable — prep creates and chowns each dir to 1000:1000 before compose up.
      - /opt/gitops/stacks/todo/vikunja-config:/config
      - /opt/gitops/stacks/todo/vikunja-config/files:/app/vikunja/files
      - /opt/gitops/stacks/todo/vikunja-config/db:/db
    labels:
      - "com.centurylinklabs.watchtower.enable=true"
      - "com.homelab.backup.pause=true"
      - "traefik.enable=true"
      - "traefik.http.routers.vikunja.rule=Host(\"todo.example.com\")"
      - "traefik.http.services.vikunja.loadbalancer.server.port=3456"
```

### 11.2 Promtail (separate app directory)

```yaml
services:
  promtail:
    image: grafana/promtail:latest
    container_name: <stack>-promtail
    # No user: — runs as root to read /var/lib/docker/containers.
    environment:
      - TZ=Europe/Brussels
      - DOCKER_API_VERSION=1.40
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro                              # system — prep skips
      - /var/lib/docker/containers:/var/lib/docker/containers:ro                  # system — prep skips
      - /opt/gitops/stacks/<stack>/promtail-config/config.yml:/etc/promtail/config.yml:ro  # :ro — prep skips
    env_file:
      - .env
    command: -config.file=/etc/promtail/config.yml -config.expand-env=true
    labels:
      - "com.centurylinklabs.watchtower.enable=true"
```

### 11.3 Watchtower (separate app directory)

```yaml
services:
  watchtower:
    image: containrrr/watchtower:latest
    container_name: <stack>-watchtower
    # No user: — must run as root to control the Docker daemon.
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock  # read-write required — no :ro
    environment:
      DOCKER_API_VERSION: "1.40"
      WATCHTOWER_LABEL_ENABLE: "true"
      WATCHTOWER_CLEANUP: "true"
      WATCHTOWER_POLL_INTERVAL: "86400"
      WATCHTOWER_ROLLING_RESTART: "true"
    labels:
      com.centurylinklabs.watchtower.enable: "true"
```

### 11.4 Traefik (separate app directory)

```yaml
services:
  traefik:
    image: traefik:v3
    container_name: <stack>-traefik
    # No user: — must run as root to bind ports 80/443.
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro                     # :ro — prep skips
      - /opt/gitops/stacks/<stack>/traefik-config:/etc/traefik           # writable — prep mkdir (root)
      - /opt/gitops/stacks/<stack>/traefik-config/acme:/acme            # writable — prep mkdir (root)
    environment:
      DOCKER_API_VERSION: "1.40"
    labels:
      com.centurylinklabs.watchtower.enable: "true"
      traefik.enable: "true"
```
