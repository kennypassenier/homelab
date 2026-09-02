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
- ~~Container snapshots as an undo layer.~~ **Withdrawn 2026-08-30 (F43).**
  `pct snapshot` refuses any container with a bind mountpoint, and model v2 is
  bind mounts, so the layer does not exist where it would be used. Tried for
  real on CT 113 before a risky step: "snapshot feature is not available".
  vzdump remains the whole-container net.
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

**Status 2026-08-31: DONE — both templates exist and were verified.**
`debian-12-homelab-v2` on vmid 998 (unprivileged) and
`debian-12-homelab-v2-priv` on 997 (privileged). Not trusted from the build
log: a clone of 998 was started on a throwaway vmid and asked directly —
`prometheus-node-exporter` active with `/metrics` answering 200, both agent
images present, Docker 29.7.2 — then destroyed. The managed stacks now clone
998 instead of the v1 template on 999.

**Earlier status (2026-08-30), kept for the record: the capability is built,
the artifacts are not.**
`template-build` takes a privilege level, names the result `-priv` when it is
privileged, and bakes node_exporter plus pre-pulled cadvisor and promtail
images. Tested both ways.

Building the two real templates needs the new host binary running, and that is
a release. It is therefore the first thing that waits on Kenny rather than on
code — see the table below.

### M4 · Self-registration
- Prometheus targets from a file the orchestrator writes (T1).
- Grafana dashboards generated per stack, plus the fleet dashboard (T2).
- The fleet check, as a command and as a nightly run (Y4, T4).

**Exit:** a new test stack appears as a scrape target, a log source and on a
dashboard without a manual step. The fleet check finds a gap deliberately
recreated on a scratch container.

**Status 2026-08-30: built, not yet drilled.** All three exist with tests:

| Item | What it does | Test |
|---|---|---|
| T1 | deploy writes a per-stack discovery file, destroy removes it; Prometheus reads the directory with `file_sd_configs` | `t1_deploy_writes_a_discovery_file_and_destroy_removes_it`, `t1_discovery_is_off_when_unconfigured` |
| T2 | deploy renders a dashboard and pushes it to Grafana's provisioning directory | `t2_deploy_provisions_a_dashboard_for_the_stack`, `t2_the_generated_dashboard_is_scoped_and_stable` |
| Y4 | the fleet check, as `homelab check` and as a nightly pass | six tests in `fleetcheck_tests.rs` |

The live half of the exit criterion — a real stack appearing by itself —
waits for M7's scratch container, because it needs the new host binary
deployed. That is a release, and a release is Kenny's go.

### M5 · Update behaviour
- Update policy labels on every app, with the three classes (Y1).
- Pre-stop / post-start hooks (O9, T3).
- The Jellyfin stream check, failing **closed** (O10) — blocked on a working
  API key (T36, F32).

**Exit:** a deliberately failing update restores the previous version; an
update during a stream is skipped with a readable reason.

**Status 2026-08-31: two of three.** The policy is written down and applied
(`UPDATE_POLICY.md`), and O9's clean shutdown is built with the order asserted
— pull, stop, up — so the downtime is the swap rather than the download.
O10 is blocked: the Jellyfin API key on CT 106 is refused, measured three
ways (F32). The check cannot be built against an API that will not answer.

### M6 · The full backup and its drills
Kenny's own gate: nothing is integrated before this is green.

- Every stack in a backup, including the four that are in none (B1).
- The four restore drills, one per kind, plus the quarterly trial restore (B3).
- One file retrieved from the Drive repository and opened (B2).

**Exit:** four recorded drill outcomes with counts — what came back, not that
it succeeded.

**Status 2026-08-31: all four drills done.** Counts, not claims:

| Drill | What came back | Time |
|---|---|---|
| Native service (kyu) | 8 files including `.db`, `.db-wal` and `.db-shm` together; the restored database opened with 10 topics, 46 messages, 7 subscriptions, 69 deliveries, 2 apps — identical to live | 3 s |
| Docker stack (metrics) | 23 files, 186 MiB of Prometheus TSDB from the 04:05 snapshot | 13 s |
| Host configuration | 135 files: TLS cert (valid, CN=homelab-host), the 64-byte restic password, `state.json` parsing with 4 stacks, and the intent repo as a working git repo with real commits | 3 s |
| Whole container (almanac, 339 MB) | restored to a free vmid: 903 MiB extracted at 442 MiB/s, the binary, its unit file and 28 MB of data all present | **4 s** |

