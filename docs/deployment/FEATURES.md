# Features — The Homelab Deployment Project

Phase 2 output. **Frozen 2026-08-30** (round 2, item Y7). Changes go through
mini-rounds only (`FORM_PROTOCOL.md` §5), recorded here as dated amendments.

IDs are permanent: they appear in commits, tests, documents and forms from
here on. Ratings are Kenny's, taken verbatim from the gate forms.

Scale: **Onmisbaar** (essential) · **Gewenst** (desired) · **Later** ·
**Niet doen**.

## E · Services that need a home

| ID | Feature | Rating | Test bar |
|---|---|---|---|
| E1 | Deploy kyu-runner. HA automation first, then the route, then a smoke test — HA answers 200 to unknown webhook ids, so the reverse order acks into the void | Onmisbaar | A test message fires the HA automation, visible in Traces; one deliberate POST to a non-existent webhook id is measured, not assumed |
| E2 | Deploy HTTPSwitchboard and close the alert chain. Container healthcheck on plain `/healthz`, never `?strict=1` | Onmisbaar | A deliberately triggered alert arrives as a Home Assistant notification |
| E3 | Recyclarr for Sonarr and Radarr. TRaSH `trash_id` hashes resolved at deploy time, never written from memory | Onmisbaar | A dry run lists the profile changes; after the real run they are present in both apps |
| E4 | Remove CT 107. MQTT terminates on the Home Assistant VM (Kenny, 2026-08-30), so the `lxc-mqtt-stack.yml` route and Traefik's `mqtt` entrypoint go with it | Onmisbaar | No Traefik route resolves to an address where nothing listens — checked across all routes, not just this one |
| E5 | Remove scratch containers 190 and 191, freeing 10.10.10.14/.15 for the vmid convention. 191 is shared with notification-pipeline-v2 — coordinate first | Onmisbaar | Both addresses free; a new stack can follow the convention |

## O · What the orchestrator must learn

| ID | Feature | Rating | Test bar |
|---|---|---|---|
| O1 | Full native-service lifecycle: create the container, install the binary, write the unit, place the secrets — not only adopt an existing one | Onmisbaar | kyu-runner deployed from zero onto a scratch container by the orchestrator, then again after a destroy |
| O2 | **Two** golden LXC templates, one privileged and one not (Kenny, X2), with docker, unattended-upgrades, node_exporter, cadvisor and promtail baked in | Onmisbaar | A freshly cloned container appears in Prometheus and in Loki with no further steps |
| O3 | A new stack registers itself with the observability services: Prometheus targets from a file the orchestrator writes, Grafana dashboards from repo provisioning | Onmisbaar | A new test stack appears as a scrape target, a log source and on a dashboard without manual steps |
| O4 | Model v2 storage: configuration moves to `/appdata/<stack>/<app>-config` on the host, restic runs host-side. See `BACKUP_MODEL.md` | Onmisbaar | Per migrated stack: destroy the container, redeploy, the service returns with its settings — for Jellyfin including hardware transcoding |
| O5 | Close the three privileged-container gaps: the clone path silently ignores `unprivileged`, no test covers the privileged path, and `host_owner_uid` is never checked against the privilege level | Onmisbaar | A manifest asking for privileged either gets it or is refused loudly; a uid that cannot work is refused before the container is built |
| O6 | Auto-restore granularity per config path instead of per stack, as the Ansible generation had it | Onmisbaar | A test with one of two config directories empty, failing on today's code and passing after |
| O7 | `<app>-config` becomes an enforced naming rule in `validate_manifest`, not a convention | Onmisbaar | A stack with a config path not ending in `-config` is refused |
| O8 | The orchestrator owns docker updates (Kenny, Z1) — no second updater beside it. Watchtower was archived 2025-12-17; the maintained fork is `nicholas-fedor/watchtower`, deliberately not adopted | Onmisbaar | A deliberately failing update on a test stack demonstrably restores the previous version |
| O9 | Pre-stop / post-start hooks so databases shut down cleanly before an update, extending the existing `com.homelab.backup.pause` pattern | Onmisbaar | An update of the productivity stack shows Postgres stopped cleanly before the new image arrived |
| O10 | Never update Jellyfin during an active stream; after seven skipped nights it reports instead of silently deferring | Onmisbaar | An update started during a stream is skipped with a readable reason |

