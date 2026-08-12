# homelab v2

Two-binary Rust homelab orchestrator: CLIENT (CLI + TUI) on the desktop,
HOST daemon on Proxmox (10.10.5.250), one TLS-pinned WS line between them.

This project follows the dev procedure in `~/Projects/dev-procedure/`
(`/project-flow`). Standing rules apply to every change:
`~/Projects/dev-procedure/STANDING_RULES.md`.

## Procedure status

| Field | Value |
|---|---|
| Current phase | 7 · Hardening COMPLETE (H1–H22 closed per Kenny's form; H9 later, H22 accepted) |
| Last completed gate | Procedure-evaluation form (2026-08-11): V1–V10 close, V11 later, V12 retro |
| Next gate | OPEN: deep-dive 2 (B6 update-flow + B8 enabled-flag). V12 retro DONE (dev-procedure updated, L1-L8). V11 release tag ready when Kenny says go |
| AFK mode | on for build work (Kenny: "keep going"), off for gates |

## Project state (resume here)

- **Feature-complete + HARDENED at v2.6.0** (2026-08-12): all features +
  phase-7 hardening (3 audits; 20 gaps closed incl. E3 auto-restore, D3
  GC, real diff plans, real-git E2E, configurable safety list, fail-loud
  state, real doctor probes, host-meta backup); 117 tests; CI green.
  First autonomous nightly run succeeded 2026-08-12 04:04 (snapshot
  ecf42ecf + tiered retention prune).
- **Host daemon LIVE** on Proxmox as `homelab-host.service` (:8443, TLS
  fp SHA256:85:00:F8:84…); ships via `homelab self-update` (H5, armed
  rollback proven). Golden template = CT 999 (`clone:999` default).
- **vmid 108** = dedicated automated-test container (create/destroy
  allowed there and only there). LVM snapshot `pve/root-v2-preinstall`
  is the host-OS rollback net.
- **No-touch list is law**: `core/src/safety.rs` (VMs 100/101/201–203,
  CT 102/103, and 104–107/111 until migration).
- **Open**: M5 migration (after gridsim demo Aug 15–31; procedure in
  docs/MIGRATION_INVENTORY.md), CT 107/111 decommission (needs explicit
  go), Kenny's own test pass (docs/TEST_PLAN.md), phase-7 hardening
  results, v2 release tag (deliberately after hardening), phase-10 retro.
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

Commits are blocked by `.claude/hooks/check-commit.sh` unless
`.claude/hooks/gates.sh` passes (fmt, clippy -D warnings, full suite) and
the message carries IDs in brackets (`[B4]`, `[AR9]`, `[meta]`). CI
re-runs the same gates on every push; red blocks merge.

## Build & ship

```bash
cargo test --workspace                       # 83 tests
docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/w/target-debian \
  rust:1-bookworm cargo build --release -p homelab-host
homelab self-update target-debian/release/homelab-host   # never scp
```
`.env` holds HOMELAB_HOST/HOMELAB_TOKEN (source it: `set -a; . ./.env; set +a`).