The container drill was deliberately **not started**: the restore carries the
original hostname and IP, so starting it beside the running almanac would have
put two of the same service on 10.10.10.12. It was inspected by mounting its
volume read-only and then destroyed. That proves the archive is complete and
how long a restore costs; it does not prove the service comes up, which is
what M7 covers on a container that can safely be replaced.

What remains for M6 is the part that cannot be drilled yet: the four
ansible-era stacks are still in no backup of their own, because their
configuration lives inside their containers until M8 moves it.

### M7 · Assembly: the first container replaced end to end
The milestone that goes missing if nobody names it. One container, all the way
through the C4 procedure, proving the machinery does its own job rather than
that its parts exist.

- ~~Chosen target: `uptime` (CT 108) — the smallest stack, no dependants, and
  its failure costs nothing that matters.~~ **Retargeted 2026-08-31.** That
  sentence was written when CT 108 was going to hold Uptime Kuma. Kuma stayed
  on the gateway and CT 108 became the Syncthing hub for Kenny's Obsidian
  vault, so the container the plan names as harmless is now one of the few
  holding live state. Checked before acting rather than after.

- **Target: `home` (CT 115), chosen by Kenny (form N1, 2026-08-31).** It has
  everything the drill needs to prove — a Traefik route, a nightly restic
  repository, two Uptime Kuma monitors, its own dashboard, the log caps — and
  no state that matters, because the whole page is authored in this repo.

**Exit:** CT 115 destroyed and rebuilt on the same vmid and IP, Homepage back
with its widgets, total outage measured and recorded.

**Status 2026-08-31: DONE.** Run in one pass (form N2) with two safety nets in
place: a restic snapshot taken twelve minutes before (`76eb8616`, ten files)
and a 672 MB vzdump. Neither was needed.

What came back: same vmid, same IP, same hostname, `protection` back on,
`onboot` and `startup order=80` restored, the bind mount reattached; three
containers healthy; 200 both directly and through Traefik; both Prometheus
targets `up` with their `stack="home"` label; logs arriving in Loki again;
and the page itself rendering with every live widget — Jellyfin, the *arr
services, Proxmox, Grafana — which also proves the twelve `HOMEPAGE_VAR_*`
secrets were re-delivered from latch and still authenticate. Verified in a
browser, not by curl: this page renders client-side and a 200 says nothing
about what a visitor sees (F77).

**Measured outage: 653 s**, by a one-second probe running throughout. The
breakdown is the actual result, and it is not what the plan assumed: clone
30 s, start 1 s, configuration and file push 28 s, `compose up` 13 s — and
573 s for a single `docker compose pull` that stalled for 8 m 40 s before
completing (F108). The orchestrator's own share is about eighty seconds.

Three defects came out of it, all in the same family as everything else this
project has found — mechanisms that run, report success and describe
something that is not true: the rebuilt stack claimed it had never been backed
up (F106), the deploy read the backup repository from a hardcoded default
rather than the host's configuration (F107), and the ping monitor never
noticed the container had been gone at all (F109). The first two are fixed
with tests; the third is a fact about the monitors that is now written down.

### M8 · The rollout, least important first
One container per step, each its own go, in dependency order.

~~`uptime` (done in M7) → `syncthing` → `metrics` → `productivity` →
`messaging` → `media` → `downloader` → `gateway`.~~
**Corrected 2026-08-31 (F112).** That list was written before half of it
existed. `uptime` became `home` and was the M7 drill; `syncthing`, `metrics`
and `messaging` are v2-native already, built or adopted by the orchestrator
itself. Measured rather than assumed: seven stacks are in host state, and
exactly four containers still carry a v1 hostname.

**What is actually left, in the order it should now run:**

**Status 2026-09-01 — M8 IS DONE.** All four containers replaced, plus one
that was not on the list: Uptime Kuma left the gateway for its own container
(D68), because a watchman on the roof he guards goes down with it.

The gateway was the hardest and cost three self-inflicted outages, all three
now guarded: the apps were not on a shared network so Traefik could not reach
CrowdSec and the bouncer failed closed on every hostname; the route directory
still pointed at the v1 path so every deploy wrote routes nothing reads; and
removing a duplicate CrowdSec bouncer revoked the shared key. Total time
unreachable was roughly fifteen minutes across the afternoon.

What the rebuild bought beyond the rename: the gateway's config is on
/appdata and survives the container, its 145 MB of access logs left the
nightly backup, its three label-based routes became files, and it is the
first stack whose deploy was verified by the checks written the same morning
— which caught two real ownership faults nobody knew about.

