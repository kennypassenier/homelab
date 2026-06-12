# LXC Features (Current)

Last updated: 2026-06-12

## Scope

- Headless daemon inside each LXC.
- Sync execution, docker compose orchestration, mount checks, telemetry APIs.

## GitOps Scope Enforcement

- Sparse checkout targets only stacks/<stack_name>/.
- Sparse scope is enforced during initialization and every sync cycle.
- This guarantees each LXC consumes only the folders required for that specific stack.

## Runtime Behaviors

- setup.sh hook support.
- mount validation defaults to primary path `/appdata` and verifies mount points via `/proc/self/mountinfo` (legacy `/docker` + `/config` checks removed).
- optional secondary mount validation can be enabled with `MOUNT_CHECK_SECONDARY=<path>` (empty by default).
- native `latch` release binary install during bootstrap with guarded daily update checks.
- one-shot request-scoped secrets workflow: CLIENT can pass latch pull context (`PAT` / `KEY` / `REPO` / `project` / optional `env` / `sparse`) in sync/update RPC or HTTP requests.
- compose pull/up execution per stack app folder.
- lock-file based sync exclusion.
- failsafe sync windows (default hourly) with heartbeat-aware suppression when CLIENT is active.
- heartbeat API endpoint (`POST /api/heartbeat`) for CLIENT session liveness.
- update API endpoint (`POST /api/update`) for immediate daemon image refresh/recreate, with optional one-shot `latch` payload.
- websocket telemetry endpoint for CLIENT modal/log views, with bounded in-memory replay history capped at 10,000 old lines, an age threshold controlled by `LOG_HISTORY_AGE_SECS`, and severity-aware eviction that removes old `DEBUG`/`INFO` lines before `WARN` or `ERROR` lines.
- restore execution backend endpoint (`POST /api/restore`) with phased status events.
- websocket update RPC (`update_request`/`update_response`) and keepalive frames for idle-stable connections.
- websocket sync/update RPC (`sync_request`, `update_request`) accept optional one-shot `latch` payload used for request-scoped `latch pull` execution.

## Image Delivery

- LXC daemon container image is built and published to GHCR through GitHub Actions.
- Workflow is change-aware and only runs automatically when `lxc-daemon/` (or its workflow definition) changes.
- Local `make build-lxc` uses a containerized Rust builder so the standalone daemon binary stays compatible with the older glibc versions present in deployed LXCs.
- Runtime update endpoint can pull `ghcr.io/kennypassenier/homelab-lxc-daemon:latest` (or env override) and recreate the daemon compose service.
