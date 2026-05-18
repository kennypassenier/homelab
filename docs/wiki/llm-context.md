# LLM Context — Homelab GitOps

> Dense structured reference for AI agents and LLMs. Covers architecture, rules, deployed stacks, known quirks, and recent changes. Read this first when onboarding to this codebase.

---

## Architecture (Three-Tier)

```
Linux Client (desktop)
    │  git push / scripts/client/
    ▼
Git Repository (GitHub)
    │  pulled every 5 min by node-sync.sh
    ▼
Proxmox VE Host
    │  unprivileged LXC containers
    ▼
LXC containers → Docker Compose → Apps
```

- **Client**: Linux desktop. All user-facing scripts are in `scripts/client/`. Never run host/container scripts here.
- **Host**: Proxmox VE. Scripts in `scripts/host/`. `pct exec` into LXCs.
- **Containers**: Unprivileged LXC containers. Docker + Docker Compose inside each. Scripts in `scripts/container/`.
- **Shared code**: `scripts/shared/lib-ui.sh` — used by all environments.

---

## GitOps Sync Cycle

`scripts/container/node-sync.sh` runs in each LXC every 5 minutes (cron). Full cycle:

1. Acquire lock (`/tmp/node-sync.lock`) — abort if already running
2. `git fetch origin main`
3. `git reset --hard origin/main` — no merge, hard reset
4. Run `stacks/<stack>/pre-sync.sh` if it exists (network creation, migrations)
5. For each app dir: `docker compose pull -q && docker compose up -d --remove-orphans`
6. **GC**: for app directories that existed in the previous run but are now gone, run `docker compose down --volumes --rmi all` (stop, remove containers + images + volumes)
7. Emit logfmt structured log to `/var/log/node-sync.log`
8. Release lock

Log format: `ts=<RFC3339> level=<info|error> stack=<name> app=<name> msg=<text>`

---

## Secret Management

- **Tool**: SOPS + Age encryption
- **Git filter**: `scripts/client/init-ground-zero.sh` installs `smudge` (decrypt on checkout) and `clean` (encrypt on add) Git filters. `.env` files are transparently encrypted/decrypted.
- **Key location**: `~/.config/sops/age/keys.txt` on the client. Encrypted backup: `secrets/age.key.enc` in the repo.
- **LXC setup**: `bootstrap-lxc.sh` installs SOPS + Age in the LXC and copies the Age key from the host.
- **Rule**: Never hardcode credentials. All secrets go in SOPS-encrypted `.env` files.

---

## Storage Layout

```
Proxmox Host                LXC (bind-mounted)
/opt/appdata/<STACK>/  →    /appdata/<STACK>/
/mnt/downloads/        →    /mnt/downloads/
/mnt/data/18TB/        →    /mnt/data/18TB/
/mnt/data/12TB/        →    /mnt/data/12TB/
/opt/gitops/           →    /opt/gitops/    (Git clone)
```

---

## Networking

- Static IPs via OPNsense DHCP reservations
- SSH aliases defined in `~/.ssh/config` on the client
- Per-stack Docker bridge networks created by `pre-sync.sh` (e.g. `media_network`, `gateway_network`, `paperless_network`)
- Loki endpoint: `10.10.10.7:3100` (monitoring LXC static IP)

---

## Deployed Stacks

### downloader
- Apps: qbittorrent (+ gluetun), watchtower, promtail
- **Gluetun kill switch**: qBittorrent uses `network_mode: service:gluetun` — all traffic goes through the VPN tunnel. qBittorrent won't start until Gluetun is healthy.
- **Gluetun healthcheck quirk**: Use `wget -qO /dev/null http://127.0.0.1:9999` (GET). Do NOT use `wget --spider` (sends HEAD → HTTP 405 → always unhealthy).
- **HEALTH_TARGET_ADDRESSES**: Must be IP addresses (`1.1.1.1:443`), not hostnames — DNS isn't ready at first health check.
- **VueTorrent**: Installed via `DOCKER_MODS=ghcr.io/vuetorrent/vuetorrent-lsio-mod:latest`. Activate once in qBittorrent settings.
- **TUN passthrough** required on the LXC.

