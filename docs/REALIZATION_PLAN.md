# Realization plan

How the decided featureset (docs/FEATURES.md — 27 Must / 23 Should / 7 Could)
gets built on the decided architecture (docs/ARCHITECTURE_DECISIONS.md,
AR1–AR16). Six milestones; every milestone has a hard **definition of done**.
Nothing before M3 touches the Proxmox host — M0–M2 are pure software.

## Target structure (AR1)

```
homelab/
├── Cargo.toml            workspace, ONE version for everything (AR10)
├── proto/                wire types + envelope {v, topic, id, payload} (AR5)
├── core/                 zero-I/O domain logic:
│   ├── manifest/         schema v2 types + THE validator (D10 — one impl,
│   │                     imported by client and host)
│   ├── safety/           A1 whitelist, A2 hostname guard, A3 fail-closed
│   ├── pipeline/         Step trait + runner: transcript, verify gate,
│   │                     journal hook, byte counters (AR3 · B3 B5 G6)
│   ├── ops/              step lists per operation: deploy, backup, restore,
│   │                     destroy, patch, template-build, update
│   ├── executor.rs       Executor trait (AR2) — RealExecutor lives in host,
│   │                     MockExecutor in core's test support
│   ├── state/            typed JSON stores, schema_version, atomic writes
│   │                     (AR4): applied hashes B4, digests B6, journal B5
│   ├── plan/             change-plan computation (D6)
│   ├── templates/        minijinja engine + embedded defaults (AR8, D7/D8)
│   ├── capacity.rs       commitment math (C6)
│   ├── runbook.rs        DR-runbook generator (E7)
│   └── error.rs          thiserror types + OperatorError w/ remediation (AR7)
├── host/                 daemon: axum WS + TLS (A4), RealExecutor, tracing
│   │                     (AR15), incidents (AR14), watchdog (B7), self-update
│   │                     (H5), schedulers (E4, D9 checks), config TOML (AR11)
├── client/               Elm-style TUI (AR6): model/, msg.rs, update/, view/,
│   │                     fx/ (ported from tui-preview), backend/ (WS + test),
│   │                     wizards (G2), palette (G3), focus modes, transfers
│   │                     viz (G6); thin CLI subcommands share the backend
├── stacks/               real stack manifests (syncthing first)
├── templates/            user-overridable minijinja templates (AR8)
└── docs/                 FEATURES.md · ARCHITECTURE_DECISIONS.md · this plan ·
                          V2_PILOT_HANDOFF.md · generated runbook (E7)
```

Legacy `client-app/`, `host-daemon/`, `lxc-daemon/`, `tui-preview/` stay
untouched until M5 archives them (tui-preview's fx/sim code is *ported into*
`client/` during M2, then the crate is archived too).

## Milestones

### M0 · Foundations — "the machine room" *(no infra needed)*
Workspace restructure to the tree above. Executor trait + MockExecutor (AR2).
Step-pipeline runner with transcript/gate/journal hooks (AR3). State stores
(AR4). Error model (AR7). tracing (AR15). TOML config (AR11). Port the MVP
host logic (safety gates A1–A3, runaway guards B2, bootstrap steps) onto these
foundations *with their FEATURES.md test scenarios implemented*. Hard CI
(AR9) + release workflow (AR10) live from day one.
**Done when**: full safety/idempotency test suite green in CI; a `deploy` runs
end-to-end against MockExecutor producing the exact golden command sequence;
tagging a commit produces a GitHub Release with Debian-compatible binaries.

### M1 · The line — protocol, TLS, validation, failure model *(no infra)* ✅ core done
AR5 envelope in proto. A4 TLS (rcgen self-signed + client pinning) and
required token. D10 validator wired into both sides. B5 journal + AR13
interrupt recovery. AR14 incident bundles. AR16 replay export. CLI
(`homelab ping|status|deploy|plan|doctor|incidents`) on the new protocol;
F5 health; F6 doctor (first checks).
**Done when**: in-process integration tests cover happy path + every failure
category (auth fail, pin mismatch, mid-operation disconnect, interrupted-op
recovery); a forced failure produces a complete incident bundle whose
transcript replays into a MockExecutor test.
**Status**: core + host + client built; 15 tests green (8 M0 + 7 M1:
AR14 bundle-with-replay, AR16 script extraction, AR13 interrupt detection,
F6 doctor matrix). A4 TLS end-to-end (pin mismatch refusal) verifies live
in M3 against the real host. `TracingExecutor` decorator makes command
transcripts flow through the sink → captured in bundles, replayable.

