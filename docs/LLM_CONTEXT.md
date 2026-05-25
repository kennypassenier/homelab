# LLM Context — GitOps Proxmox Homelab (2026, Rust Architecture)

This file provides the essential context and rules for LLMs (Claude, ChatGPT, Gemini, etc.) assisting with this project. **Always read this file first at the start of a new session.**
## 1. Architecture Overview (Authoritative Source)

The homelab is now managed by a 3-tier Rust architecture:
- **CLIENT:** Desktop TUI (Ratatui) for scaffolding, management, and GitOps triggers. Handles all stack/app creation, directory scaffolding, SSH alias management, and advanced docker-compose.yml generation (Traefik, Watchtower, healthchecks, permissions, VPN, restart policies). No shell scripts are used for management.
- **HOST:** Proxmox host daemon (Ratatui) for LXC provisioning, persistent storage management, atomic hardware passthrough (GPU/TUN), and backup orchestration. All host logic is implemented in Rust; legacy scripts are fully deprecated.
- **LXC:** Rust daemon (Ratatui) running inside each LXC container, responsible for GitOps sync, secrets management (Ephemeral Secrets Container), atomic mount validation, container orchestration, and telemetry. All sync, secrets, and validation logic is handled here. No bash scripts remain.
**All legacy scripts and tools are fully deprecated:** client.sh, host.sh, container.sh, node-sync.sh, pre-sync.sh, enable-gpu.sh, enable-tun.sh, backup-stacks.sh, reset-stack.sh, Infisical, Nginx Proxy Manager, Gum, SOPS/Age.

**Reverse proxy and security:** Traefik (with CrowdSec Bouncer middleware) is the only supported reverse proxy. Nginx Proxy Manager is not used anywhere.
**Secrets:** Managed by a short-lived Ephemeral Secrets Container, not by scripts or Infisical. No secrets are ever committed to Git or managed by shell scripts.

**TUI/UX:** All terminal UIs use Ratatui. Gum is not used anywhere.
**Storage:** All persistent data is managed by the HOST daemon and bind-mounted from /opt/appdata/<STACK> on the Proxmox host. CLIENT scaffolding ensures all directories exist before deployment.

**Backups:** Orchestrated by the HOST daemon using Restic, with API-driven pause/resume for safe backups.
**Networking:** Static IPs via DHCP reservations (OPNsense), SSH via ~/.ssh/config aliases. VPN kill-switch enforced via network_mode: service:gluetun in compose files.

**Observability:** Promtail ships all logs to Loki; Grafana dashboards are auto-provisioned and require no manual edits.
**All features and requirements are now mapped and tracked in:**
  - docs/architecture.md (global rules, architecture, and infrastructure)
  - docs/client-features.md (CLIENT TUI requirements)
  - docs/host-features.md (HOST daemon requirements)
  - docs/lxc-features.md (LXC daemon requirements)

**FEATURES.md is deprecated.**
## 2. Strict LLM Instructions (Rules)

1. **ALWAYS ASK PERMISSION:** NEVER execute terminal commands or file edits unprompted. Always explain your plan first, show the code/commands, and wait for an explicit "go" from the user.
2. **Keep documentation up-to-date:** Whenever architecture, code, or CLI flags change, update docs/README.md and relevant tier docs in the same iteration.
3. **Context Check:** Assume the terminal is on the Linux desktop unless explicitly logged into the host or a container. Never run host or container commands in a client context.
4. **Contributing Guidelines:** Always follow docs/CONTRIBUTING.md for code style, DRY, UI/UX, idempotency, and error handling.
5. **GitOps first — always:** Never suggest direct fixes inside containers or on the host. All changes must be made in Git and applied via the GitOps flow.

## 3. Tier Responsibilities (Summary)
- **CLIENT:**
  - Scaffolds stacks/apps, generates all docker-compose.yml with advanced features (Traefik, Watchtower, healthchecks, permissions, VPN, restart policies)
  - Manages SSH aliases and directory structure
  - Triggers GitOps sync via HTTP Push API to LXC
  - Provides a premium Ratatui TUI for all workflows

- **HOST:**
  - Proxmox LXC provisioning, hardware passthrough (GPU/TUN), persistent storage management
  - Orchestrates backups (Restic) with API-driven pause/resume
  - All logic in Rust, no shell scripts

- **LXC:**
  - Handles all GitOps sync, secrets management (Ephemeral Secrets Container), atomic mount validation, container orchestration, and telemetry
  - Provides a Ratatui TUI for in-container management
  - No bash scripts or legacy tools

**For full requirements and implementation details, always consult architecture.md and the three x-features.md files.**
# LLM Context - GitOps Proxmox Homelab

This file contains the essential context and rules for LLMs (such as Claude, ChatGPT, Gemini) that assist in building and maintaining this project. **Always read this file first at the start of a new session.**

## 1. Project Architecture & Technologies