## R · Repairs to what is broken or crooked

| ID | Feature | Rating | Status |
|---|---|---|---|
| R1 | Repair the kyu stack record (pre-rename paths, `enabled: false`) and re-enable its nightly run | Onmisbaar | open |
| R2 | Bring the live monitoring stack into the repo: Alertmanager + four rules, cadvisor on six hosts, Grafana datasource and three dashboards, the scrape jobs, the SMART collector | Onmisbaar | open |
| R3 | One no-touch list only | Onmisbaar | **done** — the override is out of the live `host.toml`; the compiled list takes effect at the next host update |
| R4 | A real Uptime Kuma monitor set. Some services need a body check, not a status code — kyu-runner answers 200 while delivering nothing | Onmisbaar | open |
| R5 | Capture the Cloudflare tunnel ingress and Access policies into the repo | Onmisbaar | blocked on access |
| R6 | promtail on CT 104, the one docker host that ships no logs to the Loki it hosts | Onmisbaar | open |
| R7 | Adopt CT 111 so SuperSync and Vikunja get backups. A change of container or IP must be reported to Kenny first, with what he has to reconfigure | Onmisbaar | open |
| R8 | Clean up the Traefik routes and add a check that every route resolves to something that answers | Onmisbaar | open |
| R9 | Move the ansible-era secrets into latch — ten plaintext `.env` files across four containers | Onmisbaar | **partly done** — Grafana's password rotated off its shipped default and stored in latch 2026-08-30 |
| R10 | Remove the v1 generation from the working tree | Onmisbaar | **done** 2026-08-30 |

## B · Backup and restore

| ID | Feature | Rating | Test bar |
|---|---|---|---|
| B1 | A full backup covering every stack, including the four ansible-era containers that are in none today | Onmisbaar | A real restore per kind — docker stack, native service, host config — on a scratch container |
| B2 | The Google Drive target. **Verified working 2026-08-30**: it authenticates and holds five repos; no new token needed. Shrinks to a restore drill | Onmisbaar | One file retrieved from the Drive repo and opened — not "the run succeeded" |
| B3 | Four restore drills, one per kind, plus a quarterly automatic trial restore of one stack to a throwaway container | Onmisbaar | Per drill a recorded outcome with counts: files or records that came back |
| B4 | The seven v1 restic repos in the Drive root stay until the new backup is proven by a restore | Onmisbaar | — decision — |

## Y · Cross-cutting

| ID | Feature | Rating | Test bar |
|---|---|---|---|
| Y2 | Homelab operation notifications travel through kyu instead of a direct HA webhook, so an HA outage no longer loses them. One deliberate exception: "kyu itself is down" keeps the direct path | Gewenst | HA down for a minute, an operation run, and the notification still arrives afterwards |
| Y4 | A fleet check that holds the repo against reality: every recorded container exists and is named as expected, every stack file targets a free or owned vmid, every route resolves, every running service is monitored and scraped, every stack's backup is younger than its own schedule allows | Onmisbaar | It finds the gaps found by hand on 2026-08-30 when one is recreated on a scratch container |

## Z · Update ownership per service (decided 2026-08-30)

| Service | Self-updates? | Who updates it |
|---|---|---|
| latch | yes, minisign-signed | itself; not a deployed service |
| almanac | yes, reverts itself | itself; the homelab observes its revert event |
| kyu-runner | no, by decision | the orchestrator, from its release binary + checksum |
| HTTPSwitchboard | no, by decision | the orchestrator's docker path, policy `manual` while it sits on the alert path |
| kyu | no — and **publishes no release assets at all** | kyu starts publishing binaries (Kenny, Z4); until then nothing can update it |
| newsflash | no, by decision | Kenny, by hand on his own machine |

## Amendments

*(none yet — mini-rounds land here with their date)*
