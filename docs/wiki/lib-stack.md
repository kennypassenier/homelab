# lib-stack — Stack Management Library

> `scripts/client/lib/lib-stack.sh` provides shared functions for creating stacks and apps, selecting them interactively, and generating standard component templates (Watchtower, Promtail).

## Overview

`lib-stack.sh` is a DRY library used by the client-side lifecycle scripts. Instead of each script having its own stack/app selection logic, they all call the functions here. It sources [lib-ui.sh](lib-ui.md) at load time so color variables and interactive prompts are always available.

## Sourcing

```bash
source "$(dirname "$0")/lib/lib-stack.sh"
# lib-ui.sh is sourced automatically by lib-stack.sh
```

## Functions

### `require_repo_root`

Verifies that the current working directory is the repo root by checking for `stacks/` and `scripts/` directories. Exits with an error if not. Called at the top of every client script.

```bash
require_repo_root
```

### `prompt_stack_selection`

Presents an interactive menu of all directories in `stacks/`. Returns the selected stack name via stdout. Returns exit code 2 if the user cancels.

```bash
STACK_NAME=$(prompt_stack_selection) || { ui_info "Cancelled."; exit 0; }
```

Uses `ui_choose` internally — shows a Gum picker or a numbered list depending on terminal capabilities.

### `prompt_app_selection`

Presents an interactive menu of all app directories within a given stack. Returns the selected app name via stdout. Returns exit code 2 if the user cancels.

```bash
APP_NAME=$(prompt_app_selection "$STACK_NAME") || { ui_info "Cancelled."; exit 0; }
```

### `generate_app`

Creates a new app directory inside a stack with a boilerplate `docker-compose.yml` and `.env`.

```bash
generate_app <stack_name> <app_name> [use_docker]
# use_docker: "y" (default) or "n"
```

**Generated `docker-compose.yml`** includes:
- LinuxServer.io image (`lscr.io/linuxserver/<app_name>:latest`)
- `PUID=1000`, `PGID=1000`, `TZ=Europe/Brussels`
- Volume: `/appdata/<stack>/<app>/config:/config`
- Labels: `com.centurylinklabs.watchtower.enable=true` + `com.homelab.backup.pause=true`
- Port placeholder: `8080:80`

**Generated `.env`**:
```
SECRET_EXAMPLE_TOKEN=replace_with_your_actual_secret
```

### `generate_watchtower`

Creates `stacks/<stack_name>/watchtower/docker-compose.yml` with the standard Watchtower configuration for the stack.

```bash
generate_watchtower <stack_name>
```

Watchtower is configured with `--cleanup --label-enable`, meaning it only updates containers with `com.centurylinklabs.watchtower.enable=true`. It updates itself automatically via the same label. Uses `DOCKER_API_VERSION=1.41` for compatibility.

### `generate_promtail`

Creates `stacks/<stack_name>/promtail/` with a `docker-compose.yml`, `.env`, and `config.yml`.

```bash
generate_promtail <stack_name>
```

**Generated `config.yml`** configures three scrape jobs:
1. `varlogs` — ships `/var/log/*log` to Loki
2. `docker` — ships all Docker container logs with Docker pipeline parsing
3. `node_sync` — ships `/var/log/node-sync.log` with logfmt parsing; promotes `level`, `stack`, and `app` as Loki labels; uses `ts` field as the log timestamp

The Loki endpoint uses `${LOKI_IP}` (injected at runtime via `-config.expand-env=true`). The `.env` defaults to `LOKI_IP=10.10.10.7`.

## Which Scripts Use This Library

| Script | Functions used |
|---|---|
| [create-new-stack.sh](script-create-new-stack.md) | `require_repo_root`, `generate_app`, `generate_watchtower`, `generate_promtail` |
| [create-new-app.sh](script-create-new-app.md) | `require_repo_root`, `prompt_stack_selection`, `generate_app` |
| [remove-app.sh](script-remove-app.md) | `require_repo_root`, `prompt_stack_selection`, `prompt_app_selection` |
| [remove-stack.sh](script-remove-stack.md) | `require_repo_root`, `prompt_stack_selection` |

## See also

- [lib-ui.md](lib-ui.md)
- [script-create-new-stack.md](script-create-new-stack.md)
- [script-create-new-app.md](script-create-new-app.md)
- [app-watchtower.md](app-watchtower.md)
- [app-promtail.md](app-promtail.md)