- **Client/Workstation:** Linux desktop. All local scripts and Git actions are executed from this desktop. Assume by default that the terminal is running here.
- **Host:** Proxmox VE (runs unprivileged LXC containers). Recommended resources per standard LXC (like the gateway): 2 Cores, 1GB RAM, 512MB Swap, 8GB Disk.
- **Containers:** Docker & Docker Compose run _inside_ the LXC containers.
- **GitOps Flow:** Each application/stack has a configuration in `stacks/<stack_name>/<app_name>`. Inside the LXC, the `node-sync.sh` script runs every 5 minutes (via cron) to fetch changes via Git Pull & Git Sparse Checkouts. Any `pre-sync.sh` scripts in the stack folder are executed first (e.g., for creating external networks). Then the script executes `docker compose pull -q` and `docker compose up -d --remove-orphans`. The script now also includes **Garbage Collection (GC)**: if an app folder disappears from Git, it stops the container and automatically deletes the app data on the host.
- **Secret Management:** Secrets are managed via local uncommitted `.env` files. SOPS/Age is no longer used.
- **Storage:** Fast configuration data (SSD) is located on the Proxmox host under `/opt/appdata/<STACK_NAME>` and is shared via an unprivileged bind-mount to the LXC at `/appdata`.
- **Networking:** DHCP reservations (static IPs) are managed centrally in OPNsense based on the MAC address of the LXC container. Local DNS/SSH is handled via `~/.ssh/config` aliases.
- **Backups:** Restic runs on the host, temporarily pauses containers with the label `com.homelab.backup.pause=true` to prevent database corruption, and backs up the host mounts.

## 2. Strict LLM Instructions (Rules)

1. **ALWAYS ASK PERMISSION:** NEVER execute terminal commands or file edits unprompted. Always explain your plan first, show the code/commands, and wait for an explicit "go" from the user.
2. **Keep documentation up-to-date:** Whenever we adjust the architecture, scripts, or CLI flags, the `README.md` MUST be updated in the same iteration.
3. **Context Check:** Remember that we are not on the Proxmox server or in a container unless we are explicitly logged in via a command (like `ssh`). Scripts in `/scripts/client/` are for Linux desktop, `/scripts/host/` for Proxmox, and `/scripts/container/` for inside the LXC. For user interaction, there are now central manager scripts in the root: `client.sh`, `host.sh`, and `container.sh`.
4. **Contributing Guidelines & Best Practices:** ALWAYS read and follow the guidelines in `docs/CONTRIBUTING.md` for code style, DRY principles (use of shared libraries), UI/UX (colors and spinners via `lib-ui.sh`), idempotency, and error handling when creating or modifying scripts.
5. **GitOps first — always:** NEVER suggest fixing problems by running commands directly inside a container or on the Proxmox host. The correct answer is always: fix the source in Git, push, and let `node-sync.sh` apply the change. For recovery scenarios (e.g. broken sync state), point to the scripts in `scripts/host/` via `host.sh` — never ad-hoc `pct exec` or direct SSH workarounds.

## 3. Current Status

- **Deployed Stacks:**
  - `monitoring`: Contains Uptime Kuma, Grafana, Loki, and Watchtower. Grafana is configured to automatically provision Loki as a datasource.
  - `paperless`: Contains Paperless-ngx, DB, Redis, Broker, Paperless-AI (Tagger UI + RAG backend), Promtail, and Watchtower.
  - `media`: Contains Sonarr, Radarr, Prowlarr, Bazarr, Jellyfin, Seerr, Promtail, and Watchtower. Configuration is neatly separated into individual stacks mounted via `/appdata/media/...`.
  - `gateway`: Contains Nginx Proxy Manager (configured with built-in CrowdSec L7 Bouncer), CrowdSec (equipped with LAN/Tailscale whitelists), and GoAccess. Serves as the central reverse proxy and provides active security (including blocks) and web log analysis.
  - `downloader`: Contains qBittorrent behind a Gluetun VPN kill switch (Surfshark WireGuard), Promtail, and Watchtower. Gluetun and qBittorrent live in the same `docker-compose.yml` — this is required by Docker Compose's `network_mode: service:<name>` which forces both into the same project. qBittorrent only starts once gluetun's internal health server (`http://127.0.0.1:9999`) confirms an active VPN tunnel (`condition: service_healthy`), guaranteeing the kill switch is never bypassed. **Gluetun healthcheck quirks:** the `/gluetun/healthcheck` binary does not exist in the `latest` image — use `wget -qO /dev/null http://127.0.0.1:9999` (GET) instead. `wget --spider` (HEAD) returns HTTP 405 and must not be used. Health target addresses must be set to IPs (`HEALTH_TARGET_ADDRESSES=1.1.1.1:443,8.8.8.8:443`) to avoid a DNS race condition at startup. The WireGuard private key is stored in the `.env` file. Downloads are mounted from the host at `/mnt/downloads`. The LXC requires TUN passthrough — auto-configured by `bootstrap-lxc.sh` or retroactively via `host.sh → Enable TUN Passthrough`. **VueTorrent** is used as the alternative Web UI — installed and kept up-to-date via the LSIO Docker mod (`DOCKER_MODS=ghcr.io/vuetorrent/vuetorrent-lsio-mod:latest`), which runs on every container start. No `pre-sync.sh` or volume mounts needed. Watchtower automatically updates the mod image. The UI must be activated once manually in qBittorrent: Settings → Web UI → Use alternative Web UI → `/vuetorrent`.
