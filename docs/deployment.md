# Deployment Guide

Last updated: 2026-05-28

This document describes the current deployment order, required credentials, environment files, and verification steps for bringing up the homelab stack safely.

## 1. Deployment Model

The system has three runtime tiers:

- CLIENT: interactive Linux desktop TUI and GitOps authoring surface
- HOST: Proxmox-side daemon and operational scripts
- LXC: per-stack daemon/runtime inside each container

Deployment is GitOps-first:

- change Git-managed config in this repository
- push to the repo
- let HOST/LXC services consume those changes
- do not patch running LXCs manually unless you are in a break-glass recovery scenario

## 2. Required External Services

You need these services/accounts before deployment:

- GitHub repository hosting this repo
- GitHub Container Registry if you will publish/pull the LXC daemon image
- Proxmox VE host with unprivileged LXCs enabled
- Linux client workstation for CLIENT
- Docker / Docker Compose inside each LXC
- Secret source for per-app `.env` material

Current stack hooks in this repo now assume Latch-based `.env` sync in several `pre-sync.sh` files. If you use those hooks, the host or client must be able to run `latch pull` before first sync.

## 3. GitHub Setup

### Releases and Workflows

Current workflows:

- `.github/workflows/host-release.yml`
- `.github/workflows/lxc-image.yml`

Current behavior:

- HOST release assets are built from tag pushes matching `host-daemon-v*`
- LXC image publishing runs on main-branch changes under `lxc-daemon/` or on manual dispatch
- GitHub Actions can publish both without a custom PAT because the workflows use `GITHUB_TOKEN`

### Optional Tokens

You only need extra GitHub tokens for private-repo/private-package scenarios.

1. `HOST_UPDATE_TOKEN`
Purpose: lets HOST self-update against private GitHub Releases.
Permissions:
- `Contents: Read`
- `Metadata: Read`

2. `GITOPS_REPO_TOKEN`
Purpose: lets the LXC daemon clone a private GitOps repo over HTTPS.
Permissions:
- `Contents: Read`
- `Metadata: Read`

3. GHCR pull token
Purpose: only needed if the published LXC image is private.
Permissions:
- `Packages: Read`
- `Metadata: Read`

You do not need a PAT for the current workflows to create releases or push GHCR images in GitHub Actions.

## 4. Environment Files

Use one central file only:

- `config/.env.example` -> copy to `config/.env`

This is the single source of truth for CLIENT, HOST, and LXC daemon runtime configuration.

For the Proxmox host, the default layout is now:

- repo checkout: `~/homelab` (typically `/root/homelab`)
- HOST env file: `~/homelab/config/.env`
- HOST binary: `~/homelab/apps/HOST`

When HOST is launched without a TTY, it auto-loads `config/.env` and runs headless so it can be managed by `systemd`.

Runtime loading precedence:

- CLIENT: `CLIENT_ENV_FILE` -> `config/.env`
- HOST: `HOST_ENV_FILE` -> `config/.env` -> `~/homelab/config/.env`
- LXC (local dev): `LXC_ENV_FILE` -> `config/.env`

This keeps setup user-friendly (single place) and removes split-brain config between app folders.

### CLIENT variables

CLIENT currently cares about:

- `LXC_API_TOKEN`
- `LXC_API_IP`
- `OPNSENSE_BASE_URL` (for example `https://10.10.5.1`)
- `OPNSENSE_API_KEY`
- `OPNSENSE_API_SECRET`
- optional `OPNSENSE_TLS_INSECURE=true` for lab-only self-signed HTTPS
- optional `HOST_IP` for Proxmox HOST metrics API targeting (default `10.10.5.250`)

### HOST metrics API quick checks

Use these checks from CLIENT or HOST to validate Host Management telemetry.

When `LXC_API_TOKEN` is empty (no auth required):

```bash
HOST_IP="10.10.5.250"
curl -fsSL "http://${HOST_IP}:8080/api/metrics"
```

When `LXC_API_TOKEN` is set (Bearer required):

```bash
HOST_IP="10.10.5.250"
TOKEN="$(grep '^LXC_API_TOKEN=' config/.env | cut -d '=' -f2-)"
curl -fsSL \
  -H "Authorization: Bearer ${TOKEN}" \
  "http://${HOST_IP}:8080/api/metrics"
```

Expected auth failure if token is configured but missing/wrong:

```bash
curl -i "http://${HOST_IP}:8080/api/metrics"
# HTTP/1.1 401 Unauthorized
```

### HOST variables

