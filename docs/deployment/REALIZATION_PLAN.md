# Realization plan — The Homelab Deployment Project

Phase 5 output. **Approved via the gate form on 2026-08-30.** Kenny agreed to
every milestone and confirmed the reasoning behind the rollout order in his
own words:

> "elke container doen we individueel zodat we bij fouten die container kunnen
> restoren. Bij elke container dat we succesvol kunnen uitrollen daalt de kans
> op fouten en zo kunnen we de meest gevoelige containers onder handen nemen
> in de best mogelijke condities."

That is the order's purpose exactly: each success is evidence for the next
one, and the containers that would hurt most are done last, when the procedure
has been rehearsed the most.

He also authorised working through **M0 to M4** without asking per step
(P7). Anything that destroys or replaces a live system — M7 onward — still
waits for his go per step, and a deviation from a frozen decision still
quarantines that area and queues a mini-round rather than being built on.

Ordering rule, from `SCOPE.md` C1: nothing is integrated before the backup
step is green, and inside the rollout the edge comes before what sits behind
it. Everything that only changes this repo runs before anything that touches a
live container.

## Milestones

### M0 · The safety net, before anything else
Nothing here changes how a service runs.

- Take a vzdump of every container that has none: 105, 106, 108, 111, 112, 113
  (104 and 109 already have nightly jobs).
- Container snapshots become part of the replacement procedure (M7/M8): taken
  immediately before a risky step and removed straight after.
  **Correction, 2026-08-30:** this originally said "take one per container",
  which is wrong for the same reason a snapshot is not a backup. On
  thin-provisioned LVM a snapshot grows as the origin diverges from it, so a
  standing snapshot on eight containers is a slow way to fill the thin pool —
  the pool is at 20.8% and would not stay there. A snapshot is an undo button
  with a short life, not an artifact to leave lying around.
- Repair the kyu stack record so its nightly restic run works again (R1).
- Give CT 111 a nightly vzdump so SuperSync is protected at all.
  **Correction, 2026-08-30:** this milestone originally said "adopt CT 111".
  That is not possible: `homelab adopt` takes a `service.yml` describing a
  systemd unit (C7 is native-only), and CT 111 runs a docker stack whose
  hostname is `lxc-productivity-stack`, which A2 refuses. Real adoption
  happens when the stack is rebuilt in M8; until then vzdump is the backup.