### M2 · The face — TUI rebuilt for real *(no infra)* ◑ mostly done
Elm-style client (AR6) with Backend trait; port the approved mockup visuals
(G1 fx engine, tabs, focus modes, logs UX) onto the real protocol; wizards G2
with live D10 validation; palette G3; plan preview D6; transfers viz G6 wired
to the transfer topic; C6 capacity panel.
**Done when**: TUI snapshot tests green for every screen; full deploy flow
runs against a scripted TestBackend including failure + incident display;
mockup parity checklist signed off by Kenny.
**Status**: build surface COMPLETE. `homelab tui` runs the Elm client over
the real wss+pinned protocol; `homelab tui --offline` runs a self-contained
DemoBackend for testing without infra. In: tabs (dashboard/stacks/logs/
doctor), splash, palette (G3), help, all fx, capacity panel (C6), live
DATA_TRANSFERS (G6), deploy focus window (SHIFT+D → task feed + flow +
incident-on-failure), change-plan preview (D6, [P]), new-stack wizard (G2,
[N]) that scaffolds a real deployable stacks/<name>/ tree. AZERTY-safe
throughout; modifiers spelled out. 27 tests (12 TUI snapshots + scaffold
roundtrip). **Remaining**: mockup parity sign-off by Kenny (run
`homelab tui --offline` in a real terminal).

### M3 · First contact — the syncthing pilot *(AFTER the gridsim demo, per-step go)*
Install HOST on Proxmox (unit from the handoff, TLS + token). Deploy
`stacks/syncthing`: C1 provision, bootstrap + B2 guards + A7, D1 push-sync,
C3 boot policy, H1 traefik route, F1 promtail, D8, B3 gates. E1+E3+E5 backups
with the fresh rclone token. B4 drift live. Verification checklist from
V2_PILOT_HANDOFF.md (incl. runaway-guard checks and the Loki label query).
**Done when**: syncthing hub syncs desktop+phone; backup → wipe → E3
auto-restore round-trip proven; `homelab doctor` all green; pilot runs 2 weeks
unattended with zero manual interventions.

### M4 · Full operations — everything that makes it professional ◑ in progress
C2 gated destroy [DONE, live-proven on 108] (then, with explicit go:
decommission CT 107 + 111 and the zombie host-daemon). E1 backup + E2 restore
[DONE, unit-tested; live needs E5 rclone token] + first drill. E4 scheduler.
D4 host git [DONE — /var/lib/homelab/repo] + D5 mirror. F3 HA notifications.
B6+D9 managed updates with per-app policy. B7 systemd watchdog. H5 self-update
live (first real release-to-host cycle). B8 golden template. E7 DR-runbook
generated. H6 fleet patching. A6 exec endpoint (built, default off). F6 doctor
[DONE, live].
**Live on Proxmox as of 2026-08-11** (vmid 108): daemon installed :8443,
deploy/idempotency/no-touch-refusal/gated-destroy/recreate all proven end to
end. Rollback net: LVM snapshot `pve/root-v2-preinstall`.
**Remaining M4**: E4 scheduler, D9/B6 managed updates + rollback, H5
self-update, E7 DR-runbook, F3 notifications, H6 fleet patching, A6 exec.
Known edge: container mounts set only at create, not reconciled on update.
**Done when**: every Must feature outside migration is live with its
FEATURES.md test scenario passing; a deliberate broken release proves the H5
watchdog; a restore drill and a DR-runbook tabletop review are logged.

### M5 · Migration and closure *(per-stack explicit go)*
Platform → media → downloader migrated (H4 hardware flags, A8 CrowdSec fixed
and proven, fixed NAS permissions). Ansible repo archived; legacy crates
archived; README + docs honesty pass (the old project's README fiction never
happens again — docs are part of each milestone's done-definition from M0 on).
Vault services queue (PBS project, metrics stack F4, Stirling/Mealie/Actual)
becomes ordinary D7 work afterwards.
**Done when**: all three stacks run under v2 with green doctor + drift-free
state for 2 weeks; ansible repo is archived; FEATURES.md shows every Must
shipped.

### M6 · Comprehensive documentation (Kenny's explicit final ask)
A full documentation set once features are implemented — every aspect, with
use cases and debugging guides:
- **User guide** per feature (keyed to FEATURES.md IDs) — what it does, how to use it.
- **Preset guide**: add/edit presets + their compose templates so new apps take
  a few keystrokes with no code change (goal: presets become editable data, not
  a recompiled const).
- **Debugging guide**: incident bundles, `commands.sh` replay, doctor, tracing
  levels, frame capture, common failures → fixes.
- **Operations runbook**: deploy/update/backup/restore/destroy day-to-day + the
  generated DR-runbook (E7).
- **Architecture reference**: the AR decisions distilled for a future maintainer.
- `docs/TEST_PLAN.md` (structured per-feature test steps, offline + live) —
  already written, kept current as features land.

## Standing quality rules (from M0, forever)

1. **Red CI blocks merge** (AR9) — no exceptions, including for me.
2. **Every bug becomes a test** before its fix merges (AR14 process).
3. **Every feature lands with its FEATURES.md test scenario** implemented, or
   the registry entry is updated with the reason why not.
4. **Docs move with code** — a milestone isn't done with stale docs.
5. **Feature/architecture IDs in commit messages** (e.g. `feat(core): A2
   hostname guard`).
6. **Proxmox is never touched outside an agreed milestone step**, and the
   no-touch list is eternal.