HOST currently cares about:

- `GITOPS_REPO` (recommended: `/root/homelab`)
- `HOST_ENV_FILE` (recommended: `/root/homelab/config/.env`)
- `HOST_UPDATE_REPO`
- `HOST_UPDATE_ASSET`
- optional `HOST_UPDATE_TOKEN`
- optional `HOST_UPDATE_SERVICE` (default `host-daemon.service`)
- optional `RESTIC_REPO_BASE` for HOST daemon per-stack restic target base
- optional `RCLONE_CONFIG_FILE` for rclone-backed restic repositories (Google Drive, etc.)
- optional `FAILSAFE_SYNC_INTERVAL_SECS` for inverse heartbeat failsafe window cadence
- optional `HEARTBEAT_TTL_SECS` freshness threshold for heartbeat suppression
- optional `HOST_LOG_HISTORY_MAX` for in-memory HOST websocket replay retention (default `500`, clamp `50..10000`)

### LXC variables

LXC currently cares about:

- `GITOPS_REPO_URL`
- optional `GITOPS_REPO_TOKEN`
- optional `LXC_SELF_UPDATE_CMD` (overrides full update command used by update APIs)
- optional `LXC_DAEMON_IMAGE` (default `ghcr.io/kennypassenier/homelab-lxc-daemon:latest`)
- optional `LXC_DAEMON_COMPOSE_DIR` (default `/opt/lxc-daemon`)
- optional `LXC_DAEMON_COMPOSE_SERVICE` (default `lxc-daemon`)

LXC stack identity is not sourced from central `config/.env`.
It is derived per container from:

- `/etc/homelab/lxc-daemon.toml` (`[sync].stack_name`) written by HOST bootstrap
- `stacks/<stack>/lxc-compose.yml` (for `network.reserved_ipv4`)

## 5. Stack Secret Files

Per-app runtime secrets should not be committed.

Current expected model:

- stack `pre-sync.sh` scripts export app `.env` files into `/appdata/<stack>/<app>/.env`
- those files are created on the LXC side before compose up
- secret values come from your external secret source, not Git

Before first deployment, verify that every stack hook can populate the required `.env` targets it references.

## 6. Bring-Up Order

Use this order.

1. Prepare GitHub.
- Confirm workflows exist.
- Confirm release tags and GHCR namespace conventions.
- If the repo or packages are private, create the optional read tokens above.

2. Prepare the client workstation.
- Install Rust toolchain if building locally.
- Ensure Git SSH access works.
- Create central env file from `config/.env.example`.

3. Prepare the Proxmox host.
- Clone this repo to `~/homelab` on the host.
- Ensure `config/.env` exists in that repo checkout.
- Install the persistent HOST service with `./install-host-service.sh`.
- Ensure the systemd service name is `host-daemon.service` if you want self-update restarts to work unchanged.
- Ensure `/opt/appdata` and backup storage roots exist.

4. Prepare each LXC.
 
 
 
5. *(Optional)* Sync credentials to LXC via Latch Clone.
- On CLIENT, run `./scripts/client/setup-latch.sh` to set up the latch CLI + keyring.
- Configure credentials with `latch login` and `latch init` or `latch project`.
- Use the CLIENT TUI secrets flow or `latch clone offer/create/apply` for encrypted machine transfer.
- Latch Clone supports end-to-end encrypted credential migration without temp files.

6. Deploy stacks via GitOps.
- If private, also set `GITOPS_REPO_TOKEN`.

5. Prepare secrets.
- Verify every stack `pre-sync.sh` can authenticate to the secret backend.
- Verify destination directories under `/appdata/<stack>/<app>/` exist or are created by your stack workflow.

### Restic + Google Drive (rclone backend)

Current status:

- Restic backup workflow is implemented.
- Google Drive is supported through restic's `rclone:` backend when rclone is installed/configured.

Host setup checklist:

1. Install `restic` and `rclone` on the Proxmox host.
2. Configure rclone remote (example remote name: `gdrive`).
3. Set host backup env values:
	- `RESTIC_REPOSITORY=rclone:gdrive:homelab-restic` (script path)
	- `RESTIC_REPO_BASE=rclone:gdrive:homelab` (HOST daemon per-stack path)
	- `RESTIC_PASSWORD=<strong password>`
	- optional `RCLONE_CONFIG_FILE=/root/.config/rclone/rclone.conf`
4. Run `scripts/host/backup-stacks.sh`; repository init is automatic when missing.