- Fix the two dead Traefik routes: syncthing (T42) and MQTT (E4's route half).

**Exit:** every container has at least one backup taken this week and a nightly
job going forward. Evidence: the archive list and one restore drill.

**Status 2026-08-30:** done except the restore drill. All ten guests now carry
a vzdump — 104 (2.4 G), 105 (1.2 G), 106 (24 G), 108 (626 M), 109 (231 M),
111 (1.3 G), 112 (338 M), 113 (965 M), plus 102 and 103 from July — and eight
nightly jobs run between 02:30 and 03:30, before the orchestrator's own 04:00
window. Media keeps `keep-daily=4,keep-weekly=2` rather than 14/8 because 24 GB
a night adds up; everything else keeps 14/8.

### M1 · Close the drift, in the repo only
Everything running on the machines that exists in no repository.

- Alertmanager with its four rules and the `rules/` mount (R2).
- cadvisor on every docker host, the node and cadvisor scrape jobs, the almanac
  job, the SMART collector (R2).
- The Prometheus datasource and three dashboards as provisioning files (R2).
- promtail on CT 104 (R6).

**Exit:** a deploy of the metrics stack changes nothing that is already live,
except where it deliberately does.

**Status 2026-08-30: done.** Everything built by hand on 29-30 August is now
in the repo — Alertmanager with its config and four rules, the rules mount,
the full scrape configuration, Grafana's two datasources and seven
dashboards, the fleet cadvisor file, the SMART collector and its timer. R6 is
closed live as well: promtail runs on CT 104 and its logs arrive in Loki under
`host="platform"`, verified by query rather than by looking at the container.

The exit criterion cannot be met word for word, and pretending otherwise
would be the wrong kind of green. Capturing was not a straight copy: two
differences are deliberate and a deploy will apply them.

1. The live `prometheus.yml` still called the hub `mailbox`. The repo was
   already correct, so a wholesale copy would have regressed it. Kept as
   `kyu`; series scraped before the change keep `job="mailbox"`.
2. The scrape target `10.10.10.14` (scratch container 190) is dropped, because
   E5 removes it and it would otherwise fire `HostDown` on the way out.

Both are recorded here rather than discovered during the first deploy.

### M2 · The orchestrator learns what the layout needs
Code, with tests, no live systems touched.

- `native` becomes a list of services; accept an explicit "no state, by
  decision" (T5, T40, O5's siblings).
- Close the three privileged-container gaps and build the second, privileged
  golden template (O5, O2).
- Auto-restore per config path instead of per stack (O6).
- Enforce `<app>-config` naming (O7).
- Restic repositories per app rather than per stack (D25), plus the
  `synctest-config` → `syncthing-config` history move (T45).
- Bring `restic_base`, `password_file` and the restore timeout into the
  configuration surface (F38, F39).

**Exit:** every item has a test that fails before its fix. A drill on a
throwaway container deploys a two-service native stack from zero.

**Status 2026-08-30: six of seven done**, each red before green.

| Item | Test |
|---|---|
| O6 auto-restore per path | `o6_restore_is_per_path_not_per_stack` |
| F38 restore timeout configurable | `f38_restore_honours_the_configured_timeout` — measured against the old constant, not argued |
| F39 backup target in `host.toml` | `settings_render_keeps_every_config_field`, extended |
| O5 clone-privilege guard | `o5_clone_refuses_a_privilege_level_the_template_cannot_give` |
| O5 uid-mapping validation | `o5_host_owner_uid_must_match_the_privilege_level` |
| D25 per-app repositories | `d25_backup_writes_one_repo_per_owning_app` |
| T40 declared-stateless services | `t40_stateless_must_be_declared_not_inferred` |
| T5 several natives per stack | `t5_a_stack_holds_several_native_services`, `t5_pre_list_state_migrates_on_load` |

Not done: **O7**, the `<app>-config` naming rule. It deviates from its own
frozen text and is quarantined pending a mini-round rather than built — see
`REGISTER.md` F42. The live drill of a two-service native stack waits for M3's
templates and M7's scratch container.

### M3 · The golden templates
- Two templates, privileged and unprivileged, with docker,
  unattended-upgrades, node_exporter, cadvisor and promtail baked in (O2).

**Exit:** a container cloned from either appears in Prometheus and in Loki
with no further steps.

### M4 · Self-registration
- Prometheus targets from a file the orchestrator writes (T1).
- Grafana dashboards generated per stack, plus the fleet dashboard (T2).
- The fleet check, as a command and as a nightly run (Y4, T4).

**Exit:** a new test stack appears as a scrape target, a log source and on a
dashboard without a manual step. The fleet check finds a gap deliberately
recreated on a scratch container.

### M5 · Update behaviour
- Update policy labels on every app, with the three classes (Y1).
- Pre-stop / post-start hooks (O9, T3).
- The Jellyfin stream check, failing **closed** (O10) — blocked on a working
  API key (T36, F32).

**Exit:** a deliberately failing update restores the previous version; an
update during a stream is skipped with a readable reason.

### M6 · The full backup and its drills
Kenny's own gate: nothing is integrated before this is green.

- Every stack in a backup, including the four that are in none (B1).
- The four restore drills, one per kind, plus the quarterly trial restore (B3).
- One file retrieved from the Drive repository and opened (B2).

**Exit:** four recorded drill outcomes with counts — what came back, not that
it succeeded.

### M7 · Assembly: the first container replaced end to end
The milestone that goes missing if nobody names it. One container, all the way
through the C4 procedure, proving the machinery does its own job rather than
that its parts exist.

- Chosen target: `uptime` (CT 108) — the smallest stack, no dependants, and
  its failure costs nothing that matters.

**Exit:** CT 108 destroyed and rebuilt on the same vmid and IP, Uptime Kuma
back with its monitors, total outage measured and recorded.

### M8 · The rollout, least important first
One container per step, each its own go, in dependency order.

`uptime` (done in M7) → `syncthing` → `metrics` → `productivity` →
`messaging` → `media` → `downloader` → `gateway`.

The gateway is last because everything behind it depends on it, and because
its route files are what makes rollback cheap for every stack before it.

**Exit:** S1-S5 from `SCOPE.md` all met.

### M9 · The services that had no home
- kyu-runner deployed, HA automation first (E1).
- http-switchboard deployed, alert chain closed (E2).
- Recyclarr (E3).
- Uptime Kuma's real monitor set, including body checks where a status code
  lies (R4).
- The ansible-era secrets into latch (R9).

**Exit:** a deliberately triggered alert arrives as a Home Assistant
notification.

## What waits for Kenny, and what for another project

Named here so it is visible rather than discovered at execution time.

| Blocked on | What |
|---|---|
| Kenny | a Jellyfin API key (T36) · Cloudflare access (R5) · the D5 mirror deploy key · H2 OPNsense credentials |
| the kyu project | release binaries, so the orchestrator can update it (T35, filed in the vault) |
| the kyu-runner project | two config corrections (T44) |
| notification-pipeline-v2 | releasing scratch container 191 (E5) |

## Scratch resources

`199` for recipe rehearsals (M2, M7), created and destroyed per drill.
CT 190 and 191 are removed, not reused — they hold addresses the layout needs.
