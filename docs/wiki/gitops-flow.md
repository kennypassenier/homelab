# GitOps Flow

> Every infrastructure change is applied by pushing to Git — containers pull and self-deploy within 5 minutes.

## Overview

This project follows a strict GitOps model: the Git repository is the single source of truth for all deployed configuration. No manual `docker run` commands, no in-container edits. If it is not in Git, it does not exist. This means rollbacks, audits, and reproducibility are free.

## The Sync Cycle

Every LXC container runs a cron job that triggers [node-sync.sh](script-node-sync.sh) every 5 minutes:

```
*/5 * * * * root /opt/gitops/scripts/container/node-sync.sh <STACK_NAME> >> /var/log/node-sync.log 2>&1
```

The sync cycle executes these steps in order:

```
1. git fetch origin main
2. git checkout -- .          ← reset SOPS-managed .env files to encrypted state
3. git pull origin main       ← apply latest changes
4. run pre-sync.sh            ← create Docker networks, run migrations
5. docker compose pull -q     ← pull new images silently
6. docker compose up -d --remove-orphans  ← deploy
7. health check               ← warn if any service exited immediately
8. garbage collection         ← purge apps removed from Git
```

### Step 2 — Why `git checkout -- .` before pull?
SOPS [smudge filters](secret-management.md) decrypt `.env` files on checkout, leaving them as unstaged "local changes". Without this reset, `git pull` would fail with a merge conflict on every run. The checkout restores them to their encrypted (committed) form so the pull is always clean.

### Step 4 — Pre-sync hooks
If a stack folder contains a `pre-sync.sh` script, it is executed before any `docker compose` commands. This is used to create Docker networks that span multiple compose projects within the same stack. See [networking.md](networking.md) for why external networks need to be pre-created.

Stacks with `pre-sync.sh`:
- [gateway stack](stack-gateway.md) — creates `gateway_network`
- [media stack](stack-media.md) — creates `media_network`, handles Jellyseerr→Seerr migration
- [paperless stack](stack-paperless.md) — creates `paperless_network`

### Step 7 — Health warnings
After deployment, `node-sync.sh` runs `docker compose ps --filter status=exited`. If any service stopped immediately after starting, a `level=warn` logfmt line is emitted so Grafana/Loki can surface it. The sync does not abort — it warns and continues.

### Step 8 — Garbage Collection
When an app folder is **deleted from Git**, the next sync detects that the folder no longer exists under `stacks/<STACK_NAME>/` but still has data in `/appdata/<STACK_NAME>/<app_name>/`. It then:
1. Runs `docker compose -p <app_name> down` to stop and remove containers
2. Falls back to `docker stop` + `docker rm` if compose project metadata is gone
3. Deletes `/appdata/<STACK_NAME>/<app_name>/` from the host storage

This means removing an app is as simple as deleting its folder in Git and pushing — [remove-app.sh](script-remove-app.md) and [remove-stack.sh](script-remove-stack.md) automate this.

## Making a Change

The standard workflow for any infrastructure change:

```bash
# 1. From your Linux desktop (client), edit the compose file or .env
vim stacks/media/radarr/docker-compose.yml

# 2. Stage and commit (SOPS clean filter encrypts .env automatically on git add)
git add stacks/media/radarr/docker-compose.yml
git commit -m "feat(media): bump radarr port"
git push

# 3. Wait up to 5 minutes — node-sync.sh inside the media LXC applies it automatically
```

## Deploying a New Stack

1. Run `./client.sh → Create a new Stack` ([create-new-stack.sh](script-create-new-stack.md))
2. Edit the generated `docker-compose.yml` and `.env` files
3. `git push`
4. Bootstrap the LXC on the Proxmox host: `./host.sh → Bootstrap a new LXC` ([bootstrap-lxc.sh](script-bootstrap-lxc.md))
5. The LXC runs `node-sync.sh` on its first cron tick and deploys everything

## Removing an App or Stack

Deleting the folder from Git and pushing triggers Garbage Collection automatically on the next sync:

```bash
# Via interactive script (recommended — double confirmation, automatic git push)
./client.sh → Remove an App          # runs remove-app.sh
./client.sh → Remove an entire Stack  # runs remove-stack.sh
```

## Structured Logging

`node-sync.sh` emits logfmt lines to `/var/log/node-sync.log`:

```
ts=2026-05-18T12:00:01+02:00 level=info stack=media app=jellyfin msg="Syncing app"
ts=2026-05-18T12:00:45+02:00 level=warn stack=media app=sonarr msg="Services not running after deploy: sonarr"
```

[Promtail](app-promtail.md) ships these to [Loki](app-loki.md) with `level`, `stack`, and `app` as filterable labels. In Grafana, you can query `{job="node_sync", stack="media", level="warn"}` to see all deployment warnings for the media stack.

## Lock File

To prevent two overlapping cron ticks from racing, `node-sync.sh` acquires a per-stack lock at `/var/lock/node-sync-<STACK_NAME>.lock`. If a previous sync is still running (e.g. a slow image pull), the new instance logs `level=info msg="Another sync is already running. Skipping this cycle."` and exits cleanly.

## See also

- [script-node-sync.md](script-node-sync.md)
- [Secret Management](secret-management.md)
- [Networking](networking.md)
- [script-remove-app.md](script-remove-app.md)
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
