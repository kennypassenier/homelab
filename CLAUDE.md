# homelab v2

Two-binary Rust homelab orchestrator: CLIENT (CLI + TUI) on the desktop,
HOST daemon on Proxmox (10.10.5.250), one TLS-pinned WS line between them.

This project follows the dev procedure in `~/Projects/dev-procedure/`
(`/project-flow`). Standing rules apply to every change:
`~/Projects/dev-procedure/STANDING_RULES.md`.

## Before anything else: three rules that keep breaking

Kenny has had to repeat these four times in one evening (2026-08-31). They
are not style preferences; ignoring them costs him the ability to steer.

1. **Every choice Kenny makes is ONE interactive form**, however small, per
   `~/Projects/dev-procedure/FORM_PROTOCOL.md` — re-read fresh from disk each
   time. The trigger that keeps being missed: **a question from Kenny that
   contains a choice is a form, not a task.** The moment I have just measured
   something and can see the fix is exactly the moment to stop and build the
   form instead.

2. **A decision Kenny has already made is not mine to defer, narrow or
   re-time.** If new information makes his answer look wrong — it is late, the
   risk changed, I am unsure — that is a new form item, not a paragraph
   explaining what I decided instead. Reporting a unilateral change of plan in
   prose is the same violation as never asking.

3. **My answer text may not contain a question mark aimed at Kenny.** No
   "shall I…", no "say the word and I'll…". Reporting belongs in prose;
   choosing never does. This one is deliberately FORMAL rather than
   substantive, because rule 1 requires me to *recognise* that I am putting a
   choice — and that is exactly the judgement that fails when something is
   running in the background. On 2026-09-01 all five prose-borne choices fell
   in that state, and one of them killed a backup Kenny never asked to stop.
   Second half: while a form is unanswered I start no new live action on the
   machines; code, tests and documents continue.

Both failures look identical from his side: he answered, and then had to
argue with the answer.

## Procedure status

Two projects live in this repo, each with its own phase track.

| | Orchestrator (homelab v3) | **Deployment project** |
|---|---|---|
| Docs | `docs/*.md` | `docs/deployment/*.md` |
| Phase | **10 · Retrospective — done 2026-09-26** (`docs/RETROSPECTIVE.md`); **v3.57.1** released 2026-09-27 (client fixes from the part-A run; the host still runs v3.57.0, host code unchanged) | **9 · Release & lifecycle — entered 2026-09-27** (Phase 8 documentation closed the same day). Read `docs/deployment/RESUME.md` for what is in flight |
| Frozen | features, architecture | scope, features, tech choices, architecture |
| Resume from | `docs/REALIZATION_PLAN.md` | **`docs/deployment/REGISTER.md`** — every decision, finding and task is numbered there; the Phase-7 gate log lives in `REALIZATION_PLAN.md` |

