# homelab v2

A two-binary Rust system that manages Kenny's homelab: a **CLIENT**
(CLI + cyberpunk TUI) on the workstation and a **HOST** daemon on the
Proxmox box, talking over a single TLS-pinned WebSocket line. Containers
run zero agent code; the host reaches in with `pct`. Stacks and presets are
plain files in this repo; every operation is idempotent, journaled,
fail-closed, and unit-tested against a mocked executor.

```bash
homelab tui              # the control deck (or: tui --offline to explore safely)
homelab deploy stacks/<name>
homelab --help-ish       # run with no args for the full verb list
```

**Status (2026-08-11): feature-complete at v2.5.0.** Every Must/Should/Could
feature from the registry is built and tested; the deploy → backup →
restore → update → rollback → self-update loop is live-proven on the real
host. Migration of the legacy stacks (M5) happens after the school demo.

## Documentation

Start here:

| Doc | What it answers |
|---|---|
| [docs/USER_GUIDE.md](docs/USER_GUIDE.md) | how do I use every feature? |
| [docs/OPERATIONS_RUNBOOK.md](docs/OPERATIONS_RUNBOOK.md) | what's the recurring work? |
| [docs/DEBUGGING_GUIDE.md](docs/DEBUGGING_GUIDE.md) | something failed — now what? |
| [docs/DR_RUNBOOK.md](docs/DR_RUNBOOK.md) | everything is down — now what? (regenerate: `homelab runbook`) |
| [docs/PRESET_GUIDE.md](docs/PRESET_GUIDE.md) | add an app to the catalog (two files, no code) |
| [docs/LLM_COMPOSE_CONVERSION.md](docs/LLM_COMPOSE_CONVERSION.md) | paste-into-an-LLM converter for vendor compose files |
| [docs/TEST_PLAN.md](docs/TEST_PLAN.md) | structured per-feature test steps (offline + live) |
| [docs/ARCHITECTURE_REFERENCE.md](docs/ARCHITECTURE_REFERENCE.md) | how it's built, for the future maintainer |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | working on the code: gates, hooks, releases |

Design history: [docs/FEATURES.md](docs/FEATURES.md) (the feature registry,
IDs A1–H6), [docs/ARCHITECTURE_DECISIONS.md](docs/ARCHITECTURE_DECISIONS.md)
(AR1–16), [docs/REALIZATION_PLAN.md](docs/REALIZATION_PLAN.md) (milestones),
[docs/MIGRATION_INVENTORY.md](docs/MIGRATION_INVENTORY.md) (the M5 plan).
Pre-rewrite documentation is archived under [docs/legacy/](docs/legacy/).

## Repository layout

```
core/     all domain logic, zero ambient I/O (Executor trait, 83 tests)
proto/    wire types for the one CLIENT↔HOST line
host/     the Proxmox daemon (systemd, TLS, scheduler, watchdog)
client/   CLI verbs + the TUI (Elm-style, snapshot-tested)
presets/  the app catalog — data, not code
stacks/   deployable stack definitions (secrets gitignored)
docs/     see above
```

## Development

**First thing after cloning — wire the commit gates:**

```bash
make hooks     # git config core.hooksPath .githooks
```

This is not optional and it is not automatic: `core.hooksPath` is local git
config, so it is never carried by a clone. Skip it and commits are accepted
with failing tests and untraceable messages — which is exactly what happened
here between v3.0.1 and v3.1.1 (see [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)).

```bash
make gate      # fmt + clippy -D warnings + full test suite (what the hooks run)
make release VERSION=x.y.z   # gate, tag, push; CI builds and publishes
homelab release-update       # roll out the published release to the host
```

Everything about building, gating, releasing and rolling out lives in
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

Standing rules: red CI blocks merge; every live bug becomes a test before the
fix; the no-touch list in `core/src/safety.rs` is law.
