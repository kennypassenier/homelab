# Legacy documentation

Nothing in this directory describes the system as it runs today. Current
documentation lives one level up in `docs/` and in `docs/deployment/`.

## Top level: the pre-v2 system

The files directly in this directory describe the OLD three-binary system
(CLIENT + HOST + per-container LXC daemon, latch secrets, GHCR images) that
the v2 rewrite replaced in August 2026. `ui-guidelines.md` belongs here too:
it set rules for the `client-app` Ratatui binary, which no longer exists.
`config.env.example-pre-v2` was `config/.env.example` until 2026-09-27: every
key in it (LXC daemon, OPNsense sync, GitOps, v1 latch) belongs to the old
system, and nothing in the current code reads it (gap-29).

## Finished work of the current system

Moved here on 2026-09-27 during Phase 8 of the deployment project (Kenny's
form answer `legacy-move: Verplaatsen`). Each file was accurate for the work
it served and that work is done; links elsewhere point here now.

| Directory | What it holds | Finished |
|---|---|---|
| `v2-build/` | `INVENTORY.md`, the brownfield sweep that fed the v2 feature list; `V2_PILOT_HANDOFF.md`, the handoff written when the v2 pilot paused | 2026-08-11 and 2026-08-10 |
| `m8/` | `MIGRATION_INVENTORY.md`, the mount-by-mount migration contract, and the four pre-flights and baselines taken before CT 104, 105, 106 and 111 were rebuilt | the migration milestone closed 2026-09-01 |
| `deployment-project/` | `OPNSENSE_API.md` (the key the orchestrator no longer uses since OPNsense left it), `HANDOVER_KYU_RELEASE.md` and `HANDOVER_LATCH_RECOVERY.md` (handovers to other sessions, acted on) | 2026-09-02 to 2026-09-20 |
