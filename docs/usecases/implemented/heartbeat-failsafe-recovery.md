# Use Case: Heartbeat-Failsafe Recovery

**Tier:** CLIENT + HOST + LXC
**Status:** Implemented

## Goal

Reduce idle reconciliation overhead while preserving emergency self-heal behavior when CLIENT orchestration disappears.

## Delivered Behavior

The system now uses an inverse model:

1. CLIENT sends heartbeat pulses only while the TUI is active.
2. LXC and HOST maintain periodic failsafe windows.
3. At each window:
   - if heartbeat is fresh, recovery action is skipped
   - if heartbeat is stale or missing, emergency recovery action runs

This keeps protection when CLIENT is offline while avoiding unnecessary steady-state churn.

## Current Defaults

- failsafe interval: 3600 seconds
- heartbeat TTL: 180 seconds
- CLIENT pulse cadence: 30 seconds

## Implemented Scope

- LXC
  - `POST /api/heartbeat` records heartbeat timestamp
  - failsafe sync windows run in checker loop
  - windows are skipped when heartbeat is fresh
- HOST
  - failsafe enforcer thread evaluates heartbeat freshness each window
  - stale/missing heartbeat triggers release-based self-update check
  - status lines are surfaced in HOST backup/event panel
- CLIENT
  - emits LXC heartbeat pulses while running
  - emits HOST heartbeat pulse via SSH alias while running

## Environment Knobs

- LXC: `FAILSAFE_SYNC_INTERVAL_SECS`, `HEARTBEAT_TTL_SECS`
- HOST: `FAILSAFE_SYNC_INTERVAL_SECS`, `HEARTBEAT_TTL_SECS`, `HOST_HEARTBEAT_FILE`
- CLIENT: `HOST_HEARTBEAT_SSH_ALIAS`, `HOST_HEARTBEAT_FILE`

## Files

- `lxc-daemon/src/api.rs`
- `lxc-daemon/src/gitops.rs`
- `lxc-daemon/src/app.rs`
- `host-daemon/src/failsafe.rs`
- `host-daemon/src/main.rs`
- `client-app/src/main.rs`
