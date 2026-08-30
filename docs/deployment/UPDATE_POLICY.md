# Update policy — which services update themselves, and which wait

Decided at the Phase 2 gate (Y1) and the deep-dive that followed (Z1-Z4).
Every app carries a `com.homelab.update.policy` label; a stack whose apps
carry none is a stack nobody has decided about, which is the state this
document exists to end.

## Who does the updating

**The orchestrator, and nothing else.** Kenny chose this at Z1 over adding a
watchtower fork beside it. Two reasons: the orchestrator already has the
mechanism — a nightly run, a per-app policy label, the previous image kept,
verification after, rollback on failure — and two systems updating the same
containers is the conflict that already cost this project a round with
almanac. `containrrr/watchtower` was archived on 2025-12-17; the maintained
continuation is `nicholas-fedor/watchtower`, deliberately not adopted.

## The three classes

| Class | Label | When | Today |
|---|---|---|---|
| **Automatic** | `auto` | a failed update is cheap and the rollback is proven | syncthing, promtail, and the arr-suite once it is managed |
| **Manual** | `manual` | a silent failure is expensive | traefik, cloudflared, crowdsec, prometheus, alertmanager, grafana, loki, goaccess, http-switchboard |
| **Pinned** | `manual` + a pinned tag | a new version can break something that only shows up hours later | jellyfin (hardware transcoding), gluetun (VPN throughput) |

`manual` and `auto` are the only values the code understands. `auto-after-N-days`
appears in the orchestrator's own `FEATURES.md` under D9 and was never built —
`TEST_PLAN.md` already records that.

## What an automatic update actually does

Per app, in this order (O9):

1. `docker compose pull` — while the service is still running, so the downtime
   is the swap and not the download.
2. If the container is labelled `com.homelab.update.stop-first=true`,
   `docker stop -t 60`. Sixty seconds because a database checkpoint is not
   always quick, and `up -d` would otherwise kill it and make the next start a
   recovery.
3. `docker compose up -d --remove-orphans`.
4. Verify the service is running. If not, roll back to the captured image.

The verification is deliberately narrow, and it is worth knowing exactly how
narrow: it asks `docker compose ps --status running --services`. So it catches
an image that will not start and a service that fails closed on a config it
does not understand. It does **not** catch a release that starts cleanly and
does the wrong thing. For services on the alert path that is the failure that
matters, which is why they are `manual`.

## Jellyfin, and never during a stream

O10: before updating Jellyfin the orchestrator asks its API which sessions are
playing, and skips the update if any are. After seven skipped nights it
reports rather than silently deferring forever.

**Fails closed**, deliberately. The v1 version of this check did the opposite:
a missing API key, an unreachable Jellyfin or an empty response all exited 0,
meaning "safe to update" — so the exact conditions in which you cannot tell
whether someone is watching were the conditions in which it said go ahead.

Blocked: the API key in `/opt/jellyfin/.env` on CT 106 is refused. Measured
three ways on 2026-08-30 — `Authorization: MediaBrowser Token`,
`X-Emby-Token` and `?api_key=` all return 401 (F32).

## Kenny's own Rust services

Only two of the six self-update, which is the opposite of what everyone
assumed until it was checked (F26).

| Service | Self-updates | Who updates it |
|---|---|---|
| latch | yes, minisign-signed | itself; not a deployed service |
| almanac | yes, reverts itself | itself; the homelab watches its revert event |
| kyu-runner | no, by decision | the orchestrator, from its release binary + checksum |
| http-switchboard | no, by decision | the orchestrator's docker path, `manual` while it sits on the alert path |
| kyu | no — and publishes **no release assets at all** | nothing can, until it publishes binaries (F27, filed in the vault) |
| newsflash | no, by decision | Kenny, by hand, on his own machine |