6. Publish artifacts.
- Create a `host-daemon-vX.Y.Z` tag to publish a HOST release asset.
- Push `lxc-daemon/` changes or run the LXC image workflow manually.

7. Activate stacks.
- Use CLIENT to create/edit stack config.
- If you want DHCP reservation automation, set `network.reserved_ipv4` in the stack config editor and export the OPNsense variables above on CLIENT.
- Keep `deploy.enabled=false` until a stack is ready.
- Activate stacks explicitly from CLIENT when ready.

8. Trigger first sync.
- From CLIENT, deploy selected stack(s).
- Confirm LXC sparse checkout initializes correctly.
- Confirm setup hook, compose pull, and compose up complete.

## 7.1 HOST Service Bring-Up

Recommended host flow:

1. Clone or update the repo in `~/homelab`.
2. Ensure `~/homelab/.latch/config.toml` is present from Git.
3. Authenticate Latch on the host.
4. Pull the correct secrets environment into `~/homelab`.
5. Verify `~/homelab/config/.env` exists and points `GITOPS_REPO` to `~/homelab`.
6. Run `./install-host-service.sh` as root.
7. Check status with `systemctl status host-daemon.service`.
8. Follow logs with `journalctl -u host-daemon.service -f`.

Operational notes:

- `systemd` is the correct runtime for "always active on reboot/crash".
- `tmux` is fine for manual interactive testing but is not the persistent production path.
- HOST logs go to the systemd journal in headless mode.
- CLIENT heartbeat now flows over websocket RPC with HTTP fallback and does not require SSH reachability.

Day-to-day HOST operations:

```bash
# Check whether HOST is up
systemctl status host-daemon.service

# Follow live HOST logs
journalctl -u host-daemon.service -f

# Show recent HOST logs (last 200 lines)
journalctl -u host-daemon.service -n 200 --no-pager

# Restart HOST after config or binary updates
systemctl restart host-daemon.service

# Stop HOST manually
systemctl stop host-daemon.service

# Start HOST manually
systemctl start host-daemon.service

# Confirm service is enabled at boot
systemctl is-enabled host-daemon.service

# Confirm restart policy
systemctl show host-daemon.service -p Restart -p RestartSec
```

### HOST Manual Update Recovery (Only if Auto-Update Fails)

Use this only when HOST is stuck on an old version and release-based self-update does not converge.

```bash
# On Proxmox host (check systemd unit first)
set -euo pipefail

REPO="kennypassenier/homelab"
ASSET="HOST"
DEST="/root/homelab/apps/HOST"  # Match your systemd ExecStart binary path

TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases" \
  | sed -n 's/.*"tag_name":[[:space:]]*"\(host-daemon-v[^"]*\)".*/\1/p' \
  | sort -V \
  | tail -1)"

if [ -z "${TAG}" ]; then
  echo "Could not detect latest host-daemon-v tag" >&2
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET}"

echo "Installing ${URL}"
systemctl stop host-daemon.service
curl -fLo /tmp/${ASSET} "${URL}"
chmod +x /tmp/${ASSET}
install -m 755 /tmp/${ASSET} "${DEST}"
systemctl start host-daemon.service
systemctl is-active host-daemon.service

Verify:

```bash
curl -fsSL http://127.0.0.1:8080/api/version
journalctl -u host-daemon.service -n 50 --no-pager
```

Pinned fallback (if release API lookup is blocked):

```bash
systemctl stop host-daemon.service
curl -fLo /tmp/HOST \
  https://github.com/kennypassenier/homelab/releases/download/host-daemon-v0.1.18/HOST
chmod +x /tmp/HOST
install -m 755 /tmp/HOST /root/homelab/apps/HOST
After deployment, verify:

- CLIENT `cargo check` passes locally
- CLIENT can open the app config editor and save image changes through GitOps
- CLIENT can stream live LXC deploy logs during a sync
- HOST and LXC services are running
- HOST self-update can read releases
- LXC sparse checkout contains only `stacks/<stack_name>/`
- stack `lxc-compose.yml` files contain expected `deploy`, `network`, and `resources` blocks
- stack `lxc-compose.yml` files contain expected `deploy`, `network`, `boot`, and `resources` blocks
- OPNsense reservations are present for any stack using `dhcp-reserved` plus `network.reserved_ipv4`
- per-app `.env` files exist under `/appdata/...`
- Docker containers start successfully inside each LXC
- backup orchestration can reach `http://<lxc_ip>:8080`

## 8. Known Remaining Gaps

Check `docs/usecases/pending/` for the current active backlog.