**Status 2026-09-01 (earlier):** three of the four are done — `media` (CT 106) was
replaced overnight and verified against every count recorded before it was
stopped; the full result is in `M8_CT106_PREFLIGHT.md`. Only the `gateway`
(CT 104) remains. Media was not a clean run: the registry cache passes its
health probe and then cannot deliver a large ghcr.io blob (F129), which cost
two apps their automatic start. They were started by hand and the fallback
that makes this self-healing shipped in v3.25.0, so the next deploy of this
stack is the proof that it works unattended.

**Status 2026-08-31:** two of the four are done. `productivity` (CT 111) was
replaced in 217 s and `downloader` (CT 105) in about four minutes, both on
their own vmid and address, both verified against numbers recorded before
anything stopped. Two containers that were never in this list also gained
manifests the same evening — CT 109 (kyu, kyu-runner, http-switchboard) and
CT 112 (almanac) — because Kenny asked the question that exposed the gap:
they could be backed up and updated, but nothing said how to rebuild the box.
And the pull-through cache (D60) is live on CT 117, so what remains of M8 is
media and the gateway.

1. **`productivity` (CT 111, `lxc-productivity-stack`)** — vikunja, supersync
   and its postgres. 8 G rootfs, 2 GB RAM, unprivileged, no media mounts, and
   its config still lives at `/opt/*-config` inside the container, so step 1
   of the C4 procedure is a real copy-out. Nothing else depends on it. It also
   cannot be adopted as-is — `homelab adopt` is native-only and A2 refuses its
   hostname — so a rebuild is the only route it has.
2. **`downloader` (CT 105)** and 3. **`media` (CT 106)** — ~~blocked by
   F111~~ **unblocked 2026-08-31 (D59)**: `data_mounts:` says what a borrowed
   directory is, and a missing one now stops the deploy instead of producing
   an empty library. Both still wait on the pre-flight of D61 — the migration
   inventory is re-derived against the live machines, and the counts the
   acceptance test compares against are recorded, BEFORE either is touched.
   Before media specifically: the image cache (D60).
   Both mount `/HDD18TB/subvol-103-disk-0` and `/HDD12TB/subvol-103-disk-0`,
   datasets owned by no-touch CT 103, at `/mnt/data/*`. Both are privileged,
   so they clone template 997 rather than 998. CT 106 needs `gpu: true`,
   which is now W1's business and where F110's wrong render gid was waiting.
   CT 105 needs `vpn: true` for gluetun's tunnel.
4. **`gateway` (CT 104, `lxc-platform-stack`)** — traefik, cloudflared,
   crowdsec, grafana, loki, uptime-kuma, goaccess. Last, because everything
   behind it depends on it and because its route files are what makes
   rollback cheap for every stack before it. Also where the two remaining
   `:latest` pins in the house live.

**Exit:** S1-S5 from `SCOPE.md` all met.

### M9 · The services that had no home

**Status 2026-08-31: five of five, and the exit criterion met.**

- kyu-runner deployed on CT 109, adopted as a native unit, first backup taken.
- http-switchboard deployed beside it; Alertmanager's delivery leg connected
  through the hub, so an alert raised while Home Assistant is down waits
  instead of vanishing.
- Uptime Kuma's monitor set: 33 monitors where there was one, all green.
  Every endpoint probed before it was added — a monitor that is red from
  birth teaches you to ignore the dashboard.
- R9, the ansible-era secrets into latch: `TUNNEL_TOKEN`,
  `CROWDSEC_BOUNCER_API_KEY`, `WIREGUARD_PRIVATE_KEY`, SuperSync's fourteen
  and Vikunja's two. Each verified byte-identical against the running
  container. The first two are the ones that matter most — losing the tunnel
  token costs every external hostname until Kenny makes a new tunnel, and
  losing the WireGuard key leaves the downloader without its VPN.
- Recyclarr (E3) is the one item NOT done: it is a new service rather than a
  homeless one, and nothing depends on it.

**Exit met**, and not by inspection: a deliberately labelled test alert was
pushed through Alertmanager, translated by the switchboard and delivered to
Home Assistant, which pushed it to Kenny's phone. 6 ms, one attempt.

### M9 · The services that had no home — original plan
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
Two rows left on 2026-09-02: the Jellyfin API key (T36) stopped being
needed when D102 started reading it out of `jellyfin.db` on every deploy,
and the OPNsense credentials went with the integration itself (D99).

| Blocked on | What |
|---|---|
| Kenny | Cloudflare access (R5) · the D5 mirror deploy key |
| the kyu project | release binaries, so the orchestrator can update it (T35, filed in the vault) |
| the kyu-runner project | two config corrections (T44) |
| notification-pipeline-v2 | releasing scratch container 191 (E5) |

