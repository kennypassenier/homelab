# node-sync.sh

> The GitOps engine that runs every 5 minutes inside each LXC — pulls the latest Git state and deploys all Docker Compose apps in the stack.

## Overview

`scripts/container/node-sync.sh` is the core of the [GitOps flow](gitops-flow.md). It runs as a cron job inside every LXC container and is responsible for: pulling the latest configuration from Git, running pre-sync hooks, deploying all apps, health-checking deployments, and garbage-collecting removed apps.

## Usage

```bash
./scripts/container/node-sync.sh [-h] <STACK_NAME>
```

| Argument | Description |
|---|---|
| `STACK_NAME` | Name of the stack this LXC manages (e.g. `media`) |
| `-h` | Show help and exit |

The cron job installed by [bootstrap-lxc.sh](script-bootstrap-lxc.md):
```
*/5 * * * * root /opt/gitops/scripts/container/node-sync.sh <STACK_NAME> >> /var/log/node-sync.log 2>&1
```

## Sync Steps

1. **`git fetch origin main`** — fetch without applying
2. **`git checkout main`** — ensure we are on main
3. **`git checkout -- .`** — reset SOPS-managed `.env` files to their encrypted (committed) state; prevents merge conflicts on the smudge-decrypted files
4. **`git pull origin main`** — apply the latest changes
5. **`pre-sync.sh` hooks** — all `pre-sync.sh` files in the stack are executed (using process substitution to preserve `set -euo pipefail` error propagation)
6. **Per-app deploy** — for each `docker-compose.yml` found up to 2 levels deep:
   - `docker compose pull -q` — pull new images silently
   - `docker compose up -d --remove-orphans` — deploy
   - Health check: `docker compose ps --filter status=exited` — warns if any service exited immediately
7. **Garbage Collection** — for each directory in `/appdata/<STACK>/`, if the corresponding `stacks/<STACK>/<APP>/` no longer exists in Git, the container is stopped and the data is deleted

## Structured Log Output

All output uses the `log_sync` helper, which emits logfmt lines to stdout (redirected to `/var/log/node-sync.log` by cron):

```
ts=2026-05-18T12:00:01+02:00 level=info stack=media app=jellyfin msg="Syncing app"
ts=2026-05-18T12:00:45+02:00 level=warn stack=media app=sonarr msg="Services not running after deploy: sonarr"
```

Fields: `ts` (RFC3339), `level` (info/warn/error), `stack`, `app` (optional), `msg`.

[Promtail](app-promtail.md) parses these lines and promotes `level`, `stack`, and `app` as Loki labels, making them filterable in [Grafana](app-grafana.md).

## Concurrency Lock

A per-stack lock file at `/var/lock/node-sync-<STACK_NAME>.lock` is acquired via `flock -n`. If a previous sync is still running (e.g. a slow `docker pull`), the new instance logs `"Another sync is already running. Skipping this cycle."` and exits cleanly without modifying any state.

## Garbage Collection Detail

When an app is removed from Git:
1. `docker compose -p <app_name> down` — stops and removes all containers in the compose project
2. Falls back to `docker stop <app_name> && docker rm <app_name>` if project metadata is gone
3. `rm -rf /appdata/<STACK>/<APP>/` — deletes all host data permanently

This is triggered automatically by [remove-app.sh](script-remove-app.md) and [remove-stack.sh](script-remove-stack.md) which delete the app/stack from Git and push.

## Legacy Cleanup

The script includes a one-time cleanup: if `/root/sparse-setup.sh` exists (an artefact from early bootstrap versions), it is removed.

## See also

- [GitOps Flow](gitops-flow.md)
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
- [app-promtail.md](app-promtail.md)
- [script-remove-app.md](script-remove-app.md)
- [script-container-sh.md](script-container-sh.md)
