---
description: "Use when creating, reviewing, or editing docker-compose.yml files in the homelab project. Enforces project-wide conventions for images, volumes, labels, secrets, and networking."
applyTo: "**/docker-compose.yml"
---

# Docker Compose Conventions — Homelab

## Images

- **Never use `lscr.io/linuxserver/<app>` without first verifying the image exists** at `https://fleet.linuxserver.io/image?name=linuxserver/<app>`. LSIO does not have an image for every app.
- When an LSIO image does not exist, use the official upstream image (e.g. `vikunja/vikunja`).
- Always use the `:latest` tag — Watchtower handles updates.
- Do **not** include a `version:` top-level key — it is obsolete in Compose v2.

## Volumes — Storage Layout

All persistent data **must** follow this absolute path pattern:

```
/appdata/<STACK_NAME>/<APP_NAME>/<subfolder>
```

- `<STACK_NAME>` matches the folder name under `stacks/` (e.g. `media`, `vikunja`).
- `<APP_NAME>` matches the app's folder name under the stack (e.g. `jellyfin`, `vikunja`).
- `<subfolder>` is typically `config`, `data`, or another descriptive name.
- **Never use relative paths** (e.g. `./files`). Always use absolute `/appdata/...` paths.
- This ensures all data lands under `/opt/appdata/<STACK>/<APP>` on the Proxmox host and is covered by Restic backups in a single pass.

Examples:
```yaml
# Correct
- /appdata/media/jellyfin/config:/config
- /appdata/vikunja/vikunja/config:/app/vikunja/files

# Wrong — relative path
- ./files:/app/data

# Wrong — missing stack segment
- /appdata/jellyfin/config:/config
```

## Labels — Required on Every App Container

Both labels are **mandatory** on every non-infrastructure service container:

```yaml
labels:
  - "com.centurylinklabs.watchtower.enable=true"   # Opt into Watchtower auto-updates
  - "com.homelab.backup.pause=true"                # Pause during Restic backups to prevent DB corruption
```

Watchtower and Promtail containers are infrastructure — they use `com.centurylinklabs.watchtower.enable=true` only.

## Secrets and Environment Variables

- **Never hardcode secrets, tokens, passwords, or URLs** directly in `environment:`.
- Always use `env_file: .env` for any value that is instance-specific or sensitive. The `.env` file is SOPS-encrypted transparently via Git smudge/clean filters.
- Static, non-secret configuration (e.g. `VIKUNJA_DATABASE_TYPE=sqlite`, `PUID=1000`) may live directly in `environment:`.
- The `.env` file should contain: public URL, service secrets, API keys, IP addresses used in configs.

## PUID / PGID

- Only include `PUID=1000` and `PGID=1000` for **LSIO images** (`lscr.io/linuxserver/*`).
- Official upstream images do not use PUID/PGID — omit them to avoid confusion.

## Standard Fields

Every app service must include:

```yaml
container_name: <app_name>      # Explicit, matches the app folder name
restart: unless-stopped
environment:
  - TZ=Europe/Brussels           # Always set timezone
```

## Ports

- Only expose ports that are directly needed (e.g. for Nginx Proxy Manager to proxy to).
- Do not expose ports that are only used for internal container-to-container communication — use Docker networks instead.

## Checklist Before Finalising a Compose File

- [ ] Image exists and is correct (verify LSIO images before use)
- [ ] No `version:` key at the top
- [ ] All volumes use `/appdata/<stack>/<app>/<subfolder>` — no relative paths
- [ ] Both labels present on every app container
- [ ] Secrets are in `env_file: .env`, not hardcoded
- [ ] `PUID`/`PGID` only present on LSIO images
- [ ] `TZ=Europe/Brussels`, `container_name`, and `restart: unless-stopped` set
