# Notifications redesign — draft for Kenny's choices

T64, second half. **Draft, nothing decided, nothing changed on a machine.**
Written 2026-09-26 while Kenny was away, from `NOTIFICATIONS_INVENTORY.md`
(the first half) and the measurements named below. Every item here is a
choice for Kenny; the queued form carries them with their consequences.

The register row said the redesign waits until every service runs. The
deployment project is not finished, so the first question is whether any of
this starts now. The items are ordered so the ones that do not depend on
the rollout, or on the Life design, come first.

## What does not depend on anything

These close silent paths inside the existing design: kyu as transport,
kyu-runner as delivery, Home Assistant's dispatcher as the one place that
decides how a message reaches Kenny. None of them adds a component.

| Slug | Gap (inventory section) | Proposal | Touches |
|---|---|---|---|
| `kuma-channel` | Uptime Kuma watches 40 monitors with **0** notification channels (§4; 36 when the inventory was written). kyu-alert's own comments name Kuma as the backstop for "kyu is down", so that backstop alerts nobody | One Kuma notification of type webhook, posting to the HA dispatcher **directly**, not through kyu: Kuma's job includes noticing that kyu itself is down, so it cannot depend on kyu | Kuma's database via the seeder (`stacks/uptime/kuma-seeder/seed.py`), one HA automation |
| `orphan-topics` | `ops.alerts` (a failed nightly kyu backup) and `switchboard.events` (a broken Alertmanager→HA leg) are published and read by nobody (§3) | Two kyu-runner routes to the existing `homelab-ops` automation, created in the order the captured config insists on: HA side first, route second | `/appdata/kyu/kyu-runner-config/config.toml` on CT 109, the captured copy here |
| `alertmanager-group` | The switchboard template reads only `alerts.0`, so a grouped alert delivers its first member only (§5) | Fix the template to list every alert in the group; it lives in the http-switchboard project | http-switchboard's profile, then CT 109 |

## What depends on the Life design

The Life project (Phase 1, drafts) asks questions that decide the shape of
everything above the dispatcher. They are listed so this redesign does not
answer them by accident.

| Slug | Question | Why it is not this project's to answer |
|---|---|---|
| `notify-actions` | newsflash publishes every toast click to `notify.actions`, and nothing consumes it (§3): "gelezen" and "snooze" do nothing | What a click should do is the cue design: Life's open question on whether an ignored cue is recorded |
| `dispatcher-or-cues` | Keep HA's dispatcher, or move to event cues that expire silently | Life's Every Angle, "Notifications and cues"; P1 (no nagging, no escalation) |

## What this draft proposes to drop

| Slug | Watcher | Proposal |
|---|---|---|
| `grafana-contact` | Grafana has no contact point; Loki's ruler points at a localhost Alertmanager that does not exist (§4) | Keep alerting in Prometheus → Alertmanager, the one path that is live (D54). Remove the dead `alertmanager_url` from `stacks/gateway/loki/loki-config.yaml` so the file stops promising a route |

## Measurements behind this draft

- Kuma, re-measured 2026-09-26 inside the `uptime-kuma` container on CT 107:
  `select count(*) from notification` → 0, `select count(*) from monitor` → 40.
- The three live kyu-runner routes, re-read from CT 109 on 2026-09-26 and
  matching `captured/messaging/kyu-runner/config.example.toml`.
- `ops.alerts` and `switchboard.events` have no route in that file.
