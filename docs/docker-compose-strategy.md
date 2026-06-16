# Docker Compose Strategy

Last updated: 2026-06-16

This document defines the canonical Docker Compose strategy for this repository and how to convert legacy compose files into the current format.

## 1. Scope and model

- GitOps-first: compose files are authored in this repo and applied by the LXC daemon sync flow.
- One app per directory: each app lives under `stacks/<stack>/<app>/docker-compose.yml`.
- Stack infrastructure apps are separate app directories:
  - `promtail`
  - `watchtower`
  - `traefik`
- Do not run sidecar Promtail inside normal app compose files.

## 2. Directory and data strategy

### 2.1 Repository layout

- Stack root: `stacks/<stack>/`
- App root: `stacks/<stack>/<app>/`
- App compose: `stacks/<stack>/<app>/docker-compose.yml`

### 2.2 Config mount strategy (`/appdata/<stack>/<app>-config`)

App compose files must use stack-scoped absolute paths:

- `/appdata/<stack>/<app>-config:/config`
- Additional app-specific subpaths as needed, always under the same app-config root.
  For Vikunja:
  - `/appdata/<stack>/vikunja-config/files:/app/vikunja/files`
  - `/appdata/<stack>/vikunja-config/db:/db`

For infra config-only paths (for example Promtail), use stack-scoped config path:

- `/appdata/<stack>/promtail-config/config.yml:/etc/promtail/config.yml:ro`

## 3. App compose baseline

Default baseline for non-infra apps:

```yaml
services:
  <app>:
    image: <image>:latest
    container_name: <app>
    env_file:
      - .env
    environment:
      - TZ=Europe/Brussels
    restart: unless-stopped
    volumes:
      - /appdata/<stack>/<app>-config:/config
    labels:
      - "com.centurylinklabs.watchtower.enable=true"
      - "com.homelab.backup.pause=true"
```

Notes:

- Do not include `version:` key (Compose v2).
- Do not include explicit `networks:` block in default generated app compose.
- Use `:latest` tags; updates are controlled by Watchtower label gating.

## 4. Labels policy

### 4.1 Normal app containers

Required labels:

- `com.centurylinklabs.watchtower.enable=true`
- `com.homelab.backup.pause=true`

### 4.2 Infrastructure containers

Infrastructure services (`watchtower`, `promtail`, `traefik`) must include:

- `com.centurylinklabs.watchtower.enable=true`

`com.homelab.backup.pause=true` is for stateful app containers and is not required on infra services.

## 5. Traefik strategy

Traefik is a dedicated app directory under each stack (`stacks/<stack>/traefik/`).

For app routing labels in normal app compose files:

- Enable Traefik:
  - `traefik.enable=true`
- Router rule:
  - `traefik.http.routers.<app>.rule=Host("<subdomain>.<DOMAIN>")`
- Service port:
  - `traefik.http.services.<app>.loadbalancer.server.port=80`

If no subdomain is provided in wizard flow, fallback rule is:

- `Host("<app>.local")`

## 6. Watchtower strategy

Watchtower runs as a stack core app in `stacks/<stack>/watchtower/docker-compose.yml`.

Current watchtower behavior flags:

- `WATCHTOWER_LABEL_ENABLE=true`
- `WATCHTOWER_CLEANUP=true`
- `WATCHTOWER_POLL_INTERVAL=86400`
- `WATCHTOWER_ROLLING_RESTART=true`

Because `WATCHTOWER_LABEL_ENABLE=true`, only containers with `com.centurylinklabs.watchtower.enable=true` are updated.

## 7. Promtail strategy

Promtail runs as a separate stack app in `stacks/<stack>/promtail/docker-compose.yml`.

Important rules:

- Do not embed `promtail` service in any normal app compose.
- Promtail config lives in `stacks/<stack>/promtail-config/config.yml`.
- Promtail runtime `.env` is sourced via stack secret flow (latch/sync strategy).

## 8. Secrets and env strategy

- Sensitive values go to app `.env` files (not hardcoded in compose).
- Keep stable non-secret defaults in `environment:` when appropriate.
- `env_file: .env` should be present on app services by default.

## 9. Optional feature wiring

### 9.1 GPU passthrough

When GPU is enabled for an app, compose is augmented with:

- `devices` entries for Intel nodes (`/dev/dri/renderD128`, `/dev/dri/card0`)
- `group_add` entries (`104`, `44`)
- For Jellyfin, `DOCKER_MODS=linuxserver/mods:jellyfin-opencl-intel`

Also a host hint is written in stack `lxc-compose.yml` under `hardware.gpu`.

### 9.2 VPN/Gluetun

If an app uses VPN namespace wiring, keep VPN metadata and labels explicit in that app compose. Do not merge this into global defaults; apply per app that actually needs VPN isolation.

## 10. Conversion guide for legacy compose files

Use this checklist when converting old compose files:

1. Move each service into `stacks/<stack>/<app>/docker-compose.yml`.
2. Remove top-level `version:` key.
3. Ensure image uses valid upstream or verified LSIO image, with `:latest`.
4. Ensure app service includes:
   - `container_name`
   - `restart: unless-stopped`
   - `env_file: .env`
   - `TZ=Europe/Brussels`
5. Convert config mounts to `/appdata/<stack>/<app>-config:/config` (or app-specific target path under the same app-config root).
6. Add required app labels:
   - `com.centurylinklabs.watchtower.enable=true`
   - `com.homelab.backup.pause=true`
7. Remove embedded `promtail` service from app compose files.
8. Remove explicit `networks:` block unless there is a specific non-default network requirement.
9. Add Traefik labels only for services that should be externally routed.
10. Put watchtower/promtail/traefik in their own stack app directories if missing.

## 11. Canonical minimal examples

### 11.1 App with Traefik

```yaml
services:
  vikunja:
    image: vikunja/vikunja:latest
    container_name: vikunja
    env_file:
      - .env
    environment:
      - TZ=Europe/Brussels
    restart: unless-stopped
    volumes:
      - /appdata/todo/vikunja-config:/config
      - /appdata/todo/vikunja-config/files:/app/vikunja/files
      - /appdata/todo/vikunja-config/db:/db
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
    restart: unless-stopped
    environment:
      - TZ=Europe/Brussels
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - /var/lib/docker/containers:/var/lib/docker/containers:ro
      - /appdata/<stack>/promtail-config/config.yml:/etc/promtail/config.yml:ro
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
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      WATCHTOWER_LABEL_ENABLE: "true"
      WATCHTOWER_CLEANUP: "true"
      WATCHTOWER_POLL_INTERVAL: "86400"
      WATCHTOWER_ROLLING_RESTART: "true"
    labels:
      com.centurylinklabs.watchtower.enable: "true"
```
