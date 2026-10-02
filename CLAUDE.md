# homelab v2

Two-binary Rust homelab orchestrator: CLIENT (CLI + TUI) on the desktop,
HOST daemon on Proxmox (pve), one TLS-pinned WS line between them.

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

4. **New infrastructure on Kenny's PC needs his approval first** (Kenny,
   2026-10-02): a second WSL distro, a VM, a Windows or systemd service, a
   scheduled task, and kernel, binfmt, mount or network settings are proposed
   in ONE form saying what, why and the risks, and built only after his go.
   Every helper brief carries this rule.

5. **Every test run reports how long it took** (Kenny, 2026-10-02), measured
   and per run (full, carried or rerun) and per project; the gate and release
   scripts print it themselves.

6. **Web pages use grid structures** (Kenny, 2026-10-02): elements align
   with each other and keep the same space in every state (loading, empty,
   error, filled); loose flex or stacked layouts are converted.

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
| Next action | **3.70.1 live everywhere (signed, measured 2026-10-02); home (CT 115) and uptime (CT 107) destroyed with backups, restore proven afterwards (CORRECTIONS 2026-10-02). 3.70.2 being gathered on main: plan-violation never fails an op (fix-171), destroy restore-check (fix-190), host.toml apply guard + reconciled config/host.toml (fix-191), job dialog grid / Pause-Stop / steps-only bar (feat-jobpanel-1, fix-188/189), invariants list. Do NOT save the dashboard's Settings page or run `homelab host apply` before 3.70.2 is live on pve. Rollout remainder after Kenny's go: firewalls on 11 stacks one at a time, CT 120 rebuild drill, inbox deploy, restore drill, measurements.** **3.70.0 release in progress (Kenny's Go 2026-10-01 18:45; drill-restore: permission to copy restic.pw + rclone.conf to a throwaway nested VM; TUI host-settings tab removed; the ~/Projects cap failed and moved to chassis-rs).** **3.70.0 is the last feature release** (Kenny, 2026-10-01): it closes every open item from today's batch — fix-62..169 (release, restore-drill coverage, firewall and token fixes), gap-23/26/31/34, T62, the admin dashboard's backup-secrets and visuals milestones, the chassis-rs 3.1.0 nav rewrite, and rule 20 (nothing may balloon, see `docs/deployment/REGISTER.md` ask-10). **What follows 3.70.0:** release it, roll it out to the host and CT 120, turn on firewalls on the eleven stacks still without one, then measure (fix-66/68/70/142/143/146 and the rest of today's batch all carry a "doing: release" style residual that the rollout must close out in `REGISTER.md`). **After that, this project takes bugfixes only** — no further feature rounds are planned. See `docs/deployment/REGISTER.md` for the row-by-row state and `docs/deployment/CORRECTIONS.md` for today's ratified corrections (O10-twice, the two-sessions fault, fix-161). |


**The deployment project is the active work.** It brings the whole fleet under
the orchestrator: one inventory, one target layout, one proven backup, then
container-by-container replacement. Read `docs/deployment/REGISTER.md` first —
it is the resume point and is kept current as part of the work, not afterwards.

## Project state (resume here)

- **v3.69.0 is live; v3.70.0 is the last feature release for this project**
  (Kenny, 2026-10-01) — it bundles today's batch (fix-62..169, gap-23/26/31/34,
  T62, the admin dashboard's backup-secrets and visuals milestones, the
  chassis-rs 3.1.0 nav rewrite, rule 20). What follows is the release itself,
  the rollout to host and CT 120, firewalls on the eleven stacks that still
  lack one, and the measurements each of today's rows is waiting on; after
  that this project takes bugfixes only. Full row-by-row state is
  `docs/deployment/REGISTER.md`, not this file.
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

There is no GitHub Actions CI since 2026-09-29 (Kenny: every build and
check runs locally); `make check` and `make release` run what it ran. Layer 1 was
added 2026-08-28 after v3.0.1–v3.1.1 were committed from a session opened
elsewhere, where layer 2 silently did not load.

3. **branch protection** on `main` (2026-08-28): it required the `check` CI
   job, which no longer exists since 2026-09-29; whether that requirement is
   dropped is Kenny's call (local-builds form). `enforce_admins` is off so
   `make release` can push directly.

## Build & ship

```bash
cargo test --workspace                       # 211 tests
docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target-debian \
  rust:1-bookworm cargo build --release -p homelab-host
make release VERSION=x.y.z                   # scan, gate, tag, build, push, publish
homelab release-update                       # roll out to the host (H7)
```
`.env` holds HOMELAB_HOST/HOMELAB_TOKEN (source it: `set -a; . ./.env; set +a`).
