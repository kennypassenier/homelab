# Architecture decisions

Decision log for homelab v2, reviewed by Kenny per decision (IDs AR1..AR16 are
stable, like the feature IDs). **Architecture phase closed 2026-08-10 — all 16
decided, every one per Claude's advice after deep-dive rounds.** Rationale
summaries here; the full discussion lives in the review session and the vault
decision note. New architecture decisions get the next AR id and an entry here
before implementation.

| ID | Decision | Status |
|---|---|---|
| AR1 | Crates: `proto` (wire types) + `core` (all domain logic, zero I/O) + `host` + `client`; validator lives in core, imported by both sides (D10) | **Decided** |
| AR2 | All system interaction through an `Executor` trait; `MockExecutor` for tests | **Decided** |
| AR3 | Operations are step pipelines under one runner (transcript, gates, journal, fail-closed, byte counters for free) | **Decided** |
| AR4 | State = typed JSON files, `schema_version`, atomic tmp+rename writes | **Decided** |
| AR5 | WS + JSON envelope `{v, topic, id, payload}`; topics rpc/log/telemetry/transfer; version field enables graceful client/host skew after self-updates | **Decided** |
| AR6 | TUI = Elm-style (Model, Msg enum, pure `update`, side-effect-free `view`) over a `Backend` trait; fx engine stays a separate stateless layer | **Decided** |
| AR7 | `thiserror` typed errors per layer; boundary `OperatorError` always carries what/why/what-you-can-do | **Decided** |
| AR8 | Templates via minijinja; defaults embedded in the binary, user override dir | **Decided** |
| AR9 | Five test layers, hard CI gates (fmt, clippy -D warnings, tests, `compose config` on templates, D10 divergence test) — red blocks merge | **Decided** |
| AR10 | Tag → GH Actions builds Debian-compatible binaries + sha256 in a Release; H5 self-update consumes with preflight + rollback watchdog; local build+scp stays documented as emergency path | **Decided** |
| AR11 | Program config = TOML (+ env overrides); stack manifests stay YAML (content, not config) | **Decided** |
| AR12 | Mutating operations strictly serial behind one op-lock; reads/streams parallel; TUI shows a queue | **Decided** |
| AR13 | Interrupted operations: journal detects, stack goes fail-closed, TUI/F3 report "interrupted at step N — redeploy is safe"; explicit manual rerun, no auto-resume (idempotency B1 makes rerun safe) | **Decided** |
| AR14 | Every failed operation auto-captures an incident bundle (transcript, journal, state slice, daemon logs, container diagnostics, versions) under `/var/lib/homelab/incidents/`; standing process: every bug becomes a MockExecutor test scripted from the bundle → CI keeps it fixed forever | **Decided** |
| AR15 | `tracing` with spans (op-id, step, stack on every line); sinks: journald + JSONL ring under `/var/lib/homelab/logs/`; debug level toggleable at runtime without restart | **Decided** |
| AR16 | Protocol frame capture (toggle, human-readable AR5 JSON) + transcript replay export ("this operation as a shell script") from incident bundles | **Decided** |

## Decided — brief rationale

- **AR2 Executor trait**: foundation for ~40% of the FEATURES.md test
  scenarios (safety gates provable without touching real infra). Strongest
  conviction of the twelve; accepted.
- **AR3 Step pipeline**: cross-cutting features (F2 transcripts, B3 gates,
  B5 journal, A3 fail-closed, G6 byte counters) implemented once in the
  runner instead of re-implemented per operation.
- **AR4 JSON state**: tiny data volume, human-readable during emergency
  debugging on the host, feeds E7's runbook generator directly. Atomic
  writes make power loss unable to corrupt state (power-loss rule).
- **AR7 Error model**: every operator-facing error must include a
  remediation hint — consumed by the TUI and F6 doctor.
- **AR8 minijinja**: jinja2 syntax Kenny already knows from Ansible/HA;
  one engine for D7 presets, D8 injection and E7 runbook.
- **AR9 Hard CI**: a quality bar that does not block does not exist.
- **AR11 TOML config**: Rust convention, comment-friendly; manifests remain
  YAML by design.
- **AR12 Serial mutations**: eliminates the backup-vs-deploy race class
  outright; every transcript is the whole story.

---

## Amendment 2026-08-11 — Phase-3 items decided retroactively (procedure evaluation V4)

**AR17 · Dependency policy: pragmatic.** Small, pure-Rust, widely-used
crates without their own network/IO may be added silently (the practice
during the build: sha2, base64); anything with C bindings, network stacks,
or a heavy transitive tree requires a mini-round. Decided by Kenny in the
V4 mini-round.

**AR18 · MSRV: pinned at 1.82** (`rust-version` in the workspace, inherited
by every crate) with a dedicated CI job compiling on exactly that toolchain.
Chosen because the code already uses 1.82 APIs (`Option::is_none_or`).

**AR19 · License: pending deep-dive.** Kenny leans MIT/Apache-2.0 dual but
asked for a fuller explanation first; to be settled in the follow-up form.
