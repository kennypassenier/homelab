# Who notifies through which channel

T64, first half: the inventory. Written 2026-09-25 from the repositories and
this project's register, **not from the machines**. Every row says where its
evidence lives. "Live" means a register entry records it working on the
machines; "code only" means the code ships but no measurement shows the path
reaching a person; "planned" means documents only. The redesign, which is
T64's second half, waits until every service is running, as the register
says. This document changes nothing.

## The picture in one paragraph

Almost everything that reaches Kenny goes through **one funnel: a kyu topic,
then a kyu-runner route or http-switchboard, then a Home Assistant webhook
automation, then HA's dispatcher (`script.notification_dispatch`, owned by
pipeline-v2)**. That dispatcher decides whether it becomes a phone push, is
spoken, is deferred to `todo.notifications` during quiet hours, or is only
logged. The dispatcher also copies what it sends onto `notify.kenny`, and
newsflash turns that into desktop toasts. Three senders skip kyu: almanac
posts straight to HA, Proxmox mails root through Gmail, and kp-soft and
SuperSync send application mail. Two monitors are silent: Uptime Kuma has
zero notification channels, and Grafana has no contact point. Three kyu
topics fill up with nobody reading them.

## 1. Paths into Home Assistant

| # | Sender | Route | HA automation | What triggers it | State | Evidence |
|---|---|---|---|---|---|---|
| 1 | **homelab-host** (orchestrator daemon) | POST `homelab.ops` on kyu → kyu-runner route `homelab-ops` (subscription `ha-runner`, `max_attempts=25`). **Fallback:** straight to HA webhook `homelab-ops-…` | `automation.homelab_ops_webhook`: logs every event to `/media/homelab_events.log` and the logbook. It pushes to the phone only when `ok:false` **and** `input_boolean.homelab_event_notifications` is on (it has been on since 2026-09-18) | Every mutating operation; `host-online` at boot; H8 `stack-disabled-<stack>`; nightly `fleet-check` when it has alarming findings, including open or "nok" manual checks and a dead notification path (F222) | **Live**, split: the HA half and the fallback route are proven (F85, F223, F236). The full host→kyu→runner→HA chain is **not proven end to end** (F85) | `host/src/main.rs:2836-2924`, `core/src/notify.rs`, `core/src/ops/fleetcheck.rs:779-812`; REGISTER F85, F222, F223, F236 |
| 2 | **homelab-host systemd units** | `self-update-rollback` and `daemon-failed`, fired by OnFailure scripts | same as row 1 | a failed self-update, or the daemon dying | Live-proven 2026-08-11. **The scripts exist only on the host**; the repository does not have them | `docs/TEST_PLAN.md:287-298`, `docs/USER_GUIDE.md:172-177` |
| 3 | **Prometheus → Alertmanager** (CT 113) | Alertmanager receiver `kyu-hub` → `alerts.raw` → http-switchboard 3.0.0, profile `alertmanager` (subscription `switchboard`) → reshaped JSON | `automation.homelab_alert_webhook` → dispatcher | Four rules: HostDown, FilesystemAlmostFull, DiskPendingSectors, DiskSmartFailed. Grouped by alertname and host, repeated every 12 h, resolved alerts sent too | **Live** (D54, F89) | `stacks/metrics/alertmanager/alertmanager.yml:24-45`, `stacks/metrics/prometheus/rules/homelab.rules.yml`, `captured/messaging/http-switchboard/config.example.toml:32-44` |
| 4 | **kyu hub's own events** | kyu publishes onto `kyu.events` → kyu-runner route `kyu-events` | `automation.hub_kyu_events_webhook` → dispatcher, **throttled to once per 24 h per topic** | dead-lettered and archived count as warnings; expired, flagged and unarchived count as info. Since kyu 3.3.0, `message.expired` is announced at most once per subscription per day (fix-events-1) | **Live** (T55). M-T55, the first real event delivered end to end, is still **open** | `kyu/src/events.rs`, `kyu/docs/USER_GUIDE.md:277-289`; REGISTER T55, M-T55 |
| 5 | **Sonarr and Radarr** | each app's own webhook with a header token → `arr.ops` → kyu-runner route `arr-ops` | `automation.arr_manual_interaction_webhook` → dispatcher | "manual interaction required", meaning a download could not be imported | **Live** (D58). The configuration lives in the apps' own databases and is in no stack file | REGISTER D58; `captured/messaging/kyu-runner/config.example.toml` (route `arr-ops`) |
| 6 | **almanac** (CT 112) | its own notifier, a direct POST to HA with **no kyu hop** | `automation.homelab_ops_webhook`, same as row 1 | update reverted, update unverified, entry set aside, journal backlog. A successful update is only logged | Code **live**. Whether `ALMANAC_NOTIFY_WEBHOOK` is set on CT 112 is **not recorded**; it is not in `stacks/almanac` | `almanac/src/shell/notify.rs`, `almanac/src/main.rs:250-266`, `almanac/src/shell/worker.rs:198-297` |

