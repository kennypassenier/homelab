# LXC Features (Current)

Last updated: 2026-05-28

## Scope

- Headless daemon inside each LXC.
- Sync execution, docker compose orchestration, mount checks, telemetry APIs.

## GitOps Scope Enforcement

- Sparse checkout targets only stacks/<stack_name>/.
- Sparse scope is enforced during initialization and every sync cycle.
- This guarantees each LXC consumes only the folders required for that specific stack.

## Runtime Behaviors

- setup.sh hook support.
- native `latch` release binary install during bootstrap with guarded daily update checks.
- env-backed non-interactive secrets workflow via persistent `LATCH_PAT` / `LATCH_KEY` injection.
- compose pull/up execution per stack app folder.
- lock-file based sync exclusion.
- failsafe sync windows (default hourly) with heartbeat-aware suppression when CLIENT is active.
- heartbeat API endpoint (`POST /api/heartbeat`) for CLIENT session liveness.
- update API endpoint (`POST /api/update`) for immediate daemon image refresh/recreate.
- websocket telemetry endpoint for CLIENT modal/log views.
- restore execution backend endpoint (`POST /api/restore`) with phased status events.
- websocket update RPC (`update_request`/`update_response`) and keepalive frames for idle-stable connections.

## Image Delivery

- LXC daemon container image is built and published to GHCR through GitHub Actions.
- Workflow is change-aware and only runs automatically when `lxc-daemon/` (or its workflow definition) changes.
- Runtime update endpoint can pull `ghcr.io/kennypassenier/homelab-lxc-daemon:latest` (or env override) and recreate the daemon compose service.