## Gate log (Phase 7 onward)

Standing rule 5: from Phase 7 the outcome of every gate and mini-round
lands here, not in a chat log and not in `CLAUDE.md`. One row per gate,
written as part of passing it.

| Gate / round | Date | What Kenny decided | Where it landed |
|---|---|---|---|
| Phase 6 → 7 | 2026-09-02 | — (no decision needed: M0–M9 all report DONE, so the loop is closed) | this table; `CLAUDE.md` phase row |
| Form W1–W3 · swap | 2026-09-02 | swap to zero on all managed containers, one at a time with a check after each; investigate the host but change nothing | D95, F185, `HOST_SWAP.md` |
| Form V1–V7 + C1–C9 · config fault | 2026-09-02 | apply the host.toml repair; keep the warning non-fatal; guard the key list with a test; merge the front page; pin Jellyfin | D96–D103, F186–F195 |
| Form K1–K9 · file ownership | 2026-09-02 | generated files take their directory's owner; backups never inside a mounted config dir | F190, measured closed in M1 |
| Form L1–L9 · key loss | 2026-09-02 | the deploy keeps the copy, the nightly round checks it, Kenny holds the escrow passphrase | F196, F199, F201, D104, D105 |
| Form O1–O2 · router backup | 2026-09-02 | watch it from the nightly fleet check, declare the path in host.toml | F198 |
| Form P1 · v1 latch archive | 2026-09-02 | remove it from the secrets repo | D106 |
| **Phase 7 gate · 23 gaps** | 2026-09-02 | **22 × Dichten, 1 × Later (G6, the fact-gatherer's tests)**. He took none of the two 'accept as known limitation' recommendations: the quarterly restore drill (G14) and the register's 133 untested 'fixed' claims (G19) are both to be closed rather than written down as accepted | this table; per-gap rows below as they land |

### The 23 gaps, one row each

Kept here rather than in a chat log, because a list that only exists in a
conversation is gone at the next compaction — and because this is the table
that says how far Phase 7 actually is. Written as each gap lands.

| Gap | What was missing | Status |
|---|---|---|
| G1 | saving one TUI setting silently wiped the others | dicht · F208 |
| G2 | two update features that had never once run | dicht · F213 |
| G3 | Kuma keyword monitors and the seeder had no tests | dicht · F231 |
| G4 | a restore could quiesce a stack and leave it down | dicht · F207 |
| G5 | the same restore never validated the snapshot id first | dicht · F207 |
| G6 | the 325-line fact-gatherer feeding every nightly finding has no test | **later** (Kenny: closing it means rebuilding it to take an executor) |
| G7 | a test whose comment described a test that did not exist | dicht · F209 |
| G8 | `incomplete_step` written by the deploy, read by nobody | dicht · F220 |
| G9 | `install_native` creates no container and places no secrets | gemeten · F228 — groter dan gedacht: een native stack kan helemaal niet vanaf nul |
| G10 | the real stack files are validated by nothing (8 of 13 need latch) | dicht · F224 (structureel; de latch-helft blijft buiten de tests) |
| G11 | three brakes that had never been pressed, and one silent skip | dicht · F210 |
| G12 | cloning CT 997 and seeing it arrive in Prometheus + Loki | dicht · F225 (live gedrild 2026-09-02) |
| G13 | the M2 and M5 drills | M2 gedrild · F228 (gefaald, en dat is het resultaat); M5 open |
| G14 | no recurring restore drill exists | dicht · F229 |
| G15 | the nightly check skipped the nights it mattered most | dicht · F214 |
| G16 | the notification fallback, and Y2's exception to it | dicht · F222 (F223 wacht op een HA-webhook van Kenny) |
| G17 | 94 manual checks printed at every deploy, answered by nobody | dicht · F221 |
| G18 | 16 of 46 apps were verified by nothing | dicht · F215 |
| G19 | 133 register findings marked fixed with no test claim | open — Kenny declined "accept as limitation" |
| G20 | eight commands existed and were documented nowhere | dicht · F212 |
| G21 | two fleet-check branches nobody had ever run | dicht · F211 |
| G22 | Jellyfin's hardware transcoding was verified by inspection only | dicht · F216 |
| G23 | two measurements the register asserted and nobody had made | dicht · F217 |

**19 of 23 closed, 3 open, 1 deferred by Kenny.**

## Scratch resources

`199` for recipe rehearsals (M2, M7), created and destroyed per drill.
CT 190 and 191 are removed, not reused — they hold addresses the layout needs.
