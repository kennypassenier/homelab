# Register — The Homelab Deployment Project

The mechanism behind scope constraint C6: **every finding, task and decision
of this project gets a number and lives here**, not in a conversation. A
session that lost its context resumes by reading this file. Updating a row is
part of doing the work, not paperwork afterwards.

↳ *C6 = the scope constraint requiring this project to survive a context
loss; see `SCOPE.md`.*

**Numbering.** `D#` decisions taken, `T#` tasks to do, `F#` findings (facts
discovered that change what we do), `B#` blockers waiting on someone else.
Numbers are permanent and are never reused, including after a row closes.

**Status.** `open` · `doing` · `done` · `parked` · `dropped`.

## Decisions (D)

| # | Decision | Where it landed | Date |
|---|---|---|---|
| D1 | Scope approved: goals G1-G8, non-goals N1-N5, constraints C1-C8 | `SCOPE.md` | 2026-08-30 |
| D2 | No-touch list narrowed to 100, 101, 102, 103 | `core/src/safety.rs`, pinning test | 2026-08-30 |
| D3 | VM 101 keeps its VM lifecycle untouchable; HA-internal config changes only with per-change permission | `SCOPE.md` C8 | 2026-08-30 |
| D4 | CT 107 is cleaned up; CT 111 is kept and integrated | `SCOPE.md` | 2026-08-30 |
| D5 | Scratch containers 190/191 cleaned up after coordinating with pipeline-v2 | `SCOPE.md` | 2026-08-30 |
| D6 | Project documents live in `docs/deployment/` in this repo | `SCOPE.md` | 2026-08-30 |
| D7 | Inter-service configuration uses LXC IPs, not public URLs | `SCOPE.md` C3 | 2026-08-30 |
| D8 | Almanac joins the scope; it already runs on CT 112 as an adopted native service | `SCOPE.md` G3 | 2026-08-30 |

## Findings (F)

| # | Finding | Impact | Status |
|---|---|---|---|
| F1 | A working monitoring stack (Alertmanager, cadvisor ×6, Grafana datasource + 3 dashboards, node/cadvisor/almanac scrape jobs, SMART collector on the PVE host) exists on the machines and in no repository | The next deploy from this repo silently reverts all of it | open |
| F2 | CT 107 runs no docker and no mosquitto — only sshd and node_exporter | 8 GB allocated to an empty container | open |
| F3 | CT 190 and 191 hold 10.10.10.14 and .15, the addresses a new CT 114/115 would take under the vmid-to-last-octet convention | Blocks the convention for new stacks | open |
| F4 | A2's hostname guard, not the no-touch list, is what still refuses the legacy stacks (`lxc-media-stack` etc. are not `<vmid>-app-<stack>`) | Narrowing the no-touch list did not make them deployable | done |
| F5 | Two drives are old: Toshiba DT01ACA200 at 110 329 power-on hours, WD40EFRX at 63 194 — both pass SMART with zero reallocations | Justifies the pending-sector alert rule; a backup target choice must not assume these drives | open |
| F6 | Grafana runs with `GF_SECURITY_ADMIN_PASSWORD=changeme_secure_password` and 10.10.10.4:3000 answers on the LAN with no gate | Must be treated as leaked; see §6 of the vault note "Homelab Open Issues" | open |

## Tasks (T)

| # | Task | Depends on | Status |
|---|---|---|---|
| T1 | Phase 1 · full inventory of every guest, its services and its hand-tuned configuration | D1 | doing |
| T2 | Bring `stacks/metrics/prometheus/prometheus.yml` level with the live one (almanac job, node job ×11, cadvisor job ×6) | T1 | open |
| T3 | Add Alertmanager + its four rules + the `rules/` mount to the metrics stack | T1 | open |
| T4 | Add node_exporter and the SMART textfile collector as managed artifacts | T1 | open |
| T5 | Add cadvisor to every docker host, not only the metrics stack | T1 | open |
| T6 | Add the Prometheus datasource and three dashboards to Grafana provisioning in the repo | T1 | open |
| T7 | Adopt the Recyclarr preset (vault mini-round MR-2) | T1 | open |
| T8 | Write the HTTPSwitchboard preset and decide its container (S2) and config location (S3) | T1 | open |
| T9 | Deploy kyu-runner: HA automation first, then route, then smoke test | T8, T3 | open |
| T10 | Golden LXC image with docker, unattended-upgrades, node_exporter, cadvisor, promtail | T1 | open |
| T11 | Extend C7 so the orchestrator can create a container and install a Rust binary into it | T1 | open |
| T12 | Full backup to the attached HDDs plus a completed Google Drive run, restore drilled | everything above | open |
| T13 | Clean up CT 107 | T12 | open |
| T14 | Clean up CT 190 and 191 after coordinating with pipeline-v2 | T12 | open |
| T15 | Rename the stale worktree branch `hungry-elbakyan-85a663` under `.claude/worktrees/` to something descriptive, or remove it | — | open |

## Inherited open items (from before this project)

These were put to Kenny in earlier forms and never closed. They are carried
here so they cannot evaporate; each becomes a Phase 2 feature question or a
mini-round rather than a re-asked form.

| # | Item | Status |
|---|---|---|
| B1 | U1-U3 · who owns updates for the adopted native services (almanac observes its own reverts; kyu has no update mechanism and no binary release asset) | open |
| B2 | R1-R5 · the C7 milestone report was never signed off | open |
| B3 | V1-V5 · v1-tree cleanup (six dead components, `stacks-backup/`, `install-host-service.sh`, branch `v2-merge`) | open |
| B4 | Grafana service-account token (MB5) needed for the datasource, dashboards and the `kyu_sweeper_age_ms` alert | waiting on Kenny |
| B5 | D5 mirror remote + deploy key | waiting on Kenny |
| B6 | H2 OPNsense API credentials | waiting on Kenny |
| B7 | Broken-release rollback drill for C7 — needs a deliberately broken kyu release | parked |
