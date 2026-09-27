# Architecture reference — for the future maintainer

The distilled version of [ARCHITECTURE_DECISIONS.md](ARCHITECTURE_DECISIONS.md)
(AR1–19, where the *why* lives). Read this first when you come back after
six months.

## The shape

```
┌─────────────────────┐   one WS+JSON line, TLS (pinned        ┌──────────────────────┐
│ CLIENT (workstation) │   self-signed cert) + bearer token     │ HOST (Proxmox daemon) │
│ homelab CLI + TUI    │◄──────────────────────────────────────►│ systemd, Type=notify  │
│ authors stacks/      │   Envelope {v, topic, id, payload}     │ owns pct/docker/git/  │
│ presets/ locally     │   topics: rpc · log · telemetry ·      │ restic/state/vault    │
└─────────────────────┘   transfer                              └──────────────────────┘
```

Two binaries (AR1): `homelab` (crate `client`) and `homelab-host` (crate
`host`), sharing `proto` (wire types) and `core` (all domain logic). A fifth
workspace member, `tui-preview`, is a TUI mockup on simulated data and ships
nowhere. Containers run **zero** homelab code — the host
reaches in with `pct exec`/`pct push`.

## The four crates that ship

| Crate | Role | Key invariant |
|---|---|---|
| `core` | every operation, guard, and format | **zero ambient I/O** — everything flows through the `Executor` trait; no clocks (`now_unix` is injected); fully unit-testable with `MockExecutor` |
| `proto` | wire types, re-exports core's domain types | one `PROTO_VERSION`; a version mismatch tells the client to upgrade instead of failing cryptically |
| `host` | thin shell: real Executor, config, TLS/WS server, broadcast sink, journal file, scheduler | contains no domain decisions — if it needs an `if` about *what* to do, that `if` belongs in core |
| `client` | CLI verbs + Elm-style TUI (Model/Msg/pure update/view) over a `Backend` trait | the TUI cannot tell the real backend from the test/demo one (AR6) |

## The load-bearing patterns

- **Executor (AR2)** — one trait for run/read/write/sleep. `RealExecutor`
  in the host; `MockExecutor` in tests (scripted responses, recorded
  calls, in-memory files); `TracingExecutor` decorates any of them to emit
  `[run ]` transcript lines. This is why the suites that drive destructive operations (296 test
  functions in the test files that use `MockExecutor`, counted 2026-09-27
  with `grep -cE '#\[(tokio::)?test'`) run
  without a hypervisor.
- **Runner + step! (AR3)** — every operation is a list of named steps.
  Uniformly provides: transcripts, journal records before each step
  (B5; AR13 reads them back at boot to name interrupted operations), fail-closed abort (A3), changed/unchanged
  reporting (idempotency surfacing), incident bundles on failure (AR14)
  with a replayable `commands.sh` (AR16).
- **Safety (A1/A2)** — `SafetyConfig.no_touch` is a vmid list whose
  hardcoded default `[100, 101, 102, 103]` host.toml can widen but never
  shrink,
  checked by every mutating op *plus* a live hostname guard: a vmid must
  carry `<vmid>-app-<stack>` before it is touched. Defense in depth: the
  guards repeat in ops even when the caller already checked.
- **State (AR4)** — intent lives in git (client `stacks/` + host repo
  mirror of every deploy); runtime truth in `/var/lib/homelab/state.json`
  (schema-versioned, atomically written). State stores each stack's
  manifest so host-side work (scheduler) needs no client.
- **Fail direction** — mutations fail closed (abort + bundle); nice-to-have
  integrations fail open with a loud warning (webhook, mirror push, device
  backups). Know which one you're writing.
- **Presets are data** — `presets/<name>/` dirs with placeholder
  substitution; the scaffolder derives manifest storage from the compose's
  `/appdata/` binds (single source of truth). Never reintroduce a second
  place that must agree with the compose.

## Trust and secrets

- One line, TLS-pinned (TOFU on first connect → `~/.config/homelab/pin`),
  bearer token required. No PKI.
- Secrets travel only over that line and land in
  `/var/lib/homelab/secrets/` (0600) on the host, and in
  `/opt/<stack>/<app>/.env` (0600) inside the container that uses them.
  They never enter: git, bundles (D11), presets, argv. For curl that means
  `-K` with the credential on stdin or in a file, never `-u "$(cat file)"`
  (`core/src/ops/devicebackup.rs`; guarded by `core/tests/argv_secret_tests.rs`, fix-32).
- Remote exec is deny-by-default (`exec_enabled`), always audit-logged,
  and no-touch vmids are refused even when enabled.
- **Precondition for any future key-escrow step (gap-15).** `latch key
  backup` empties the shared OS keyring, even under a separate
  `LATCH_HOME`. A step that escrows a latch key on the host must therefore
  refuse to run on a machine that holds other latch keys, until the latch
  project decides otherwise. No such step exists today.

## Self-preservation

- H5 self-update: selfcheck gate → `.prev` backup → armed marker →
  restart; systemd `OnFailure` restores `.prev` if the new binary never
  reports healthy (marker cleared only after 5s of serving).
- B7: `Type=notify` + a systemd watchdog (the daemon pings every 10 s;
  `WatchdogSec` lives in the unit on the host, not in this repo) — a *hung* daemon is killed and
  restarted; a *crashing* one is rolled back (different failure, different
  mechanism).
- Boot: journal names interrupted operations; `host-online` webhook tells
  HA the box is back.

## Things that look wrong but are decisions

- **Overcommit on the dashboard** — LXC RAM limits routinely sum past
  physical; actual usage is the primary gauge, committed is context (C6).
- **Bootstrap still runs over golden-template clones** — bootstrap is the
  source of truth; the template only makes it a no-op. Never make the
  template authoritative.
- **The SHELL tab is not a PTY** — every command is one audited round-trip
  by design; an interactive PTY would bypass the audit model.
- **Devices apply at create only** — documented edge; destroy + redeploy
  applies them (data survives in /appdata). Mounts are reconciled: a
  redeploy attaches missing `mp` mounts to an existing container (F118).
- **swap = clamp(RAM/4, 512M, 2G)** — container swap caps shared *host*
  swap; big swap on a runaway container grinds the whole host.
- **`exec_enabled` is not in the SETTINGS tab** — enabling remote code
  execution should take an ssh session, deliberately.

## Where to add things

| You want to… | Touch |
|---|---|
| new operation | `core/src/ops/<name>.rs` (Runner + step! + guards) → proto Command → host arm (`run_mutating_op` if it mutates) → client verb → tests with MockExecutor |
| new catalog app | `presets/<name>/` only — no code |
| new host setting | host.toml `FileConfig` (+ `HostConfigView`/SETTINGS tab only if it's safe to edit remotely) |
| new safety rule | `core/src/safety.rs` + a test proving refusal |
| new TUI surface | model fields + pure update + view fn + snapshot test; AZERTY: spell out modifiers, digits need their symbol twins |

## The one rule

Every bug found live becomes a MockExecutor test before it is fixed. The
test suite is the only reason a two-binary system that runs `pct destroy`
can be edited without fear.