- **Recent Changes:**
  - Promtail configurations (for logging to Loki) now use `-config.expand-env=true` along with `.env` files for dynamically injecting variables (like `LOKI_IP`), making hardcoded IPs in `config.yml` a thing of the past.
  - Client scripts added for lifecycle management: `create-new-app.sh`, `remove-app.sh` and `remove-stack.sh` with a shared library `lib-stack.sh` for DRY code and an interactive numbered CLI interface. Destructive actions now feature red warnings and a double-confirmation mechanism.
  - Introduced central manager scripts (`client.sh`, `host.sh`, `container.sh`) in the repository root to provide interactive menus for all underlying operations.
  - Documentation files (`README.md`, `LLM_CONTEXT.md`, `CONTRIBUTING.md`) have been moved to the `docs/` directory to keep the root clean for users.
  - Client script `register-local-node.sh` has been renamed to the more compact `add-ssh.sh` and made 100% idempotent. It intelligently adjusts local `~/.ssh/config` aliases (instead of blindly adding them) and overwrites incorrect settings without breaking other hosts.
  - Shell scripts (`bootstrap-lxc.sh`, `add-ssh.sh`, `node-sync.sh`) have been upgraded with CLI arguments (`getopts`) and `--help` functionality for better automation. `bootstrap-lxc.sh` is now interactive and dynamically fetches stacks, with support for a `.env` file on the host. All host scripts now follow the clear `[action]-[object]` naming convention.
  - `node-sync.sh` has been made more robust by adding a specific `cd` and by running `docker compose pull -q` beforehand (to catch updates faster and deploy efficiently without unnecessary `--force-recreate` restarts).
  - Full support and integration of `pre-sync.sh` scripts (as used in the media stack for creating Docker networks outside of compose).
  - Advanced Watchtower lifecycle pre-checks added (like `check-streams.sh` for Jellyfin) which cancel updates if streams are active, preventing downtime during use. Watchtower is now also set to update itself across all stacks (`containrrr/watchtower:latest`).
  - **Host Management:** Idempotent host scripts added (`sync-host.sh`, `setup-cron.sh`) to periodically update the Proxmox host via Git, plus a script (`enable-gpu.sh`) for controlled, safe hardware acceleration per LXC (without punching global holes), and `enable-tun.sh` for TUN device passthrough (required for VPN containers like gluetun). `bootstrap-lxc.sh` auto-detects whether a stack uses `/dev/net/tun` in any compose file and automatically configures TUN passthrough before the LXC starts — no manual steps needed. For already-bootstrapped LXCs, `host.sh → option 4` auto-detects the stack from the LXC's cron job and only configures TUN if the stack actually needs it — safe to run on any LXC. Host scripts have also received handy flags and an `-h` help function.
  - **`container.sh` auto-detects stack name:** The container manager no longer prompts for a stack name. It reads the stack from the LXC's `/etc/cron.d/gitops-sync` cron job automatically — just run `./container.sh → option 1` to trigger a sync.
  - **Gum TUI integration:** `lib-ui.sh` now integrates [Gum](https://github.com/charmbracelet/gum) (charmbracelet) as a modern, rich TUI backend. When `gum` is installed and stdout is an interactive TTY, all prompts and UI elements use its styled components automatically (bordered headers, animated spinners, interactive menus, styled inputs). When `gum` is absent or the output is non-interactive (cron, CI, piped), everything falls back to plain POSIX equivalents — no changes needed in scripts consuming the library. New wrapper functions added: `ui_choose` (single select), `ui_multiselect` (checkbox picker), `ui_input` / `ui_input_required` (text input), `ui_confirm` (yes/no), `ui_spin` (spinner — preferred over `ui_run_pacman` for new code), `ui_header` (full-width page header), `ui_section` (section heading), `ui_divider` (horizontal rule). All interactive prompts across client scripts route through these wrappers — direct `gum` calls are forbidden outside `lib-ui.sh`.
  - **Structured logging for node-sync + Grafana/Loki integration:** `node-sync.sh` now emits structured logfmt lines (`ts=... level=... stack=... app=... msg="..."`) instead of plain `echo` output. All Promtail `config.yml` files (gateway, media, paperless, QDQ) have a `node_sync` scrape job that parses these lines and promotes `level`, `stack`, and `app` as Loki labels, and uses the logfmt `ts` field as the official timestamp. This enables filtering in Grafana by e.g. `{job="node_sync", level="warn"}` or `{job="node_sync", stack="media", app="jellyfin"}`. The `generate_promtail` function in `lib-stack.sh` has been updated so new stacks get this job automatically.