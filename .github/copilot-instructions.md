# GitHub Copilot Instructions — GitOps Proxmox Homelab

These instructions apply to all Copilot interactions in this repository.
Always read `docs/LLM_CONTEXT.md` and `docs/CONTRIBUTING.md` for full context.

---

## Architecture Overview

- **Client:** Linux desktop — all local scripts and Git actions run here. `scripts/client/` only.
- **Host:** Proxmox VE running unprivileged LXC containers. `scripts/host/` only.
- **Containers:** Docker & Docker Compose run _inside_ LXCs. `scripts/container/` only.
- **Shared code:** `scripts/shared/` (e.g., `lib-ui.sh`) is used across environments.
- **Stack layout:** `stacks/<stack_name>/<app_name>/docker-compose.yml`. Each stack may have a `pre-sync.sh`.
- **GitOps sync:** `node-sync.sh` runs every 5 min in the LXC — it pulls Git, runs `pre-sync.sh`, then `docker compose pull -q` + `docker compose up -d --remove-orphans`. Deleted app folders trigger automatic GC (stop + purge).
- **Secrets:** SOPS + Age via Git smudge/clean filters. Never hardcode credentials.
- **Storage:** `/opt/appdata/<STACK>` on Proxmox host, bind-mounted to `/appdata` in LXC.
- **Networking:** Static IPs via DHCP reservations in OPNsense. DNS/SSH via `~/.ssh/config` aliases.

---

## Strict Behavioral Rules

1. **Always ask before acting.** Never execute terminal commands or file edits unprompted. Explain the plan, show the code, and wait for explicit approval.
2. **Keep documentation in sync.** Any change to scripts, architecture, or CLI flags must update `docs/README.md` (and `docs/LLM_CONTEXT.md` if relevant) in the same iteration.
3. **Context awareness.** Assume the terminal is on the Linux client desktop unless an explicit `ssh` login was made. Do not run host or container commands in a client context.

---

## Code Style & Best Practices

### DRY & Libraries
- Always check `scripts/shared/lib-ui.sh` and `scripts/client/lib/lib-stack.sh` before writing new helper functions.
- Source shared libraries at the top of scripts instead of duplicating logic.

### UX & Output
- Use `lib-ui.sh` functions for all output — no raw `echo` with hardcoded ANSI codes.
- Green = success, Red = errors, Yellow = warnings, Cyan/Blue = info.
- Wrap long-running commands with the pacman spinner (`ui_run_pacman`).
- Use numbered interactive menus for user input instead of free-text prompts.
- Scripts must handle non-TTY environments (cron, CI) gracefully — `lib-ui.sh` must auto-disable colors and spinners.

### Safety & Idempotency
- Scripts must be safe to run multiple times — check before modifying (e.g., `~/.ssh/config` entries, existing directories).
- Use `trap` in scripts that create temp files or perform multi-step critical operations, to clean up on failure or Ctrl+C.
- Use `set -e` or explicit error checks; always exit with a non-zero code on critical failure.
- **Destructive actions** (delete, remove, purge) require red-colored warnings and **double confirmation** (`ui_warning` + two "Are you sure?" prompts).
- Never hardcode secrets. Use SOPS/Age or uncommitted `.env` files (`chmod 600`, protected by `.gitignore`).

### Naming & Structure
- Scripts follow `[action]-[object].sh` convention (e.g., `sync-host.sh`, `create-new-app.sh`).
- Place scripts in the correct folder based on execution context (client / host / container / shared).

### Documentation & Arguments
- All comments and docs must be in **English**.
- Explain *why* code exists, not just *what* it does.
- Every significant script must support `-h` / `--help` via `getopts`.
- Add optional CLI flags to all interactive scripts so they can be automated (skip prompts).
