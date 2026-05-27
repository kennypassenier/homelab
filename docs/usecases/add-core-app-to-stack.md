# Use Case: Add Core App to Stack

**Tier:** CLIENT (scaffold + detection) → LXC (sync and deploy)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Core apps are mandatory infrastructure services that every stack should include:

| Core App | Role |
|---|---|
| `promtail` | Log aggregation — ships Docker logs to centralized Loki |
| `watchtower` | Automatic image updates for all labelled containers |
| `traefik` | Reverse proxy with CrowdSec L7 Bouncer middleware |

This use case handles adding one or more of these core apps to an existing stack that is **missing** them. It is the targeted complement to the automatic injection that occurs during `add-stack.md` Phase 4.

The CLIENT detects which core apps are absent, scaffolds only the missing ones with correct pre-configured templates, and triggers an immediate sync if the stack is `ACTIVE`.

---

## 2. Core App Templates

### Promtail Template

Generated at `stacks/<stack_name>/promtail/docker-compose.yml`:

```yaml
services:
  promtail:
    image: grafana/promtail:latest
    container_name: <stack_name>-promtail
    restart: unless-stopped
    volumes:
      - /var/log:/var/log:ro
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ../promtail-config:/etc/promtail
    env_file:
      - .env
    command: -config.file=/etc/promtail/config.yml -config.expand-env=true
    labels:
      com.centurylinklabs.watchtower.enable: "true"
```

Generated config at `stacks/<stack_name>/promtail-config/config.yml`:

```yaml
server:
  http_listen_port: 9080
  grpc_listen_port: 0

positions:
  filename: /tmp/positions.yaml

clients:
  - url: ${LOKI_URL}/loki/api/v1/push

scrape_configs:
  - job_name: docker
    static_configs:
      - targets: [localhost]
        labels:
          job: docker
          stack: <stack_name>
          __path__: /var/log/docker/*.log
    pipeline_stages:
      - docker: {}
      - labels:
          stream:
          level:

  - job_name: lxc_daemon
    static_configs:
      - targets: [localhost]
        labels:
          job: lxc_daemon
          stack: <stack_name>
          __path__: /var/log/lxc-daemon.log
    pipeline_stages:
      - logfmt: {}
      - labels:
          level:
          app:
      - timestamp:
          source: ts
          format: RFC3339Nano
```

### Watchtower Template

Generated at `stacks/<stack_name>/watchtower/docker-compose.yml`:

```yaml
services:
  watchtower:
    image: containrrr/watchtower:latest
    container_name: <stack_name>-watchtower
    restart: unless-stopped
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    environment:
      WATCHTOWER_LABEL_ENABLE: "true"
      WATCHTOWER_CLEANUP: "true"
      WATCHTOWER_SCHEDULE: "0 3 * * *"
      WATCHTOWER_ROLLING_RESTART: "true"
    labels:
      com.centurylinklabs.watchtower.enable: "true"
```

### Traefik Template

Generated at `stacks/<stack_name>/traefik/docker-compose.yml`:

```yaml
services:
  traefik:
    image: traefik:v3
    container_name: <stack_name>-traefik
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ../traefik-config:/etc/traefik
      - ../traefik-config/acme:/acme
    labels:
      com.centurylinklabs.watchtower.enable: "true"
      traefik.enable: "true"
```

Generated static config at `stacks/<stack_name>/traefik-config/traefik.yml` with:
- Docker provider enabled.
- Let's Encrypt TLS resolver configured with `${ACME_EMAIL}`.
- CrowdSec Bouncer plugin declared.

> **Note:** Only one Traefik instance should exist per Proxmox node. Before scaffolding Traefik, CLIENT checks all stacks on the same HOST for an existing Traefik instance and warns the user if one is found.

---

## 3. Step-by-Step Flow

### Phase 1 — CLIENT: Detect Missing Core Apps

**Trigger:** User selects a stack and presses `c` (core apps), or selects "Add Core App" from the context menu.

**Actions:**
1. CLIENT scans `stacks/<stack_name>/` for directories named `promtail`, `watchtower`, `traefik`.
2. Missing ones are listed in a multiselect modal:
   ```
   Add Core Apps to Stack: <stack_name>

   [x] Promtail     — log aggregation (not present)
   [x] Watchtower   — image auto-updates (not present)
   [ ] Traefik      — already present in stack: gateway
   
   [ Cancel ]  [ Add Selected ]
   ```
3. Already-present core apps are shown but disabled (checked, greyed out).
4. If a Traefik instance is found on the same HOST in a different stack, Traefik is shown with a note: "Already deployed in stack: gateway. Adding a second instance is not recommended."

---

### Phase 2 — CLIENT: Scaffold Selected Core Apps

For each selected core app:
1. Generate `docker-compose.yml` from the template above.
2. Generate config directory and files (e.g., `promtail-config/config.yml`).
3. Create `<app>-config/.gitkeep`.
4. Pre-flight lint.

**Logfmt emitted by CLIENT:**
```
ts=<ISO8601> level=info component=scaffold stack=<stack_name> app=promtail msg="core app scaffolded"
ts=<ISO8601> level=info component=scaffold stack=<stack_name> app=watchtower msg="core app scaffolded"
```

---

### Phase 3 — CLIENT: Git Commit and Push

1. Stage all new files.
2. Commit: `feat(scaffold): add core apps [promtail, watchtower] to stack <stack_name>`.
3. Push to `main`.

---

### Phase 4 — Conditional: CLIENT → LXC Sync (if stack is ACTIVE)

```
POST http://<lxc_ip>:8080/api/sync
Authorization: Bearer <lxc_api_token>
Content-Type: application/json

{ "force": true, "stack": "<stack_name>" }
```

LXC detects the new core app directories and deploys them.

---

### Phase 5 — CLIENT: Completion

Toast: "Core apps added to stack <stack_name>: promtail, watchtower."

---

## 4. Runtime Configuration Requirements

After adding Promtail, the following secret must be present in the ephemeral secrets container configuration:
- `LOKI_URL` — URL of the centralized Loki instance (e.g., `http://10.0.1.100:3100`).

If this secret is missing, the LXC daemon's ephemeral secrets step will fail and deployment will halt (fail-closed). The CLIENT warns the user with an amber notice if `LOKI_URL` is not configured in the secrets vault.

---

## 5. Idempotency

- Re-running this flow when all core apps are already present results in no file writes and no Git commit.
- Syncing an already-running Promtail/Watchtower is a `docker compose up -d` no-op.

---

## 6. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `delete-core-app-from-stack.md` | Inverse: remove a core app safely |
| `add-stack.md` | Core app injection at stack creation time (Phase 4) |
| `add-app-to-stack.md` | General app addition flow |
| `error-warning-logging.md` | Promtail config schema and Loki label conventions |
