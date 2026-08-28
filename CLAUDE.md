# homelab v2

Two-binary Rust homelab orchestrator: CLIENT (CLI + TUI) on the desktop,
HOST daemon on Proxmox (10.10.5.250), one TLS-pinned WS line between them.

This project follows the dev procedure in `~/Projects/dev-procedure/`
(`/project-flow`). Standing rules apply to every change:
`~/Projects/dev-procedure/STANDING_RULES.md`.

## Procedure status

| Field | Value |
|---|---|
| Current phase | 9 · Released — v3.0.0 (first tag) through v3.1.1 live on the host; `main` IS the rewrite since 2026-08-28 (v1 tip kept as branch `v1-archive`) |
| Last completed gate | Pre-test backup round (2026-08-27): H10 fix, E8 build, LVM+vzdump safety net |
| Next gate | Kenny's own test pass (docs/TEST_PLAN.md part A/B), then phase 10 retro for E8/G9/H7/H8 |
| AFK mode | on for build work (Kenny: "keep going"), off for gates |

## Project state (resume here)

- **Released and live at v3.1.1** (2026-08-27); 135 tests, CI green.
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
- **Host daemon LIVE** on Proxmox as `homelab-host.service` (:8443, TLS
  fp SHA256:85:00:F8:84…); ships via `homelab self-update` (H5, armed
  rollback proven). Golden template = CT 999 (`clone:999` default).
- **vmid 108** = dedicated automated-test container (create/destroy
  allowed there and only there). LVM snapshot `pve/root-v2-preinstall`
  is the host-OS rollback net.
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
  itself.
- **Awaiting Kenny**: D5 mirror remote+deploy-key, H2 OPNsense API creds,
  F4 PVE token.

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
cargo test --workspace                       # 135 tests
docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target-debian \
  rust:1-bookworm cargo build --release -p homelab-host
make release VERSION=x.y.z                   # gate, tag, push; CI publishes
homelab release-update                       # roll out to the host (H7)
```
`.env` holds HOMELAB_HOST/HOMELAB_TOKEN (source it: `set -a; . ./.env; set +a`).
