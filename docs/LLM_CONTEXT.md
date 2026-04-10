# LLM Context - GitOps Proxmox Homelab

This file contains the essential context and rules for LLMs (such as Claude, ChatGPT, Gemini) that assist in building and maintaining this project. **Always read this file first at the start of a new session.**

## 1. Project Architecture & Technologies

- **Client/Workstation:** Pop!\_OS. All local scripts and Git actions are executed from this desktop. Assume by default that the terminal is running here.
- **Host:** Proxmox VE (runs unprivileged LXC containers). Recommended resources per standard LXC (like the gateway): 2 Cores, 1GB RAM, 512MB Swap, 8GB Disk.
- **Containers:** Docker & Docker Compose run _inside_ the LXC containers.
- **GitOps Flow:** Each application/stack has a configuration in `apps/<stack_name>/<app_name>`. Inside the LXC, the `node-sync.sh` script runs every 5 minutes (via cron) to fetch changes via Git Pull & Git Sparse Checkouts. Any `pre-sync.sh` scripts in the stack folder are executed first (e.g., for creating external networks). Then the script executes `docker compose pull -q` and `docker compose up -d --remove-orphans`. The script now also includes **Garbage Collection (GC)**: if an app folder disappears from Git, it stops the container and automatically deletes the app data on the host.
- **Secret Management:** Transparent encryption with **SOPS and Age**. `.env` files are automatically encrypted locally via Git smudge/clean filters and decrypted in the containers.
- **Storage:** Fast configuration data (SSD) is located on the Proxmox host under `/opt/appdata/<STACK_NAME>` and is shared via an unprivileged bind-mount to the LXC at `/appdata`.
- **Networking:** DHCP reservations (static IPs) are managed centrally in OPNsense based on the MAC address of the LXC container. Local DNS/SSH is handled via `~/.ssh/config` aliases.
- **Backups:** Restic runs on the host, temporarily pauses containers with the label `com.homelab.backup.pause=true` to prevent database corruption, and backs up the host mounts.

## 2. Strict LLM Instructions (Rules)

1. **ALWAYS ASK PERMISSION:** NEVER execute terminal commands or file edits unprompted. Always explain your plan first, show the code/commands, and wait for an explicit "go" from the user.
2. **Keep documentation up-to-date:** Whenever we adjust the architecture, scripts, or CLI flags, the `README.md` MUST be updated in the same iteration.
3. **Context Check:** Remember that we are not on the Proxmox server or in a container unless we are explicitly logged in via a command (like `ssh`). Scripts in `/scripts/client/` are for Pop!\_OS, `/scripts/host/` for Proxmox, and `/scripts/container/` for inside the LXC. For user interaction, there are now central manager scripts in the root: `client.sh`, `host.sh`, and `container.sh`.
4. **Philosophy & Best Practices:** ALWAYS read and follow the guidelines in `docs/PHILOSOPHY.md` for code style, DRY principles (use of shared libraries), UI/UX (colors and spinners via `lib-ui.sh`), idempotency, and error handling when creating or modifying scripts.

## 3. Current Status

- **Deployed Stacks:**
  - `monitoring`: Contains Uptime Kuma, Grafana, Loki, and Watchtower. Grafana is configured to automatically provision Loki as a datasource.
  - `paperless`: Contains Paperless-ngx, DB, Redis, Broker, Paperless-AI (Tagger UI + RAG backend), Promtail, and Watchtower.
  - `media`: Contains Sonarr, Radarr, Prowlarr, Bazarr, Jellyfin, Seerr, Promtail, and Watchtower. Configuration is neatly separated into individual apps mounted via `/appdata/media/...`.
  - `gateway`: Contains Nginx Proxy Manager (configured with built-in CrowdSec L7 Bouncer), CrowdSec (equipped with LAN/Tailscale whitelists), and GoAccess. Serves as the central reverse proxy and provides active security (including blocks) and web log analysis.
- **Recent Changes:**
  - Promtail configurations (for logging to Loki) now use `-config.expand-env=true` along with `.env` files for dynamically injecting variables (like `LOKI_IP`), making hardcoded IPs in `config.yml` a thing of the past.
  - Client scripts added for lifecycle management: `create-new-app.sh`, `remove-app.sh` and `remove-stack.sh` with a shared library `lib-stack.sh` for DRY code and an interactive numbered CLI interface. Destructive actions now feature red warnings and a double-confirmation mechanism.
  - Introduced central manager scripts (`client.sh`, `host.sh`, `container.sh`) in the repository root to provide interactive menus for all underlying operations.
  - Documentation files (`README.md`, `LLM_CONTEXT.md`, `PHILOSOPHY.md`) have been moved to the `docs/` directory to keep the root clean for users.
  - Client script `register-local-node.sh` has been renamed to the more compact `add-ssh.sh` and made 100% idempotent. It intelligently adjusts local `~/.ssh/config` aliases (instead of blindly adding them) and overwrites incorrect settings without breaking other hosts.
  - Shell scripts (`bootstrap-lxc.sh`, `add-ssh.sh`, `node-sync.sh`) have been upgraded with CLI arguments (`getopts`) and `--help` functionality for better automation. `bootstrap-lxc.sh` is now interactive and dynamically fetches stacks, with support for a `.env` file on the host. All host scripts now follow the clear `[action]-[object]` naming convention.
  - `node-sync.sh` has been made more robust by adding a specific `cd` and by running `docker compose pull -q` beforehand (to catch updates faster and deploy efficiently without unnecessary `--force-recreate` restarts).
  - Full support and integration of `pre-sync.sh` scripts (as used in the media stack for creating Docker networks outside of compose).
  - Advanced Watchtower lifecycle pre-checks added (like `check-streams.sh` for Jellyfin) which cancel updates if streams are active, preventing downtime during use. Watchtower is now also set to update itself across all stacks (`containrrr/watchtower:latest`).
  - **Host Management:** Idempotent host scripts added (`sync-host.sh`, `setup-cron.sh`) to periodically update the Proxmox host via Git, plus a script (`enable-gpu.sh`) for controlled, safe hardware acceleration per LXC (without punching global holes). Host scripts have also received handy flags and an `-h` help function.