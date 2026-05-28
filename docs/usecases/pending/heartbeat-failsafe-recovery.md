# Use Case: Heartbeat-Failsafe Recovery

**Tier:** CLIENT + HOST + LXC
**Status:** Pending

## Goal

Reduce idle reconciliation overhead while preserving emergency self-heal behavior when CLIENT orchestration disappears.

## Problem Statement

Current periodic pull/self-update loops are useful as an emergency fallback, but they run even when systems are healthy and managed through normal CLIENT-driven workflows.

## Required Behavior

Instead of fixed-interval emergency pulls at all times:

1. CLIENT sends heartbeat pulses to HOST and LXC while control plane is healthy.
2. HOST/LXC maintain last-seen heartbeat timestamps.
3. If heartbeats stop, HOST/LXC start a grace timer.
4. Only when the grace timer expires do they trigger emergency `git pull` + update/reconcile.

## Why This Is Useful

- lower steady-state resource usage than always-on periodic pulls
- keeps emergency recovery path when CLIENT is unreachable
- reduces churn and avoids unnecessary update checks during healthy operation

## Suggested Guardrails

- jittered timers to avoid HOST and all LXCs pulling simultaneously
- capped retry with exponential backoff
- recovery actions only from trusted branch/tag policy
- structured events for "heartbeat stale", "recovery started", "recovery completed/failed"

## Dependencies

- heartbeat transport and persistence in HOST/LXC daemons
- policy configuration for grace timeout and recovery retry limits
- audit/event logging path surfaced in CLIENT UI
