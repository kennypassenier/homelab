# Architecture reference — for the future maintainer

The distilled version of [ARCHITECTURE_DECISIONS.md](ARCHITECTURE_DECISIONS.md)
(AR1–16, where the *why* lives). Read this first when you come back after
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

Two binaries (AR1): `client` and `host`, sharing `proto` (wire types) and
`core` (all domain logic). Containers run **zero** homelab code — the host
reaches in with `pct exec`/`pct push`.

## The four crates

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
  `[run ]` transcript lines. This is why 83 tests cover destructive
  operations without a hypervisor.
- **Runner + step! (AR3)** — every operation is a list of named steps.
  Uniformly provides: transcripts, journal records before each step
  (crash-visibility, AR13), fail-closed abort (A3), changed/unchanged
  reporting (idempotency surfacing), incident bundles on failure (AR14)
  with a replayable `commands.sh` (AR16).
- **Safety (A1/A2)** — `SafetyConfig.no_touch` is a hardcoded vmid list
  checked by every mutating op *plus* a live hostname guard: a vmid must
  carry `<vmid>-app-<stack>` before it is touched. Defense in depth: the
  guards repeat in ops even when the caller already checked.
- **State (AR4)** — intent lives in git (client `stacks/` + host repo
  mirror of every deploy); runtime truth in `/var/lib/homelab/state.json`
  (schema-versioned, atomically written). State stores each stack's
  manifest so host-side work (scheduler) needs no client.
- **Fail direction** — mutations fail closed (abort + bundle); nice-to-have
  integrations fail open with a loud warning (Kea reservations, webhook,
  mirror push). Know which one you're writing.
- **Presets are data** — `presets/<name>/` dirs with placeholder
  substitution; the scaffolder derives manifest storage from the compose's
  `/appdata/` binds (single source of truth). Never reintroduce a second
  place that must agree with the compose.

## Trust and secrets

- One line, TLS-pinned (TOFU on first connect → `~/.config/homelab/pin`),
  bearer token required. No PKI.
- Secrets travel only over that line and land in
  `/var/lib/homelab/secrets/` (0600). They never enter: git, bundles
  (D11), presets, argv (see the Kea curl pattern: `-u "$(cat file)"`).
- Remote exec is deny-by-default (`exec_enabled`), always audit-logged,
  and no-touch vmids are refused even when enabled.

## Self-preservation

- H5 self-update: selfcheck gate → `.prev` backup → armed marker →
  restart; systemd `OnFailure` restores `.prev` if the new binary never
  reports healthy (marker cleared only after 5s of serving).
- B7: `Type=notify` + `WatchdogSec=30` — a *hung* daemon is killed and
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
- **Mounts/devices apply at create only** — documented edge; destroy +
  redeploy applies them (data survives in /appdata). A reconcile step is
  future work, not a bug you half-fix in an afternoon.
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
