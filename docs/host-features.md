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
- The canonical host env file is `~/homelab/host-daemon/.env`.
- `HOST --version` (or `HOST -V`) prints the running binary version.

## Contract with CLIENT

- HOST is invoked by CLIENT APIs.
- CLIENT remains orchestration owner for multi-step flows.
- HOST emits status/events for CLIENT rendering.

## API Surface

- `GET /api/health` for service liveness.
- `GET /api/version` for binary version (Postman-friendly).
- `GET /api/metrics` for runtime metrics including process uptime seconds.
- `POST /api/update` to trigger an immediate self-update check.
- `GET /api/logs/ws` for live log streaming over WebSocket.

## One-Time Bootstrap: Manual Deploy of v0.1.10+ to Proxmox

If HOST is stuck on an older version and won't auto-update, bootstrap it with the fixed binary:

```bash
# On Proxmox host
sudo systemctl stop host-daemon.service
cd /tmp
curl -fLo HOST-linux-x86_64-unknown-linux-gnu \
  https://github.com/kennypassenier/homelab/releases/download/host-daemon-v0.1.10/HOST-linux-x86_64-unknown-linux-gnu
chmod +x HOST-linux-x86_64-unknown-linux-gnu
sudo install -m 755 HOST-linux-x86_64-unknown-linux-gnu /root/homelab/apps/HOST-linux-x86_64-unknown-linux-gnu
sudo systemctl start host-daemon.service
```

After this bootstrap, future `make push` releases should auto-update HOST without manual intervention (watch CLIENT Logs tab for "HOST updated" messages).

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
- CLIENT sends heartbeat pulses while the TUI is active.
- If heartbeat is fresh at a failsafe window, HOST skips emergency update checks.
- If heartbeat is stale/missing, HOST performs emergency release self-update check.

## GPU Clarification

- GPU passthrough cannot be offloaded to LXC runtime.
- It requires host-level modifications to Proxmox LXC config and host device access rules.
- CLIENT-side hardware.gpu in lxc-compose is an orchestration hint for HOST execution.

## Reconciliation Controls

- `o` / `O`: preview/apply boot policy reconciliation.
- `h` / `H`: preview/apply hot-applicable CPU+memory reconciliation.
