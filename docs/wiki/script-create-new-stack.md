# create-new-stack.sh

> Scaffolds a complete new stack directory with optional Docker Compose template, Watchtower, and Promtail in a single interactive workflow.

## Overview

`scripts/client/create-new-stack.sh` creates the `stacks/<STACK_NAME>/` directory structure for a new LXC stack. It optionally generates a boilerplate Docker Compose app, a centralised [Watchtower](app-watchtower.md) container for automatic updates, and a centralised [Promtail](app-promtail.md) container for log shipping. A `trap` rolls back the partially created directory if anything goes wrong before completion.

## Usage

```bash
./scripts/client/create-new-stack.sh [OPTIONS] [STACK_NAME]
# or via menu:
./client.sh → Create a new Stack
```

## Flags

| Flag | Description |
|---|---|
| `-d` | Force Docker Compose inclusion (skip prompt) |
| `-w` | Include centralised Watchtower (requires Docker) |
| `-p` | Include centralised Promtail for Loki (requires Docker) |
| `STACK_NAME` | Optional positional argument — skips the name prompt |
| `-h` | Show help and exit |

If flags are partially provided, the script fills in the missing ones interactively.

## Interactive Flow

1. **Stack name** — prompted if not given as argument; rejects names that already exist
2. **Docker Compose?** — whether to generate a compose template (defaults to yes)
3. **Watchtower + Promtail** — `ui_multiselect` checkbox (both pre-selected by default)

## What Gets Created

Given `./create-new-stack.sh -d -w -p mystack`:

```
stacks/mystack/
├── mystack/
│   ├── docker-compose.yml   ← boilerplate LSIO app template
│   └── .env                 ← plaintext secret placeholder
├── watchtower/
│   └── docker-compose.yml   ← containrrr/watchtower with --label-enable
└── promtail/
    ├── docker-compose.yml
    ├── config.yml           ← 3 scrape jobs: varlogs, docker, node_sync
    └── .env                 ← LOKI_IP=10.10.10.7
```

## Rollback Behaviour

A `trap cleanup_on_error EXIT` monitors the exit code. If the script exits non-zero before completing (`SUCCESS=0`), it removes the partially created `stacks/<STACK_NAME>/` directory entirely and prints troubleshooting tips. Ctrl+C triggers a clean "Cancelled" exit without rollback.

## After Running

1. Edit the generated `docker-compose.yml` for the real image and configuration
2. Replace the `SECRET_EXAMPLE_TOKEN` placeholder in `.env`
3. `git add` — SOPS clean filter encrypts `.env` automatically
4. `git commit && git push`
5. Bootstrap the LXC on the host: `./host.sh → Bootstrap a new LXC` ([bootstrap-lxc.sh](script-bootstrap-lxc.md))

## Libraries Used

- [lib-stack.sh](lib-stack.md) — `require_repo_root`, `generate_app`, `generate_watchtower`, `generate_promtail`
- [lib-ui.sh](lib-ui.md) — all prompts and output

## See also

- [script-create-new-app.md](script-create-new-app.md)
- [script-bootstrap-lxc.md](script-bootstrap-lxc.md)
- [app-watchtower.md](app-watchtower.md)
- [app-promtail.md](app-promtail.md)
- [lib-stack.md](lib-stack.md)
