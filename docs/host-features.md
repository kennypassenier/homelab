# HOST Features (Current)

Last updated: 2026-06-05

## Scope

- Headless daemon responsibilities:
- Proxmox LXC lifecycle operations.
- Host-side storage operations.
- Host-side hardware passthrough operations (GPU/TUN).
- Backup execution endpoints and event streaming.
- Runtime storage health inspection surfaced in HOST Storage tab.
- Runtime hardware readiness + per-stack intent reconciliation surfaced in HOST Hardware tab.
- Boot policy reconciliation (preview/apply) from stack `lxc-compose.yml` intent.
- Hot-applicable CPU/memory reconciliation (preview/apply) from stack `lxc-compose.yml` intent.

## Runtime Modes

- HOST runs as a headless daemon in deployed operation.
- Runtime workers (API server, backup policy enforcer, failsafe enforcer, release update checker) stay active continuously.
- The canonical host-side repo path is `~/homelab` (usually `/root/homelab` on Proxmox).
- The canonical host env file is `~/homelab/config/.env`.
- `HOST --version` (or `HOST -V`) prints the running binary version.

## Contract with CLIENT

- HOST is invoked by CLIENT APIs.
- CLIENT remains orchestration owner for multi-step flows.
- HOST emits status/events for CLIENT rendering.

## API Surface

- `GET /api/health` for service liveness.
- `GET /api/version` for binary version (Postman-friendly).
- `GET /api/metrics` for runtime metrics including process uptime and per-LXC runtime rows.
  - Optional Bearer auth via `Authorization: Bearer <LXC_API_TOKEN>` when `LXC_API_TOKEN` is configured.
- `POST /api/update` to trigger an immediate self-update check.
- `GET /api/logs/ws` for live log streaming over WebSocket, with bounded in-memory replay history capped at 10,000 old lines, an age threshold controlled by `LOG_HISTORY_AGE_SECS`, and severity-aware eviction that removes old `INFO` lines before `WARN` or `ERROR` lines.

### Metrics Response Schema (`GET /api/metrics`)

```json
{
  "hostname": "proxmox",
  "ip": "10.10.5.250",
  "uptime_secs": 12345,
  "lxc_runtime": [
    {
      "vmid": 101,
      "name": "lxc-cloudflared",
      "status": "RUN",
      "cpu_pct": 3,
      "ram_pct": 25,
      "uptime_secs": 12345
    }
  ]
}
```

- `hostname`: Proxmox node short hostname.
- `ip`: host IP exposed by HOST daemon metrics.
- `uptime_secs`: HOST daemon process uptime in seconds.
- `lxc_runtime[]`: runtime rows visible to CLIENT Host Management.
- `lxc_runtime[].uptime_secs`: mirrors host uptime for running rows and `0` for stopped rows.

## Manual Recovery: HOST Update Runbook

Use this only when automatic HOST self-update is not converging.

Step 1: Replace HOST with the latest GitHub release asset.

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
```

Step 2: Verify runtime version.

```bash
curl -fsSL http://127.0.0.1:8080/api/version
journalctl -u host-daemon.service -n 50 --no-pager
```

Pinned fallback (if API tag lookup is blocked):

```bash
systemctl stop host-daemon.service
curl -fLo /tmp/HOST \
  https://github.com/kennypassenier/homelab/releases/download/host-daemon-v0.1.18/HOST
chmod +x /tmp/HOST
install -m 755 /tmp/HOST /root/homelab/apps/HOST
systemctl start host-daemon.service
```

After manual recovery, future releases should self-update normally again.

## Backup Policy Enforcement

- HOST now runs a continuous backup policy enforcer loop.
- The enforcer reads `~/.config/homelab/backup-schedule.json` and triggers interval-based backup cycles.
- Scheduled cycles enforce restic retention (`forget --prune`) using daily/weekly/monthly policy values.
- Overlapping manual/scheduled cycles are prevented through a guarded single-cycle lock.

## Release-Based Self Update

- HOST supports release-based self-update, not per-push updates.
- Update checks target GitHub Releases latest tag and compare against local binary version.
- On update availability, HOST downloads the release asset, atomically replaces the local executable, and requests a service restart.
- HOST now emits websocket-visible lifecycle/update telemetry including startup `daemon_version=...`, update check status, and post-update reconnect expectations.
- Empty `HOST_UPDATE_REPO` / `HOST_UPDATE_ASSET` env values fall back to safe defaults; the updater now picks the highest `host-daemon-v*` release tag.

## WebSocket Keepalive

- HOST websocket stream emits periodic keepalive frames while idle.
- CLIENT websocket worker actively sends ping frames and auto-recovers stale links.

## Heartbeat-Gated Failsafe Recovery

- HOST runs periodic failsafe windows (default hourly).
- CLIENT sends heartbeat pulses while the TUI is active, primarily via websocket RPC (`client_heartbeat`) with HTTP `POST /api/heartbeat` as backup.
- If heartbeat is fresh at a failsafe window, HOST skips emergency update checks.
- If heartbeat is stale/missing, HOST performs emergency release self-update check.

## GPU Clarification

- GPU passthrough cannot be offloaded to LXC runtime.
- It requires host-level modifications to Proxmox LXC config and host device access rules.
- CLIENT-side hardware.gpu in lxc-compose is an orchestration hint for HOST execution.

## Reconciliation Controls

- `o` / `O`: preview/apply boot policy reconciliation.
- `h` / `H`: preview/apply hot-applicable CPU+memory reconciliation.
