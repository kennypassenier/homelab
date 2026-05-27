# Use Case: Pre-Sync Hooks

**Tier:** LXC (executes `setup.sh` before every `docker compose up`)  
**Replaces:** `pre-sync.sh` (deprecated; all hook logic moved to `setup.sh`)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

Pre-sync hooks are shell scripts that run **inside the LXC container**, immediately before `docker compose up`, on every GitOps sync cycle. They perform setup work that Docker Compose cannot: creating Docker networks, generating runtime config files, seeding directories, or validating external dependencies.

The hook file is: `stacks/<stack_name>/setup.sh`

It is committed to Git, pulled via sparse checkout, and executed by the LXC daemon inside the LXC container before deploying any app in the stack. It replaces the legacy `pre-sync.sh` mechanism entirely.

---

## 2. Design Principles

| Principle | Requirement |
|---|---|
| **Idempotent** | `setup.sh` must be safe to run on every sync cycle (every 30 min). It must not fail if resources already exist. |
| **Fast** | Must complete within 30 seconds. Long-running setup tasks belong in app entrypoints, not hooks. |
| **Fail-closed** | Non-zero exit code aborts the entire sync cycle. No containers are started or updated. |
| **No secrets** | Must not contain hardcoded credentials. Dynamic values are injected via the `.env` file written by the ephemeral secrets container, which runs before `setup.sh`. |
| **No directory creation** | Creating `/opt/appdata/` subdirectories is the HOST daemon's responsibility. `setup.sh` must not run `mkdir -p /appdata/...`. |

---

## 3. Execution Model

### Execution Order within a Sync Cycle

```
1. Acquire /run/lxc-daemon/gitops.lock
2. git pull (sparse checkout, stacks/<stack_name>/ only)
3. Ephemeral secrets container → writes /run/lxc-daemon/secrets/.env
4. source /run/lxc-daemon/secrets/.env  ← env vars available to setup.sh
5. [[ -f stacks/<stack_name>/setup.sh ]] && bash stacks/<stack_name>/setup.sh
   ↑ If exit code ≠ 0 → abort sync; emit level=error; do NOT proceed to docker compose up
6. docker compose pull -q  (for each app)
7. docker compose up -d --remove-orphans  (for each app)
8. Post-deploy health checks (see post-deploy-actions.md)
9. Release lock
```

### Environment Available Inside `setup.sh`

All variables from the `.env` file are available:
- `LOKI_URL` — centralized Loki endpoint.
- `STACK_NAME` — the current stack name (injected by LXC daemon as env var before exec).
- `APPDATA_PATH` — `/appdata` (the bind-mounted appdata directory).
- Any other secrets from the vault.

The LXC daemon also injects these automatically (not from secrets):
```bash
export STACK_NAME="<stack_name>"
export APPDATA_PATH="/appdata"
export GITOPS_SHA="<current_git_sha>"
```

---

## 4. Standard Use Cases for `setup.sh`

### 4a. Create Isolated Docker Bridge Networks

Apps that communicate over an internal Docker network (e.g., paperless: webserver → db → broker) must declare the network in `setup.sh` so it exists before `docker compose up`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Idempotently create the paperless internal network
docker network inspect paperless_network > /dev/null 2>&1 \
  || docker network create paperless_network
```

### 4b. Create Docker Volumes (if not using bind mounts)

```bash
#!/usr/bin/env bash
set -euo pipefail

# Idempotently create a named volume (rare; prefer bind mounts)
docker volume inspect paperless_db_data > /dev/null 2>&1 \
  || docker volume create paperless_db_data
```

### 4c. Validate External Dependency Reachability

If an app requires an external service (e.g., Loki, a database on another stack) to be reachable before starting:

```bash
#!/usr/bin/env bash
set -euo pipefail

# Fail-closed: abort if Loki is not reachable
curl -sf "${LOKI_URL}/ready" || {
    echo "ERROR: Loki at ${LOKI_URL} is not reachable. Aborting sync."
    exit 1
}
```

### 4d. Generate Runtime Config Files from Templates

```bash
#!/usr/bin/env bash
set -euo pipefail

# Render a config template with env var substitution
envsubst < /opt/homelab/stacks/${STACK_NAME}/qBittorrent.conf.template \
         > /appdata/qbittorrent-config/qBittorrent.conf
```

---

## 5. LXC Daemon: setup.sh Execution Details

```rust
// Executed inside the LXC daemon sync loop
let setup_path = format!("/opt/homelab/stacks/{}/setup.sh", stack_name);
if Path::new(&setup_path).exists() {
    let status = Command::new("bash")
        .arg(&setup_path)
        .env("STACK_NAME", &stack_name)
        .env("APPDATA_PATH", "/appdata")
        .env("GITOPS_SHA", &current_sha)
        .envs(&secrets_env)          // all variables from .env
        .timeout(Duration::from_secs(30))
        .status()?;
    
    if !status.success() {
        // Emit error log and abort sync — do NOT proceed to docker compose up
        emit_log(Level::Error, "setup.sh exited non-zero; aborting sync");
        return Err(SyncError::HookFailed(status.code()));
    }
}
```

**Timeout enforcement:** If `setup.sh` runs for more than 30 seconds, the LXC daemon sends `SIGTERM`, waits 5 seconds, then sends `SIGKILL`. The sync is aborted with a `level=error` log.

**Logfmt events:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="setup.sh found; executing"
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="setup.sh completed successfully" duration_ms=<ms>
ts=<ISO8601> level=error component=lxc stack=<stack_name> msg="setup.sh failed" exit_code=<N>
ts=<ISO8601> level=error component=lxc stack=<stack_name> msg="setup.sh timed out after 30s; killed"
```

---

## 6. CLIENT: Scaffold and Validation of `setup.sh`

When CLIENT scaffolds a new stack (`add-stack.md` Phase 6), it generates a minimal `setup.sh` stub:

```bash
#!/usr/bin/env bash
# Pre-deploy hook for stack: <stack_name>
# This script runs inside the LXC before 'docker compose up' on every sync.
# Rules: must be idempotent, must exit 0 on success, must complete in <30s.
set -euo pipefail

# Example: create internal Docker network for inter-service communication
# docker network inspect <stack_name>_network > /dev/null 2>&1 \
#   || docker network create <stack_name>_network
```

The file is committed with `chmod +x` bits preserved via `.gitattributes`:
```
stacks/*/setup.sh text eol=lf
```

During pre-flight lint, CLIENT validates:
- `setup.sh` starts with `#!/usr/bin/env bash`.
- No `mkdir -p /appdata` or `/opt/appdata` references (forbidden).
- No hardcoded IP addresses or credentials (regex scan for common secret patterns).

---

## 7. Migration from `pre-sync.sh`

Legacy `pre-sync.sh` files in existing stacks are automatically **detected but not executed** by the LXC daemon. If the daemon finds a `pre-sync.sh` in the stack directory, it emits:

```
ts=<ISO8601> level=warn component=lxc stack=<stack_name> msg="legacy pre-sync.sh found; not executed — migrate to setup.sh"
```

The CLIENT TUI shows a persistent amber warning on affected stacks: "Legacy `pre-sync.sh` detected — migrate to `setup.sh`."

---

## 8. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-stack.md` | `setup.sh` stub generated in Phase 6 |
| `update-active-stacks.md` | `setup.sh` runs on every sync triggered here |
| `post-deploy-actions.md` | Runs after `docker compose up`, not before |
| `error-handling-fail-closed.md` | Non-zero exit from `setup.sh` triggers abort |
| `transactional-actions.md` | Rollback on hook failure |
