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

**AR18 · MSRV: pinned at 1.88** (`rust-version` in the workspace, inherited
by every crate) with a dedicated CI job compiling on exactly that toolchain.
Chosen because the code already uses 1.82 APIs (`Option::is_none_or`, 1.82; `u64::is_multiple_of`, 1.87 — the clippy MSRV lint caught the higher floor immediately after pinning).
**Raised from 1.87 to 1.88 on 2026-08-31** by the ratatui 0.30 upgrade, which
requires it. The workspace manifest was raised and the CI job was not, so the
MSRV job failed on every push for forty minutes while claiming to check a
floor the project no longer had — a pin in two places is a pin that drifts.
Both now say 1.88.

**AR19 · License: MIT OR Apache-2.0 (dual), decided 2026-08-11** after the
deep-dive explanation — the Rust-ecosystem convention: patent grant for who
wants it, MIT compatibility for who needs it. LICENSE-MIT + LICENSE-APACHE
in the repo root, `license` field in the workspace.

## Milestone report (V6) — signed off 2026-08-11

Kenny approved B1–B5 (M0–M6 exit criteria with evidence) and ratified the
E1→G8 retention deviation (B7) and the E4-nightly-hour + G4-REPL deviations
(B9). Open: B6 (H5 update flow — redesign under discussion) and B8 (D2
enabled flag — decision pending a deeper trade-off round).

## Amendment 2026-09-27 — decisions the code never built (expert panel, architecture-decisions-not-built)

The panel found five decisions above that describe something the code does
not do, so a reader designing against this page would design against
features that do not exist. Each is amended here to what the code does; the
table rows above are left as they were decided. Nothing below adds a
feature. Where one is still wanted, it is a new decision with a new AR id.

**AR5 · amended: bare JSON frames, no envelope.** The `{v, topic, id,
payload}` envelope was defined in `proto` and used by neither binary; it is
removed (fix-136). Frames are bare JSON: the host opens with
`ServerMsg::Hello { version, proto }`, the client sends `RpcRequest { id,
cmd, … }`, and logs, questions and replies are `ServerMsg` variants tagged
by `kind`. Version skew is handled by the client, which refuses a mutating
command to a host older than itself. `PROTO_VERSION` is reported in `Hello`
and the incident bundle's `versions`; it is bumped only when a change would
make an older peer misread a frame (a field or message removed, renamed or
given a new meaning), not for additions, which the version gate covers. No
such change has been made, so it is still 1. `deny_unknown_fields` on
`DeploySpec` was considered and not added: the 2026-08-31 field
(`data_mounts`) sat inside `StackManifest`, which state.json also stores, so
refusing unknown fields there would make a rolled-back host refuse its own
state, and on `DeploySpec` alone it would not have caught that incident.

**AR8 · amended: no template engine.** No crate depends on minijinja.
Presets are copied and filled by plain string substitution
(`client/src/scaffold.rs`); the runbook and test plan are generated by Rust
code (`homelab runbook`, `homelab testplan`).

**AR15 · built as decided (2026-10-01, Kenny's go, superseding the
"journald only" amendment below).** The host still logs with `tracing` to
stderr, which systemd writes to the journal, with the level set from
`RUST_LOG` at start when set. It now ALSO writes a size-capped JSONL ring
under `<state_dir>/logs/host.jsonl` (`core/src/logring.rs`,
`host::RingWriter`; rule 20, cut back the way `journal.jsonl` is), and the
level is a runtime debug toggle: `log_level` in host.toml, dashboard-
editable, applied live through a `tracing_subscriber::reload::Handle` both
sinks share — `Rpc::SetHostConfig`/`Rpc::ApplyHostConfig`, no restart
(fix-122 ADDENDUM, `docs/deployment/REGISTER.md`). The operation journal
(`journal.jsonl`, AR13) stays a separate record of steps, not a log — this
amendment does not touch it.
<br><sub>Superseded text, kept for the retrospective: "AR15 · amended: logs
go to journald only. The host logs with `tracing` to stderr, which systemd
writes to the journal; the level comes from `RUST_LOG` at start. There is
no JSONL log ring and no runtime level toggle."</sub>

**AR16 · amended: replay yes, frame capture no.** Incident bundles carry the
transcript as a replayable `commands.sh`; there is no protocol frame-capture
toggle.

**AR18 · amended: MSRV 1.88, checked at release.** `rust-version = "1.88"`
holds and the code builds with it (`cargo +1.88 check --workspace --locked`,
2026-09-27). The CI job this decision names was removed on 2026-09-10
(d318ada); the check runs in `make release`, which refuses when the
toolchain is missing or the build fails. A CI job for it is the separate
panel finding `ci-hygiene-gaps`.