| | |
|---|---|
| Next action | **Orchestrator: retrospective form answered 2026-09-26** (all six as recommended; `docs/RETROSPECTIVE.md` §4). Part A run headless the same evening (`docs/TEST_RUN_PART_A.md`, Kenny: Akkoord); the dev-procedure diff is on its main (a25f706); all eight findings fixed with tests first and released as v3.57.1 (installed on WSL from the verified release asset; Garuda is on the GARUDA.md list). **Deployment: Phase 8 closed 2026-09-27** (all six documents approved on the second form; 24 Mermaid diagrams added on Kenny's remark, `make diagrams`; fix-34..38 and most gaps from the rewrites fixed, host and client on v3.58.6). **Phase 9 entered the same day; its form answered (step-20): the rollout is complete; msrv, env-check, drill-monitor and inbox-guards done; host and client on v3.58.10.** The env check found a live leak, fixed and measured (fix-39: kyu tokens in deploy transcripts). fix-39 closed (bundles masked, no rotation). Kenny asked for everything to be declarative (step-21, step-22): the seeder now keeps Uptime Kuma equal to the files (committed locally, 0ffae9c and later). Built locally the same day with tests first and documented: step-22 (declarative metadata), ask-8 (apply, orphan and unit removal, native units, mounts), ask-9 (retired stacks kept, `homelab wipe`). Pushed on Kenny's go and live on host and client as v3.59.3 (2026-09-27): declarative cleanup, `homelab apply`/`wipe`, fix-40 (containers older than their files are restarted), the seeder keeping Uptime Kuma equal to the files (39 of 39 tagged, drill monitor gone). `homelab check` now shows only noted items, kp-soft.dev `nok` (by design) and the open manual checks. The nine-lens expert panel (Kenny, Phase 9 form: De negen) reported 2026-09-27 ~16:45 local; its reports are merged into 125 findings (3 fixed: fix-41, fix-42). fix-41 (route retirement, TUI SHIFT+D) and fix-42 (retention kept ~8 days; history before 2026-09-11 is gone) live in v3.59.4 since 17:04 local. fix-43: a hand-run `homelab-host --version` started a second daemon and the unit failed; restored under systemd 17:21 with Kenny's go, the binary now refuses unknown arguments (not yet released). **Next: the triage form for the 122 open panel findings (Kenny), then Phase 10.** Waiting on the clock: fix-42 measurement after the nightly of 2026-09-28. Also 10 manual checks `homelab check` reports open. All manual checks answered 2026-09-27 (paperwork ×2, media posters, kyu-e576 ok; kp-soft.dev `nok` stays by design, D56). v3.58.0 live since 2026-09-27 06:40 local: a deploy never replaces an installed native binary (fix-28, measured), and native releases must carry the ecosystem minisign signature (fix-29; the orchestrator's own releases stay checksum-only, stated gap). CT 109 runs kyu 4.0.1, kyu-runner 1.0.1, http-switchboard 3.2.1; CT 112 almanac 4.0.6 (all signed, rolled out 2026-09-27 by the signing thread through `homelab`). Every CT rollout goes through Homelab Rust (Kenny's standing rule, 2026-09-27). **Waiting on the clock, not on Kenny:** 2026-09-28 — read whether kyu archived the idle `ha` subscription on `notify.kenny` (due ~22:38 local on 2026-09-27; 28,335 held messages lapse); the next native release that appears unsigned must be skipped (fix-29); fix-27 measured and closed 2026-09-27 (Loki restart deploy passed on its own); fix-30 measured and closed 2026-09-27 (forced reinstall of http-switchboard 3.2.1 exited 0). Waits for a first escrow step: gap-15 guard. |


**The deployment project is the active work.** It brings the whole fleet under
the orchestrator: one inventory, one target layout, one proven backup, then
container-by-container replacement. Read `docs/deployment/REGISTER.md` first —
it is the resume point and is kept current as part of the work, not afterwards.

## Project state (resume here)

- **Released and live at v3.59.4** (2026-09-27 17:04 local: fix-41, fix-42); v3.59.3 (2026-09-27; v3.59.0 declarative cleanup with `homelab apply` and `homelab wipe`, v3.59.1-3 fix-40 and the no-docker guard fact); v3.58.10 (2026-09-27; v3.58.7 MSRV in make release, doctor env check, inbox unmeasured; v3.58.8-10 fix-39 secret-free transcripts, container-only .env and adopted env files sealed into the vault); v3.58.6 (2026-09-27; v3.58.5 gap-20/32, v3.58.6 no docker guards on adopted stacks); v3.58.4 (2026-09-27 afternoon: gap-19/21/22/24/25/27/28/29/33 from the Phase 8 rewrites); v3.58.3 (2026-09-27 ~12:25 local, fix-34..38: TLS signature check, notify token off argv, exec honours the configured no-touch list, per-unit vault copies, quoted transcripts); v3.58.2 at 11:27 local (fix-32: no credential in curl argv); v3.58.1 at ~09:40 local (fix-30: no 40 MB transcript lines); v3.58.0 at 06:40 local (fix-28/29); v3.57.0 on 2026-09-26 23:22 local (fix-26/27); v3.56.0 the same evening (fix-24 log rotation); v3.55.0 on 2026-09-20 01:47 local; 3.53.0 and 3.52.0 on 2026-09-19, 3.51.0 on 2026-09-11); **552 tests in 36 suites** (counted 2026-09-27 at v3.57.1), CI green — and green now
  means something: CI ran without `--locked` until that day, so it built
  whatever crates.io served rather than what the lockfile pins (F235).
  The deployment project is what moves now — see `docs/deployment/REGISTER.md`.
  M7 is done: CT 115 destroyed and rebuilt end to end, 653 s of outage of
  which 573 s was one stalled image pull (F108). W1-W3 built straight after
  it (host hardware readiness, per-stack retention, boot-policy drift).
  The open Dependabot bumps, axum 0.7 → 0.8 among them, were combined into
  one change and merged on 2026-09-25 (674438f); no Dependabot PR is open.
  v3.0.0 was the first tag (Kenny's number: "hele nieuwe rewrite").
  Features added after the hardening batch:
  - **H7 · release-driven host updates** — TUI badge + `u` key +
    `homelab release-update`; downloads the GitHub release, verifies the
    checksum, feeds it into the H5 self-update pipeline. Live-proven.
  - **H8 · per-stack enabled flag (light)** — `homelab enable|disable`,
    TUI `E`, `[OFF]` badge. Disabled = nightly runs skip it + onboot
    cleared; never starts/stops containers; auto-disables after a failed
    nightly run.
  - **E8 · ZFS snapshots + replication** — absorbed from the retired
    `/root/full_zfs_backup.sh` cron script. Jobs in host.toml, runs in
    the nightly plan, refuses to re-seed over a populated target.
  - **G9 · own Rust services via GHCR** — `templates/rust-service/` +
    `presets/rust-service/`; no orchestrator code.
  - **H10 fix** — the host-meta backup existed but was never called;
    now part of `nightly_plan()`, and it carries the intent repo too.
  - **E8 · ZFS snapshots + replication** (absorbed the dead cron script),
    **D12 · secrets via latch** (`latch_secrets` + `latch cat --expand`,
    live-proven B25), **F4 · metrics stack live on CT 113** (Prometheus +
    pve-exporter; Grafana coupling awaits Kenny's token), **C7 · native
    services** (adopt/backup-native/update-native; CT 109 kyu and
    CT 112 almanac adopted live, B27; broken-release rollback drill
    pending). Stack files: compose stacks have
    `lxc-compose.yml`, native services `service.yml`.
- **Host daemon LIVE** on Proxmox as `homelab-host.service` (:8443, TLS
  fp SHA256:85:00:F8:84…); ships via `homelab self-update` (H5, armed
  rollback proven). Golden templates: **CT 996 `debian-13-homelab-v4` (unprivileged) and CT 995
  `debian-13-homelab-v4-priv`**, both built 2026-09-10 and carrying the fixed
  unattended-upgrades config; CT 998 and CT 997 are the Debian 12 pair they
  replace and are kept until the fleet has moved. CT 999 is the retired v1.
  `template-build` derives the name from the base template, so a Debian 14
  pair names itself.
- **There is no standing test container any more.** vmid 108 used to be it;
  since the pilot it is `108-app-syncthing` — which, measured 2026-09-01, is
  running and synchronising NOTHING: zero folders, zero devices, 120 KB on
  disk (F163). This note claimed it held Kenny's Obsidian vault sync, twice,
  on the strength of what the M4 pilot was FOR rather than what it ended up
  doing. Do not destroy it on that basis either — what it should do is
  Kenny's open decision. This note said otherwise until 2026-09-01, and a form went out
  recommending drills on a live service because of it. When something has to
  be created and destroyed for real, make a throwaway stack on a free vmid
  (`stacks/drill`, vmid 119 since 118 went to inbox) and destroy it in the same sitting — Kenny's
  form B1. There is no host-OS rollback net: the two invalid LVM
  snapshots were removed 2026-09-26 (see below).
- **No-touch list is law**: `core/src/safety.rs` (VM 100 OPNsense, VM 101
  Home Assistant, 102, 103; narrowed 2026-08-30 — the legacy stacks come
  under management through the deployment project).
- **Backups (audited 2026-08-27)**: nightly restic per stack +
  `host-meta-config` repo (vault, state.json, TLS, intent repo) — restore
  drill green. Kenny's restic password is in Bitwarden (verified).
  E8 replicates HDD2TB/HDD4TB to `HDD18TB/replica/`; the legacy
  `HDD18TB/REPLICA_*` datasets are frozen history, media pools are
  deliberately out of scope.
- **No host-OS snapshots** (removed 2026-09-26 21:15 UTC with Kenny's go,
  retrospective item root-snapshots): `pve/root-pretest` and
  `pve/root-v2-preinstall` were both invalid at 100%. `vgs pve` after
  removal: 16.00g free, `pve/root` untouched. The CT 108 pre-test vzdump
  no longer exists either.
- **Open**: nothing left of the phase-10 retrospective
  (`docs/RETROSPECTIVE.md`); Kenny's own test pass is closed (part A run
  headless and signed off 2026-09-26, part B closed as an open item). The old
  M5 migration milestone is carried by the deployment project
  (`docs/deployment/REGISTER.md`). HTTPSwitchboard preset adoption — S1
  decided (policy=manual), the container and the config location wait for
  the deployment plan; verified facts in the vault note "Homelab
  HTTPSwitchboard Deployment".
- **Awaiting Kenny**: D5 mirror remote+deploy-key, H2 OPNsense API creds,
  F4 PVE token.

### 2026-09-02 evening — what changed on the machines

- **The whole fleet left promtail** (F249, F256). It reached end of life on
  2026-03-02. All thirteen containers now run Grafana Alloy, installed with
  apt from Grafana's signed repository so unattended-upgrades keeps it
  patched. The two native containers (kyu CT 109, almanac CT 112) shipped
  **no logs at all** before this. Every stack was verified by querying Loki
  afterwards, not by reading its deploy transcript — which is how three
  faults were found that no deploy reported (F254, F255).
- **`homelab` is installed** at `~/.cargo/bin/homelab` (`make install`) and
  reads `~/.config/homelab/env`. Before this every `homelab <verb>` in every
  document here was a command nobody could run (F240, F253).
- **`make release` has `DRY=1`** and refuses to ship from a red base (F251,
  F252). Branch protection does not cover direct pushes and that is Kenny's
  call, not a defect to fix behind his back.
- **The nightly round gained readers**: a restore drill that refuses to be
  satisfied by empty files (F229), the manual checks Kenny answers with
  `homelab checks answer` (F221), a notification path that notices its own
  failure (F222), half-deployed stacks (F220), and the Uptime Kuma seeder's
  verdict about monitors that outlived their stack (F243).
- **almanac v1.5.0** live (F257).
- **The router's own configuration is backed up** (F259), which it never was.
  `homelab backup-devices` runs it on demand; it also rides the nightly round.
  `homelab check` now reports no broken findings across the whole fleet.

## Project documents

| Doc | Purpose |
|---|---|
| docs/SCOPE.md | goals, non-goals, constraints (Phase 0, retro-fitted) |
| docs/legacy/v2-build/INVENTORY.md | brownfield sweep + flaw list (Phase 1, retro-fitted; archived 2026-09-27) |
| docs/FEATURES.md | rated feature list, permanent IDs A1–H6 (Phase 2, frozen) |
| docs/ARCHITECTURE_DECISIONS.md | AR1–16, frozen (Phases 3–4) |
| docs/REALIZATION_PLAN.md | milestones M0–M6 + status (Phase 5) |
| docs/TEST_PLAN.md | per-feature test steps, offline + live (Phase 7) |
| docs/USER_GUIDE.md · DEBUGGING_GUIDE.md · OPERATIONS_RUNBOOK.md · ARCHITECTURE_REFERENCE.md | Phase 8 set |
| docs/legacy/m8/MIGRATION_INVENTORY.md | M5 migration completeness contract (finished; archived 2026-09-27) |
| docs/PRESET_GUIDE.md · LLM_COMPOSE_CONVERSION.md | preset catalog how-to |

## Gates (enforced)

Two layers, both running `.claude/hooks/gates.sh` (fmt, clippy -D
warnings, full suite) and both demanding IDs in brackets (`[B4]`,
`[AR9]`, `[meta]`):

1. **git-native** — `.githooks/pre-commit` + `commit-msg`, wired with
   `git config core.hooksPath .githooks`. Holds from ANY session,
   terminal or tool. **One-time per clone: `make hooks`** (core.hooksPath
   is local config and is never committed). Ratified by Kenny 2026-08-28:
   full suite on every commit, `--no-verify` stays as a documented
   escape, merge/revert/fixup/squash exempt from the ID rule.
   Human-facing docs: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).
2. **session hook** — `.claude/hooks/check-commit.sh` (PreToolUse on
   Bash), which only loads in a session opened in this directory.

CI re-runs the same gates on every push; red blocks merge. Layer 1 was
added 2026-08-28 after v3.0.1–v3.1.1 were committed from a session opened
elsewhere, where layer 2 silently did not load.

3. **branch protection** on `main` (2026-08-28): the `check` CI job is
   required (the `msrv` job was removed 2026-09-10, d318ada; nothing checks
   the declared rust-version 1.88 since, gap-30). `enforce_admins` is deliberately off so
   `make release` can still push directly; a red gate blocks any merge.

## Build & ship

```bash
cargo test --workspace                       # 211 tests
docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target-debian \
  rust:1-bookworm cargo build --release -p homelab-host
make release VERSION=x.y.z                   # gate, tag, push; CI publishes
homelab release-update                       # roll out to the host (H7)
```
`.env` holds HOMELAB_HOST/HOMELAB_TOKEN (source it: `set -a; . ./.env; set +a`).
