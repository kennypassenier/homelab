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
| Phase | 9 · Released — v3.41.0 live on the host | **7 · Hardening — 22 of 23 gate gaps closed (G6 deferred by Kenny). Read `docs/deployment/RESUME.md` for what is in flight** |
| Frozen | features, architecture | scope, features, tech choices, architecture |
| Resume from | `docs/REALIZATION_PLAN.md` | **`docs/deployment/REGISTER.md`** — every decision, finding and task is numbered there; the Phase-7 gate log lives in `REALIZATION_PLAN.md` |

**The deployment project is the active work.** It brings the whole fleet under
the orchestrator: one inventory, one target layout, one proven backup, then
container-by-container replacement. Read `docs/deployment/REGISTER.md` first —
it is the resume point and is kept current as part of the work, not afterwards.

## Project state (resume here)

- **Released and live at v3.41.0** (2026-09-02); 432 tests, CI green — and green now
  means something: CI ran without `--locked` until that day, so it built
  whatever crates.io served rather than what the lockfile pins (F235).
  The deployment project is what moves now — see `docs/deployment/REGISTER.md`.
  M7 is done: CT 115 destroyed and rebuilt end to end, 653 s of outage of
  which 573 s was one stalled image pull (F108). W1-W3 built straight after
  it (host hardware readiness, per-stack retention, boot-policy drift).
  Open Dependabot PR that does NOT pass: axum 0.7 → 0.8, a real breaking
  upgrade; main is green.
  v3.0.0 was the first tag (Kenny's number: "hele nieuwe rewrite").
  Features added after the hardening batch:
  - **H7 · release-driven host updates** — TUI badge + `U` key +
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
  rollback proven). Golden templates = CT 998 (v3 unprivileged, the default) and CT 997 (v3
  privileged, for media + downloader); CT 999 is the retired v1.
- **There is no standing test container any more.** vmid 108 used to be it;
  since the pilot it is `108-app-syncthing` — which, measured 2026-09-01, is
  running and synchronising NOTHING: zero folders, zero devices, 120 KB on
  disk (F163). This note claimed it held Kenny's Obsidian vault sync, twice,
  on the strength of what the M4 pilot was FOR rather than what it ended up
  doing. Do not destroy it on that basis either — what it should do is
  Kenny's open decision. This note said otherwise until 2026-09-01, and a form went out
  recommending drills on a live service because of it. When something has to
  be created and destroyed for real, make a throwaway stack on a free vmid
  (`stacks/drill`, vmid 118) and destroy it in the same sitting — Kenny's
  form B1. LVM snapshot `pve/root-v2-preinstall` is the host-OS rollback net.
- **No-touch list is law**: `core/src/safety.rs` (VMs 100/101/201–203,
  CT 102/103, and 104–107/111 until migration).
- **Backups (audited 2026-08-27)**: nightly restic per stack +
  `host-meta-config` repo (vault, state.json, TLS, intent repo) — restore
  drill green. Kenny's restic password is in Bitwarden (verified).
  E8 replicates HDD2TB/HDD4TB to `HDD18TB/replica/`; the legacy
  `HDD18TB/REPLICA_*` datasets are frozen history, media pools are
  deliberately out of scope.
- **Pre-test safety net (remove when testing is done)**: LVM snapshot
  `pve/root-pretest` (8G — a full snapshot is an invalid rollback, check
  `lvs pve`) and `vzdump-lxc-108-2026_08_27-18_10_04.tar.zst`. VG `pve`
  is 100% allocated; only new LVM volumes are blocked, containers have
  ~631G on local-lvm plus TBs on the ZFS pools.
- **Open**: M5 migration (after the gridsim demo; procedure in
  docs/MIGRATION_INVENTORY.md), CT 107/111 decommission (needs explicit
  go), Kenny's own test pass (docs/TEST_PLAN.md), phase-10 retro for
  E8/G9/H7/H8, `pve/root-v2-preinstall` cleanup once v3 has proven
  itself. HTTPSwitchboard preset adoption — S1 decided (policy=manual),
  the container and the config location wait for the deployment plan;
  verified facts in the vault note "Homelab HTTPSwitchboard Deployment".
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

## Project documents

| Doc | Purpose |
|---|---|
| docs/SCOPE.md | goals, non-goals, constraints (Phase 0, retro-fitted) |
| docs/INVENTORY.md | brownfield sweep + flaw list (Phase 1, retro-fitted) |
| docs/FEATURES.md | rated feature list, permanent IDs A1–H6 (Phase 2, frozen) |
| docs/ARCHITECTURE_DECISIONS.md | AR1–16, frozen (Phases 3–4) |
| docs/REALIZATION_PLAN.md | milestones M0–M6 + status (Phase 5) |
| docs/TEST_PLAN.md | per-feature test steps, offline + live (Phase 7) |
| docs/USER_GUIDE.md · DEBUGGING_GUIDE.md · OPERATIONS_RUNBOOK.md · ARCHITECTURE_REFERENCE.md | Phase 8 set |
| docs/MIGRATION_INVENTORY.md | M5 migration completeness contract |
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

3. **branch protection** on `main` (2026-08-28): the `check` and `msrv`
   CI jobs are required. `enforce_admins` is deliberately off so
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
