# Planned Use Case: LXC Resource Pressure Alerts

**Tier:** LXC + CLIENT + notifications
**Status:** Planned

## Goal

Detect containers that are consistently under-provisioned and emit actionable alerts.

## Suggested Trigger Rules

- RAM usage above 80% for a sustained window (for example 5-10 minutes)
- swap usage above 0 for a sustained window
- memory pressure trend still rising after restart

## Suggested Output

- structured event with stack name, vmid, thresholds, and current values
- optional recommendation: increase memory or investigate noisy service
- optional auto-open of a CLIENT resource-tuning workflow

## Dependencies

- periodic metrics collection from LXC runtime
- threshold policy storage (global defaults + stack overrides)
- notification routing backend