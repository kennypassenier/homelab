# Use Case: Live Deploy Telemetry

**Tier:** CLIENT + LXC
**Status:** Implemented

## Behavior

- CLIENT connects to the LXC daemon WebSocket log stream when a sync is dispatched.
- Live log lines are appended into the CLIENT log ring buffer during the active deploy window.
- Synthetic mock deploy logs are disabled once real LXC telemetry is received.
- Sync acceptance and stream failures are surfaced in CLIENT status/log output.

## Implemented In

- client-app/src/main.rs
- client-app/src/app.rs
- lxc-daemon/src/api.rs
- lxc-daemon/src/app.rs