### media
- Apps: jellyfin, sonarr, radarr, prowlarr, bazarr, seerr, watchtower, promtail
- Shared network: `media_network` (created by `pre-sync.sh`)
- **Jellyfin GPU**: `/dev/dri:/dev/dri` + `group_add: [993, 44, 104, 105, 106, 107]` + `shm_size: 4gb` + `JELLYFIN_TRANSCODE_DIR=/dev/shm`
- **check-streams.sh**: Watchtower pre-check label on Jellyfin. Queries `/Sessions?apiKey=...`, aborts update if `"IsPlaying": true`.
- **Seerr migration**: `pre-sync.sh` renames `/appdata/media/jellyseerr` → `/appdata/media/seerr` + `chown -R 1000:1000` if old dir exists.
- Media mounts: `/mnt/data/18TB` and `/mnt/data/12TB`
- DNS override on Arr apps: `8.8.8.8`, `1.1.1.1`

### gateway
- Apps: nginx-proxy-manager, crowdsec, goaccess, watchtower, promtail
- Shared network: `gateway_network` (created by `pre-sync.sh`)
- NPM logs at `/appdata/gateway/nginx-proxy-manager/data/logs` shared with CrowdSec and GoAccess
- CrowdSec whitelist: `stacks/gateway/crowdsec/whitelists.yaml` bind-mounted directly

### monitoring
- Apps: loki, grafana, uptime-kuma, watchtower
- Loki IP: `10.10.10.7:3100` — hardcoded in Grafana provisioning + all Promtail `.env` files
- **Grafana provisioning**: `/opt/gitops/stacks/monitoring/grafana/provisioning` bind-mounted to `/etc/grafana/provisioning`. Loki auto-provisioned as default datasource.
- Grafana runs as `user: "0:0"` to read provisioning files

### paperless
- Apps: webserver (paperless-ngx), db (postgres:16), broker (redis:7), ai-assistant (paperless-ai), watchtower, promtail
- Shared network: `paperless_network` (created by `pre-sync.sh`)
- All credentials in SOPS-encrypted `.env`

### cloudflared
- Apps: cloudflared, watchtower, promtail
- `TUNNEL_TOKEN` from SOPS-encrypted `.env`
- `--no-autoupdate` flag — Watchtower manages image updates

---

## UI Library (`scripts/shared/lib-ui.sh`)

Auto-detects Gum TUI when stdout is a TTY. Falls back to POSIX automatically.

Key functions:
- Output: `ui_info`, `ui_success`, `ui_warning`, `ui_error`, `ui_step`
- Prompts: `ui_choose`, `ui_multiselect`, `ui_input`, `ui_input_required`, `ui_confirm`
- Layout: `ui_header`, `ui_section`, `ui_divider`
- Spinners: `ui_spin <label> <cmd>`, `ui_run_pacman <label> <cmd>`

---

## Stack Library (`scripts/client/lib/lib-stack.sh`)

Key functions:
- `require_repo_root` — aborts if not in repo root
- `prompt_stack_selection` — interactive or `-s <name>` flag
- `prompt_app_selection` — interactive or `-a <name>` flag
- `generate_app` — writes `docker-compose.yml` skeleton
- `generate_watchtower` — writes watchtower compose with correct labels
- `generate_promtail` — writes promtail compose + `config.yml` with all three scrape jobs

---

## Code Style Rules

1. **GitOps first**: Never suggest fixing issues by running commands in the container. Fix in Git, push, let `node-sync.sh` apply.
2. **No raw `echo` with ANSI**: use `lib-ui.sh` functions.
3. **No raw `gum` calls**: use `lib-ui.sh` wrappers.
4. **Destructive actions**: `ui_warning` + two confirmations.
5. **Scripts must be idempotent**: check before modifying.
6. **`set -e`** or explicit error checks in all scripts.
7. **`-h`/`--help`** via `getopts` in every significant script.
8. **Naming**: `[action]-[object].sh` (e.g. `sync-host.sh`).

---

## Script Entry Points

| Script | Run on | Purpose |
|---|---|---|
| `client.sh` | Client | TUI menu for all client operations |
| `host.sh` | Proxmox host | TUI menu for host operations |
| `container.sh` | LXC | Trigger node-sync or other container ops |
| `scripts/container/node-sync.sh` | LXC (cron) | Core GitOps sync loop |
| `scripts/host/bootstrap-lxc.sh` | Proxmox host | Bootstrap a new LXC from scratch |

---

## See also

- [architecture-overview.md](architecture-overview.md)
- [gitops-flow.md](gitops-flow.md)
- [secret-management.md](secret-management.md)
- [lib-ui.md](lib-ui.md)
- [lib-stack.md](lib-stack.md)
