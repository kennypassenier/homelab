# Planned Use Case: Notification Routing (Webhook/Telegram)

**Tier:** CLIENT + HOST/LXC + external providers
**Status:** Planned

## Goal

Provide a unified alert pipeline that can send events to multiple destinations:

- generic webhooks
- Telegram bot chats
- future providers (Discord, Matrix, email)

## Candidate Alerts

- resource pressure (sustained high RAM usage, swap activity)
- sync failures and repeated retries
- backup/restore failures
- daemon offline or heartbeat gaps
- unusual restart loops or service flapping

## Design Notes

- route through one notification abstraction to avoid per-feature ad-hoc integrations
- support severity levels (`info`, `warn`, `error`, `critical`)
- include stack, host, and component tags for filtering
- enforce backoff and dedupe to avoid alert storms