## 2. Paths that do not end in Home Assistant

| # | Sender | Channel | Recipient | State | Evidence |
|---|---|---|---|---|---|
| 7 | **HA dispatcher** (pipeline-v2's shadow publish) | kyu `notify.kenny` → newsflash, subscription `desktop` (10-minute TTL) → `notify-send` → Plasma toast. `critical` stays on screen; `warning` lasts 30 s; `info` lasts 10 s. There are up to two action buttons | Kenny's desktop, only while he is logged in | **Live**. On 2026-09-20 the register noted the `desktop` subscription had no poller at that moment | `newsflash/courier-core/src/toast.rs:117-133`, `newsflash/config.example.toml`; REGISTER step-11 |
| 8 | **Proxmox VE** | `/etc/pve/notifications.cfg` → mail to root → Gmail SMTP | email | Live, not managed by the homelab | `docs/OPERATIONS_RUNBOOK.md:147-151` |
| 9 | **kp-soft, SuperSync** | SMTP | application mail to their users, such as invitations and login links; not operations mail | Live | `stacks/kp-soft/kp-soft/docker-compose.yml:49-56`, `stacks/productivity/supersync/docker-compose.yml:55-60` |
| 10 | **JobTracker → almanac** | `POST /v1/ingest` → Google Calendar | Kenny's calendar. This is a calendar entry, not an alert; it is sent on a click | Live. It is **the only source almanac has** (F265) | `JobTracker/dashboard/packages/server/src/almanac.js`; REGISTER F265 |
| 11 | **Manual checks** (F221/G17) | a Drift finding (open) or Broken finding (nok) inside the nightly `fleet-check` → row 1 | phone and log | Live. **gap-18** is open: kp-soft's deliberate "nok" makes the check red every night | `core/src/ops/manualchecks.rs`; REGISTER gap-15 |
| 12 | **H7 update badge** | badge in the TUI only | whoever has the TUI open | Live; it sends no push | `client/src/tui/model.rs:350` |

## 3. Topics nobody reads

Anything published to these topics stays on the hub. It shows up on the kyu
dashboard and in the kyu events for that topic, but no route delivers it to a
person.

| Topic | Who publishes | What gets lost | Evidence |
|---|---|---|---|
| `ops.alerts` | `kyu-alert@` on CT 109, when `kyu-backup.service` fails (F179). It is also the documentation example for the chassis `notify` feature | **a failed nightly kyu backup** | `stacks/kyu/rootfs/usr/local/bin/kyu-alert:22-51`; no route in `captured/messaging/kyu-runner/` |
| `switchboard.events` | http-switchboard, for its own delivery failures | a broken Alertmanager→HA leg, which means row 3 can fail silently | `captured/messaging/http-switchboard/config.example.toml:19-20` |
| `notify.actions` | newsflash, when Kenny clicks a toast button (M10) | **every button click**: "gelezen" and "snooze" do nothing downstream | `newsflash/courier-core/src/action_result.rs:15`; no consumer in any repo |

## 4. Watchers that notify no one

| Watcher | What it watches | Channels | Evidence |
|---|---|---|---|
| **Uptime Kuma** | 36 monitors, created by the seeder | **0** (`select count(*) from notification` = 0). kyu-alert's comments name Kuma as the backstop for "kyu is down", so that backstop alerts nobody | REGISTER F265; `stacks/uptime/kuma-seeder/seed.py` |
| **Grafana** (CT 104) and the Loki ruler | dashboards and logs | no contact point. Loki's `alertmanager_url` points at localhost with the comment "no alertmanager in this homelab setup" | `stacks/gateway/loki/loki-config.yaml:47-49`; REGISTER F265 |
| **almanac `/metrics`** | journal pending or unreadable | runbook R12 proposes alerts; no Prometheus rule exists | `almanac/docs/OPERATIONS_RUNBOOK.md:349-383` |
| **chassis-rs `notify` feature** | `service.started`, `update.*`, `health.degraded` and `health.recovered` for every kit service | the feature is built and tested, but **no production service enables it**: kyu, almanac, http-switchboard and kyu-runner all build without it | `chassis-rs/crates/chassis/src/core/notify.rs:30-39`; each consumer's `Cargo.toml` |

Stacks that have no notifier at all: paperwork, jellyfin, recyclarr,
downloader, syncthing, mealie, actual, homepage and inbox. No ntfy, gotify,
apprise, Telegram or Discord appears in any repository.

## 5. Where the documents disagree

The redesign should not inherit these. **Resolved 2026-09-26** marks the
ones corrected in this repository since.

- `docs/deployment/INVENTORY.md:117-123` says Alertmanager delivers to a
  `none` receiver and that the host posts straight to HA. D54 and Y2
  superseded both statements. **Resolved 2026-09-26:** a dated
  "superseded" note under the paragraph.
- `docs/OPERATIONS_RUNBOOK.md:15-18`, `docs/USER_GUIDE.md:172-177` and
  `docs/FEATURES.md:305-310` describe a direct HA webhook with no kyu hop.
  **Resolved 2026-09-26** for the runbook and the user guide; FEATURES.md is
  frozen and states the intent (reach HA), which still holds.
- `captured/messaging/kyu-runner/config.example.toml:48-52` says `kyu.events`
  is not enabled, but T55 enabled it. The live routes (`homelab-ops`,
  `arr-ops` and `kyu-events`) are recorded only in that captured example and in
  `/etc/kyu-runner/config.toml` on CT 109. The kyu-runner repository has one.
  **Resolved 2026-09-26:** re-captured from the live file, which now lives
  at `/appdata/kyu/kyu-runner-config/config.toml`; the three routes and
  their policies match it line for line (webhook ids redacted).
- `http-switchboard/CLAUDE.md` and its README say Alertmanager is not deployed,
  which contradicts D54. The switchboard deployed is 3.0.0; the latest release
  is 3.1.1.
- `kyu-runner/CLAUDE.md` calls the rollout pending, but its README and D53
  record it running on CT 109 since 2026-08-31. The deployed version is 0.2.2;
  the latest release is 0.2.3.
- The http-switchboard template reads only `alerts.0`, so when Alertmanager
  groups several alerts, only the first one reaches HA
  (`captured/messaging/http-switchboard/config.example.toml:39-43`).
- chassis-rs FEATURES and its architecture decisions promise `kyu-topic` and
  `ha-webhook` notify presets. The code has neither.
- almanac maps the kit's `update.failed` event, which also covers "release
  host unreachable", to "update unverified". Its runbook treats that as a
  possible compromise (`almanac/src/main.rs:262-264`).

## 6. Open measurements this inventory depends on

| Item | What is open | Where |
|---|---|---|
| kyu-e576 | the chain test that rings the phone: `homelab checks answer e576228c ok` | `stacks/kyu/kyu-runner/checks.yml`; REGISTER step-10 |
| F85 | host→kyu→runner→HA proven end to end, rather than only the HA half | REGISTER F85 |
| M-T55 | a real `kyu.events` event delivered all the way to HA | REGISTER M-T55 |
| fix-events-1 | the one-day journal reading after 2026-09-21 19:35 UTC; no result is recorded | `kyu/CLAUDE.md` |
| Y2 acceptance | "HA down for a minute, the notification still arrives"; no result is recorded | `docs/deployment/FEATURES.md:65` |
| almanac webhook | whether `ALMANAC_NOTIFY_WEBHOOK` is set on CT 112 | not recorded anywhere |

## 7. What the Life design adds to the redesign (draft, not decided)

The Life documents are Phase 1 drafts. They are listed here as input to the
redesign and are not requirements.

- **Principles from the approved Phase 0 scope.** P1: no coercion, no
  repeated nagging, no escalation, no counting misses. P3: surface what would
  otherwise be forgotten. G12: a notification about a problem proposes its
  solution. C3: kyu carries events and almanac carries the calendar. C7: the
  surfaces are the PC, Android, and HA lights, speech and dashboards.
- **Every Angle, "Notifications and cues".** Three options: keep the HA
  dispatcher; move to event-triggered prompts that expire silently; or use
  only the environment. It adds a wording lint (no "must", "don't forget" or
  "streak") and says push is only for things that must reach Kenny away from
  home.
- **Open questions that touch this inventory:**
  - Keep the dispatcher or move to event cues?
  - Should Overload Mode be one switch or a pause per subsystem?
  - Should an ignored cue be recorded?
  - Is kyu worth its hop before a third consumer exists?
  - The plant engine's escalation to `critical` has to go.

  Sources: `Life/docs/SCOPE.md`, `Life/docs/system/EVERY_ANGLE.md:676-750,
  1201-1241, 8103-8140`.
