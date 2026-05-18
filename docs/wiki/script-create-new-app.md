# create-new-app.sh

> Adds a new app template to an existing stack with an interactive picker and a boilerplate docker-compose.yml.

## Overview

`scripts/client/create-new-app.sh` lets you add a new application directory to an already-existing stack. It uses the shared [lib-stack.sh](lib-stack.md) template generator so the output is identical to what [create-new-stack.sh](script-create-new-stack.md) produces for individual apps.

## Usage

```bash
./scripts/client/create-new-app.sh
# or via menu:
./client.sh → Create a new App inside a Stack
```

No CLI flags — fully interactive.

## Interactive Flow

1. **Select a stack** — picker of all directories in `stacks/`
2. **Enter app name** — rejects empty names and names that already exist in the stack
3. **Use Docker?** — whether to generate a `docker-compose.yml` template

## What Gets Created

Given stack `media`, app `newapp`, Docker = yes:

```
stacks/media/newapp/
├── docker-compose.yml   ← boilerplate LSIO image template
└── .env                 ← SECRET_EXAMPLE_TOKEN=replace_with_your_actual_secret
```

The template includes:
- `lscr.io/linuxserver/newapp:latest`
- `PUID=1000`, `PGID=1000`, `TZ=Europe/Brussels`
- Volume: `/appdata/media/newapp/config:/config`
- Labels: `com.centurylinklabs.watchtower.enable=true` + `com.homelab.backup.pause=true`
- Port placeholder: `8080:80`

## After Running

1. Edit `docker-compose.yml` — correct the image, ports, and environment
2. Edit `.env` — add real secret values
3. `git add` → SOPS encrypts `.env` automatically
4. `git commit && git push` → deployed within 5 min by [node-sync.sh](script-node-sync.md)

## Libraries Used

- [lib-stack.sh](lib-stack.md) — `require_repo_root`, `prompt_stack_selection`, `generate_app`
- [lib-ui.sh](lib-ui.md)

## See also

- [script-create-new-stack.md](script-create-new-stack.md)
- [script-remove-app.md](script-remove-app.md)
- [lib-stack.md](lib-stack.md)
- [GitOps Flow](gitops-flow.md)
