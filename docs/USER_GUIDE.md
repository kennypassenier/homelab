# User guide

What every feature does and how you use it, organised by the feature IDs in
[FEATURES.md](FEATURES.md) (A1 to H8, plus W1 to W3). Test steps live in
[TEST_PLAN.md](TEST_PLAN.md); failure analysis in
[DEBUGGING_GUIDE.md](DEBUGGING_GUIDE.md); host procedures in
[OPERATIONS_RUNBOOK.md](OPERATIONS_RUNBOOK.md).

How to read this guide:

- **Feature IDs** (A1, D12, H7, ...) come from `docs/FEATURES.md`. A decision
  from the deployment project is always written as **deployment decision X**
  (for example deployment decision D25 or deployment decision B1), with the
  file it lives in, so it cannot be mistaken for the feature with the same
  letter.
- **Status** opens every feature: *Built*, *Built, narrower than
  FEATURES.md* (the difference is named), *Not built*, *Won't* or *Removed*.
- **Sources** in parentheses point at the code or test that makes each
  statement true, as `path:line`. Paths are relative to the repository root.
- **Output** shown in this guide is the text the code prints, copied from
  the source. Parts the program fills in are shown as `<...>`. No command was
  run to produce this guide, so no number in it is a measurement.
- Messages quoted from the program keep their own punctuation, including
  the long dashes the source uses. The guide's own prose uses none.

---

## 0 · Before you start

### 0.1 Where the client finds the host, the token and the certificate

`make install` puts the client at `~/.cargo/bin/homelab`. It needs three
things, and each has its own home:

| What | Where it comes from, first match wins | Source |
|---|---|---|
| Host address | `HOMELAB_HOST` typed before the command; else `host` in `config/client.toml`, searched upward from the current directory the way git finds its root; else `HOMELAB_HOST` from `~/.config/homelab/env` or `./.env`; else the built-in `10.10.5.250:8443` | `client/src/main.rs:127-137`, `client/src/repo_config.rs:25,63,103` |
| Token | `HOMELAB_TOKEN` in the environment; else `~/.config/homelab/env`; else `./.env` | `client/src/main.rs:49-88,140` |
| Certificate pin | `~/.config/homelab/pin`; if that is empty, the `pin` in `config/client.toml` is adopted and saved; if both are empty, the first certificate seen is trusted and saved | `client/src/lib.rs:16-18`, `client/src/repo_config.rs:143`, `client/src/main.rs:1101-1155` |

Only keys that start with `HOMELAB_` are read from the two env files, and a
key already in the environment is never overwritten (`client/src/main.rs:76-81`).
`config/client.toml` accepts exactly two keys, `host` and `pin`; a file that
does not parse stops every command rather than falling back to a default
(`client/src/repo_config.rs:75-90`).

**Worked example: which door did I knock on?** `homelab ping` is the one verb
that prints where its address came from (`client/src/main.rs:1190-1194`):

```text
● HOST v<version> (proto <n>) — link up
  via <host:port> (<source>)
✓ pong
```

`<source>` is one of `HOMELAB_HOST in the environment`, the path of
`config/client.toml`, `~/.config/homelab/env` or `built-in default`
(`client/src/repo_config.rs:48-55`). To try another address once, type it
before the command:

```bash
HOMELAB_HOST=10.10.5.250:8443 homelab ping
```

**When the certificate does not match.** If this machine pinned one
fingerprint and `config/client.toml` names another, every command stops with
a message that starts `the host certificate pinned on this machine`
(`client/src/repo_config.rs:143-152`). If the host presents a certificate
that differs from the pin, the connection is refused with
`certificate fingerprint mismatch` (`client/src/tls.rs:63`). Both messages
say what to delete; do that only after checking the fingerprint the host
logs at start (`host/src/main.rs:1876`).

### 0.2 Which verbs need a token, and which read the working directory

Every verb needs `HOMELAB_TOKEN` except `help`, `plan`, `runbook`,
`dashboard`, `presets`, `export`, `import` and `tui --offline`
(`client/src/main.rs:144-150`). Note that `new` and `testplan` never contact
the host but are not on that list, so they also stop with
`HOMELAB_TOKEN is not set` when no token is configured.

These verbs read or write paths **relative to the current directory**, so run
them from the repository root:

| Verb | Reads / writes relative to the current directory | Source |
|---|---|---|
| `homelab presets` | reads `presets/` | `client/src/main.rs:650` |
| `homelab new` | reads `presets/`, writes `stacks/<name>/` | `client/src/main.rs:585,624-628` |
| `homelab import` | writes `stacks/<name>/` | `client/src/main.rs:466` |
| `homelab runbook` | reads `stacks/`, writes `docs/DR_RUNBOOK.md` unless an output path is given | `client/src/main.rs:902-906` |
| `homelab testplan` | reads `core/tests`, `client/tests`, `docs/deployment/REALIZATION_PLAN.md`; writes `docs/deployment/TEST_PLAN.md` unless an output path is given | `client/src/main.rs:883-891` |
| `homelab check` | reads `stacks/` unless a path is given | `client/src/main.rs:311` |
| `homelab export` | writes `<name>-bundle.yml` here unless an output path is given | `client/src/main.rs:446-449` |
| `homelab tui` | reads `stacks/` and `presets/` at start | `client/src/tui/mod.rs:47-48` |

The verbs that take a stack path (`deploy`, `plan`, `backup`, ...) read the
path you give them, so they work from anywhere as long as the path is right.

### 0.3 The version gate

When the host is older than the client, the command line refuses to send any
command except `ping`, `status`, `doctor`, `incidents`, a state request and a
host update (`client/src/version.rs:15-30`, `client/src/main.rs:1203-1213`).
Read-only verbs such as `homelab check` and `homelab config` are refused too.
The message ends with `run 'homelab release-update' first`. The reason is in
the message: a host that predates a field ignores it, and the operation
quietly does less than asked. The TUI has no such gate.

### 0.4 Reading an answer

Every host operation streams its log lines and ends with one line:
`✓ <message>` and exit code 0, or `✗ <message>` and exit code 1
(`client/src/main.rs:1056-1059,1267-1277`). A successful operation says
`<label> complete — <n> step(s), <m> changed`. A failed one says
`<what> :: <why> :: remedy: <remedy> :: incident bundle <dir>`
(`host/src/main.rs:3043-3098`). An operation that deliberately did not run
says `<label> deferred — <reason>` and leaves no incident
(`host/src/main.rs:3054-3065`).

Arguments are positional. `--help` or `-h` anywhere on the line prints the
usage instead of running anything (`client/src/main.rs:109-111`,
`client/src/version.rs:98-100`).

---

## 1 · The control deck (TUI)

```bash
homelab tui             # against the host
homelab tui --offline   # against a built-in fake host, no token needed
```

`--demo` is accepted as a synonym of `--offline` (`client/src/main.rs:141`).
Start the TUI from the repository root: it looks for `stacks/` and `presets/`
in the current directory when it starts (`client/src/tui/mod.rs:47-48`), and
see the warning in 1.3 for what happens when it finds none.

### 1.1 Tabs and global keys

Keys are case-exact: `u` and `U` are different actions. The footer shows a
capital letter as `SHIFT+<letter>` (`client/src/tui/view/mod.rs:670-718`).

| Tab | Select with | What is on it |
|---|---|---|
| DASHBOARD | `1` or `&` | host mesh, CAPACITY (C6), DATA_TRANSFERS (G6) |
| STACKS | `2` or `é` | STACK_REGISTRY: per-stack detail, drift and parked badges |
| LOG_STREAM | `3` or `"` | the live operation feed (F2) |
| DOCTOR | `4` or `'` | SELF_DIAGNOSIS (F6) |
| SETTINGS | `5` or `(` | nightly hour, retention tiers, webhook (G8) |
| SHELL | `6` or `§` | line-based remote commands (G4) |

Source: `client/src/tui/model.rs:36-45,751-761`.

| Key | Anywhere outside a modal | Source |
|---|---|---|
| `TAB` / `SHIFT+TAB` | next / previous tab | `client/src/tui/model.rs:726-733` |
| `CTRL+K` or `CTRL+P` | command palette (G3) | `client/src/tui/model.rs:716-720` |
| `F2` | cycle effects `FX:OFF` → `FX:SUBTLE` → `FX:FULL` | `client/src/tui/model.rs:721-724`, `client/src/tui/fx.rs:26-39` |
| `h` | key map; `ESC`, `h` or `ENTER` closes it | `client/src/tui/model.rs:652-657,725` |
| `q` | quit | `client/src/tui/model.rs:714` |

On the SHELL tab, typing owns the keyboard: only `TAB`, `SHIFT+TAB`, `F2`,
`CTRL+K` and `CTRL+P` pass through (`client/src/tui/model.rs:699-710`).

### 1.2 Keys on DASHBOARD and STACKS

All of these act on the stack under the cursor.

| Key | Action | Source |
|---|---|---|
| `UP`/`k`, `DOWN`/`j` | move the cursor | `client/src/tui/model.rs:767-778` |
| `r` | refresh fleet state | `client/src/tui/model.rs:779` |
| `p` | change plan for the stack (D6); `ENTER` runs it, `ESC` cancels | `client/src/tui/model.rs:669-688,851` |
| `SHIFT+D` | deploy the stack (D1) without a plan | `client/src/tui/model.rs:793` |
| `SHIFT+B` | back it up (E1) | `client/src/tui/model.rs:799` |
| `SHIFT+U` | update its apps (D9) | `client/src/tui/model.rs:800` |
| `SHIFT+R` | restore it from the latest snapshot (E2), after typing the stack name | `client/src/tui/model.rs:801-817` |
| `g` | apply the runaway guards (B2) | `client/src/tui/model.rs:818` |
| `SHIFT+A` | adopt its native services (C7) | `client/src/tui/model.rs:823` |
| `SHIFT+I` | install its native services from their releases (C7) | `client/src/tui/model.rs:824` |
| `c` | fleet check over `stacks/` | `client/src/tui/model.rs:825` |
| `i` | list incident bundles | `client/src/tui/model.rs:826-836` |
| `e` | park or unpark for the nightly run (H8) | `client/src/tui/model.rs:837-850` |
| `u` | update the host binary, only when a newer release is offered (H7) | `client/src/tui/model.rs:780-792` |
| `n` | new-stack wizard (G2) | `client/src/tui/model.rs:852-872` |

`SHIFT+A` and `SHIFT+I` work but are not listed in the `h` key map
(`client/src/tui/view/mod.rs:774-794`); the palette lists them as
`adopt: native services of selected stack` and
`install-native: binaries of selected stack` (`client/src/tui/model.rs:1156-1163`).

On a native-only stack, `SHIFT+B` runs the native backup and `SHIFT+U` the
supervised self-update; `SHIFT+R` is refused with
`restore is not wired for native stacks — restore by hand as docs/OPERATIONS_RUNBOOK.md op-11 describes`
(`client/src/tui/model.rs`). `homelab restore` refuses a native stack too
(gap-28): its backup is one tar stream, which a restic restore would drop on
the host unpacked.

The placeholders before the first state (`awaiting state… [r] refresh` on STACKS, `press r or ENTER to re-run` on DOCTOR) name lowercase `r` since v3.58.4 (gap-21; they said `R`, which is restore on STACKS). Refresh is `r`. On STACKS, `SHIFT+R` opens
the restore prompt; it restores nothing unless you then type the stack name.

### 1.3 Warning: the TUI and a missing `stacks/` directory

For every per-stack action the TUI looks for `stacks/<name>/` in the
directory it was started from. When it finds none, it builds a **synthetic
manifest** from the fleet view, with default resources and **no storage
entries** (`client/src/tui/model.rs`, `resolve_spec`). Read what that means
before starting the TUI anywhere else:

- `SHIFT+B` and `SHIFT+R` refuse (gap-19): the status line says the stack
  needs `stacks/<name>/`, and nothing is sent. Before v3.58.4 a backup with
  the synthetic manifest wrote nothing while the host recorded a backup time,
  and a restore stopped and restarted every app and restored nothing.
- Drift badges are only computed for stacks found locally
  (`client/src/tui/model.rs:490-509`).

### 1.4 Keys on the other tabs

| Tab | Key | Action | Source |
|---|---|---|---|
| LOG_STREAM | `SPACE` | toggle follow | `client/src/tui/model.rs:876-881` |
| LOG_STREAM | `UP`/`k`, `DOWN`/`j` | scroll | `client/src/tui/model.rs:882-891` |
| LOG_STREAM | `LEFT`/`RIGHT` | source filter: `ALL`, then each stack name | `client/src/tui/model.rs:892-899`, `client/src/tui/view/logs.rs:13-30` |
| LOG_STREAM | `SHIFT+G` or `END` | jump to the tail and follow | `client/src/tui/model.rs:900-903` |
| DOCTOR | `r` or `ENTER` | run the doctor again | `client/src/tui/model.rs:906-910` |
| SETTINGS | `UP`/`DOWN` | pick a row | `client/src/tui/model.rs:946-947` |
| SETTINGS | `LEFT`/`RIGHT` | change the value | `client/src/tui/model.rs:948-978` |
| SETTINGS | `a` / `d` | add / delete a retention tier | `client/src/tui/model.rs:979-1002` |
| SETTINGS | `ENTER` on the webhook row | edit it; `ENTER` keeps, `ESC` cancels | `client/src/tui/model.rs:1003-1005,1076-1097` |
| SETTINGS | `SHIFT+S` | send the settings to the host | `client/src/tui/model.rs:1006-1010` |
| SETTINGS | `r` | reload from the host | `client/src/tui/model.rs:1011` |
| SHELL | type, `ENTER` | run the line in the target container | `client/src/tui/model.rs:1047-1071` |
| SHELL | `LEFT`/`RIGHT` with an empty line | change the target stack | `client/src/tui/model.rs:1026-1033` |
| SHELL | `UP` | recall the last command; `ESC` clears the line | `client/src/tui/model.rs:1034-1046` |

### 1.5 Windows that take over the keyboard

- **Progress window** (every operation): `UP`/`DOWN` scroll; `ESC` hides it
  while the operation keeps running (`deploy keeps running — feed in LOG_STREAM`);
  `ENTER` closes it once done (`client/src/tui/model.rs:634-651`).
- **A question from the host**: when a deploy's service checks see a value go
  down, the host pauses and asks. Only `a` (allow) and `s` (stop) do anything
  until it is answered (`client/src/tui/model.rs:620-633`). The window
  shows `[a] toelaten` and `[s] stoppen` (`client/src/tui/view/focus.rs:138-142`).
  Unanswered, it times out after `ask_timeout_s` seconds, 120 by default, and
  counts as a stop (`host/src/main.rs:146-152,669-671`,
  `core/src/ops/deploy.rs:2617-2659`). On the command line the question is
  printed and cannot be answered there (`client/src/main.rs:1173-1182`).
- **Typed confirmation** (restore): type the stack name and `ENTER`. Anything
  else answers `typed name does not match '<stack>' — nothing was done`
  (`client/src/tui/model.rs:1364-1392`).

---

## 2 · Features by ID

### A · Security

#### A1 · Whitelist-only management and the no-touch list

**Status:** Built.

The host only acts on stacks you send it, and four guests can never be acted
on at all: vmids 100, 101, 102 and 103 (`core/src/safety.rs:21`). The list is
compiled in. `no_touch = [...]` in `/etc/homelab/host.toml` adds vmids to it
and can never remove one (`host/src/main.rs:446-460`, test
`host/src/main.rs:843`). A vmid that belongs to a QEMU VM is refused too,
whether or not it is on the list (`core/src/safety.rs:61-70`).

Every operation that changes a container checks the list before its first
command: deploy (`core/src/safety.rs:40-52`), backup, restore, update,
enable/disable and adoption (`core/src/ops/mod.rs:42-53`), destroy
(`core/src/ops/destroy.rs`, step `no-touch check`), resize, patch, template
build and guards (`host/src/main.rs:3687-3700`), and remote exec
(`host/src/main.rs:2861-2866`, test `host/src/main.rs:864`).

**Worked example.** A stack file that names `vmid: 101` validates (the
allowed range is 100 to 354, `core/src/manifest.rs:623-625`), so
`homelab plan` accepts it. `homelab deploy` then stops in the host's
`safety gates` step with `vmid <vmid> is on the no-touch list`, here with
101 filled in (`core/src/safety.rs:47-52`), and no command reaches the machine: the test
`core/tests/deploy_tests.rs:327` asserts zero commands ran. No incomplete
state is recorded either (`core/src/ops/deploy.rs:67-75`).

#### A2 · Hostname guard

**Status:** Built.

A managed container is always called `<vmid>-app-<stack>`. The validator
refuses a stack file whose `hostname` is anything else
(`core/src/manifest.rs:631-637`), and before touching an existing container
the host reads its live hostname and refuses a mismatch with
`vmid <n> exists with hostname '<live>', expected '<expected>' — refusing`
(`core/src/safety.rs:72-87`). The same live check guards backup, restore,
update, enable/disable, adoption, native updates and orphan pruning
(`core/src/ops/mod.rs:42-80`, `host/src/main.rs:3575-3585`), and destroy
(`core/src/ops/destroy.rs`, step `hostname guard`). Tests:
`core/tests/deploy_tests.rs:354`, `core/tests/m4_ops_tests.rs:1322`.

`homelab guards <vmid>` and `homelab patch` check the no-touch list but not
the hostname (`host/src/main.rs:3687-3727`, `core/src/ops/patch.rs`).

#### A3 · Fail-closed behaviour

**Status:** Built, narrower than FEATURES.md.

What is built: the first failing step ends the operation
(`core/src/runner.rs:100-104`, `core/src/ops/deploy.rs:20-30`), and every
failed operation leaves an **incident bundle** under
`/var/lib/homelab/incidents/<unix-time>-<op>/` holding `report.json`,
`events.jsonl`, `commands.sh` (the commands it ran, as a script),
`state-at-failure.json`, `journal-tail.jsonl` and `versions.txt`
(`core/src/incidents.rs:55-110`, `host/src/main.rs:3077-3089`). A deploy that
fails after the machine was touched records the stack with the step where it
stopped, and logs `recorded as incomplete at step` (`core/src/ops/deploy.rs:55-117`).

What is not built: FEATURES.md says a failed provision leaves the stack
disabled and a missing `.env` aborts before compose. The code records a
failed first deploy as enabled (`core/src/ops/deploy.rs:99`), and an app
without a `.env` in the payload gets the host's vault copy if there is one
and otherwise none (`core/src/ops/deploy.rs:1140-1151`). Only a failed
*nightly* run disables a stack (H8).

List the bundles with `homelab incidents` or `i` in the TUI
(`host/src/main.rs:4255-4277`).

#### A4 · TLS with a pinned certificate on the client-host line

**Status:** Built.

The daemon serves TLS only (`host/src/main.rs:1903`) with a self-signed
certificate it creates once in `/var/lib/homelab/tls-cert.pem` and
`tls-key.pem` (`host/src/tls.rs:16-19`), and logs its fingerprint at start
(`host/src/main.rs:1876`). The WebSocket at `/api/ws` needs
`Authorization: Bearer <token>`; without it the answer is 401
`missing or invalid bearer token` (`host/src/main.rs:2717-2723`). The daemon
refuses to start with a token shorter than 16 characters
(`host/src/main.rs:409-419`). `/api/health` and `/api/version` answer without
a token (`host/src/main.rs:1864-1865`). Pinning is described in 0.1; test
`client/tests/tls_pin_tests.rs:33`.

#### A5 · Secrets vault on the host

**Status:** Built.

An app's secrets are its `.env`. The client reads `stacks/<stack>/<app>/.env`
(or asks latch, D12) and sends it beside the files, never as one of them
(`client/src/spec.rs:285-291`). The host writes it into the container as
`/opt/<stack>/<app>/.env` with mode 600 and keeps a copy at
`/var/lib/homelab/secrets/<stack>/<app>.env` with mode 0600; the log says
`sealed (values not logged)` (`core/src/ops/deploy.rs:1132-1138`). The intent
repository only receives the stack's files (`core/src/ops/deploy.rs:907-913`).
When a later deploy sends no `.env` for an app, the vault copy is pushed
again (`core/src/ops/deploy.rs:1140-1151`). `*.env` is in `.gitignore`
(`.gitignore:1`). Tests: `core/tests/deploy_tests.rs:1637`,
`core/tests/m4_ops_tests.rs:2176`.

#### A6 · Remote exec (off by default)

**Status:** Built.

Off until `exec_enabled = true` is set in `/etc/homelab/host.toml` on the host
(`host/src/main.rs:435`); it is deliberately not in the SETTINGS tab
(`host/src/main.rs:175-177`). While off, every call answers
`remote exec is disabled (set exec_enabled = true in host.toml to allow it)`
(`core/src/safety.rs:121`). Every call is appended to
`/var/lib/homelab/audit.log` as `<unix-time> exec vmid=<n> cmd="<command>"`
before it runs (`host/src/main.rs:3965-3979`). The command runs through
`sh -c` inside the container with a 120-second limit
(`host/src/main.rs:3981`). A vmid on the no-touch list, compiled or added in
`host.toml`, is refused even with exec on (`core/src/safety.rs:124-129`,
`host/src/main.rs:2861-2866`). Tests: `core/tests/m4_ops_tests.rs:998`,
`host/src/main.rs:864`.

**Worked example.**

```bash
homelab exec 108 df -h /
```

Everything after the vmid is joined with spaces into one command
(`client/src/main.rs:544-555`). The answer is `exit <code>`, then stdout,
then a `--- stderr ---` section when there was any
(`host/src/main.rs:3982-3995`). The SHELL tab (G4) sends the same request.

#### A7 · Unattended security updates in every container

**Status:** Built.

Every deploy and every `homelab guards <vmid>` writes
`/etc/apt/apt.conf.d/50unattended-upgrades` with Debian-Security origins
only, `Automatic-Reboot "false"` and `Remove-Unused-Dependencies "true"`
(`core/src/ops/guards.rs:125-131,309`, `core/src/ops/deploy.rs:884-895`).

#### A8 · CrowdSec and bouncer on the gateway

**Status:** No orchestrator code. CrowdSec is an ordinary app of the gateway
stack (`stacks/gateway/lxc-compose.yml:128`) with its own
`stacks/gateway/crowdsec/checks.yml`; it is deployed like any other app.

#### A9 · Multi-user / RBAC

**Status:** Won't (FEATURES.md). One token, one administrator.

### B · Idempotency and self-healing

#### B1 · Idempotent bootstrap and deploy

**Status:** Built.

Every step reports whether it changed anything (`core/src/runner.rs:40-46`),
files are pushed only when their hash differs, and the intent commit passes
when there is nothing to commit (`core/src/ops/deploy.rs:950-970`). Test
`core/tests/deploy_tests.rs:1550` deploys a stack whose files and guards are
already in place and asserts that no `pct create`, `pct start`, `pct push`,
docker restart, journald restart or timer enable ran. Re-running a deploy
after any failure is the normal remedy; the doctor says so for interrupted
operations (`core/src/doctor.rs:178-185`).

#### B2 · Runaway guards

**Status:** Built.

On every deploy (`core/src/ops/deploy.rs:884-895`) and on demand with
`homelab guards <vmid>` or `g` in the TUI, the host writes:

| File in the container | Content | Source |
|---|---|---|
| `/etc/docker/daemon.json` | container logs capped at `max-size` 10m, `max-file` 3 | `core/src/ops/guards.rs:30-31,178` |
| `/etc/systemd/journald.conf.d/homelab-limits.conf` | `SystemMaxUse=100M`, `RuntimeMaxUse=50M`, one month | `core/src/ops/guards.rs:83,187` |
| `/etc/logrotate.d/homelab` | syslog rotation | `core/src/ops/guards.rs:220` |
| `docker-prune.service` / `.timer` | weekly `docker system prune -f --filter until=168h`, docker containers only | `core/src/ops/guards.rs:133-135,226-252` |
| `/etc/apt/apt.conf.d/60homelab-clean` | apt autoclean | `core/src/ops/guards.rs:292` |

`homelab guards <vmid>` works on any container not on the no-touch list,
including ones this orchestrator did not build (`client/src/main.rs:290-302`).

A log the stack writes onto a borrowed directory (a `data_mounts` entry) gets
a rotation rule when the entry says so:

```yaml
data_mounts:
  - host_path: /HDD2TB/logs/traefik
    mount_point: /logs
    rotate:
      files: access.log      # a name or glob inside the mount
      keep: 14               # default 14, allowed 1 to 365
      reopen:                # optional; without it the rule uses copytruncate
        container: traefik
        signal: USR1         # default USR1
```

Fields: `core/src/manifest.rs:289-335`; the rule is written to
`/etc/logrotate.d/homelab-<stack>` (`core/src/ops/guards.rs:329`) in the
`log rotation` step (`core/src/ops/deploy.rs:897-905`). The `mount_point`
above is an example; use the stack's own.

#### B3 · Verify gates after every deploy

**Status:** Built.

A deploy is only green after three checks:

1. **`verify health`**: after 5 seconds, every app must have a running
   service according to `docker compose ps --status running --services`. A
   failing app's error carries `docker compose ps -a` and its last 20 log
   lines (`core/src/ops/deploy.rs:1547-1582`).
2. **`reconcile`**: the container is asked whether the whole stack file is
   true: hostname, every storage and data mount attached, boot policy, every
   app running, every native unit active. Anything else fails with
   `the deploy reported success but the container does not match the stack file`
   (`core/src/ops/deploy.rs:2423-2528`).
3. **`service checks`**: each app may carry a `checks.yml` beside its compose
   file. Every check runs before the work and after it, and the pair is
   judged (`core/src/ops/deploy.rs:239-269,2531-2660`).

Only then does the log say `Sync complete` (`core/src/ops/deploy.rs:2699-2707`).

**Worked example: a `checks.yml`.** From `stacks/home/homepage/checks.yml`,
shortened:

```yaml
checks:
  - name: "diensten op de startpagina"
    command: >-
      docker exec homepage sh -c 'grep -c "href:" /app/config/services.yaml'
    expect: never_decreases
    layer: application
    blind_spot: >-
      Counts what the page is configured to show, not what it renders ...
manual:
  - "Kijk of de startpagina de diensten toont die je verwacht. ..."
```

`expect` is `never_decreases`, `must_match` or `must_be_present`; `layer` is
`network`, `process`, `application` or `user_visible`
(`core/src/checks.rs:29-105`). A reading that went down stops the deploy and
asks you (see 1.5); in the TUI, `a` accepts the new value as normal and `s`
fails the deploy with an incident. An app the deploy restarted gets up to
twelve more readings, 5 seconds apart, before a drop counts
(`core/src/ops/deploy.rs:2561-2594`). Each `blind_spot` is printed as
`[check] does not prove: <text>` even when everything passed
(`core/src/ops/deploy.rs:2608-2612`). The `manual` lines become manual checks
you answer with `homelab checks answer` (see section 3).

#### B4 · Drift detection

**Status:** Built.

The host stores a hash of what it last applied per stack
(`core/src/ops/deploy.rs:2402`). The TUI hashes the local `stacks/<name>/`
the same way (`core/src/manifest.rs:951`) and marks a stack `[UPD]` when the
two differ (`client/src/tui/model.rs:490-509`,
`client/src/tui/view/stacks.rs:59-61`); the ticker lists them as
`⚠ UPD pending: <stacks>` (`client/src/tui/view/mod.rs:615`). Redeploy to
converge. Only stacks found under the TUI's `stacks/` are compared.

#### B5 · Transaction journal

**Status:** Built.

Every step of every operation writes `running` before it starts and `done`
or `failed` after it, and the operation ends with `complete`, `failed` or
`deferred` (`core/src/runner.rs:81-104,159-189`), in
`/var/lib/homelab/journal.jsonl` (`host/src/main.rs:3008-3010`). At start the
daemon logs every operation the journal shows as unfinished, as
`interrupted operation '<op>' at step '<step>' — re-running it is safe (idempotent)`
(`host/src/main.rs:1812-1823`), and the doctor lists them
(`core/src/doctor.rs:178-185`).

#### B6 · Rollback guard for image updates

**Status:** Built, narrower than FEATURES.md.

Before an app is updated, the image id and `repo:tag` of each of its running
containers are captured. If the app is not running after the update, the
captured images are re-tagged and the app is force-recreated; the operation
then fails either way, saying whether the rollback took
(`core/src/ops/update.rs:145-151,229-267`). Test:
`core/tests/m4_ops_tests.rs:710`. What is not built: a stored digest history
per app. The capture lives only for the duration of one update.

#### B7 · Systemd watchdog

**Status:** Built (daemon side).

The daemon sends `READY=1` at start and `WATCHDOG=1` every 10 seconds
(`host/src/main.rs:1893-1901`). The unit settings that act on it
(`WatchdogSec`, restart) are part of the host installation, not of this
repository's code.

#### B8 · Golden template

**Status:** Built.

A golden template is a Proxmox template container with docker, the guards and
unattended-upgrades baked in; a stack whose `lxc.template` is
`"clone:<vmid>"` is cloned from it instead of bootstrapped
(`core/src/ops/deploy.rs:493-532`). The clone inherits the template's
privilege level, so a mismatch is refused:
`pct clone cannot change this, it always inherits the template`
(`core/src/ops/deploy.rs:512-520`). New stacks get `clone:998` from the
scaffold (`client/src/scaffold.rs:71`).

```text
homelab template-build [vmid] [ver] [--privileged] [--base <vztmpl>]
```

The build vmid defaults to 999 and the version to 1
(`client/src/main.rs:507-508`); the base defaults to
`local:vztmpl/debian-12-standard_12.12-1_amd64.tar.zst`
(`core/src/ops/template.rs:48-60`). The build refuses a vmid that exists or is
on the no-touch list (`core/src/ops/template.rs:113-126`), and **the build
vmid becomes the template** (`core/src/ops/template.rs:291-311`). The name is
derived: `<os>-homelab-v<ver>`, plus `-priv` with `--privileged`
(`core/src/ops/template.rs:76-97`).

**Worked example: bake a new template.**

```bash
homelab templates                        # 1. list clonable templates and OS tarballs
homelab template-build 994 5 --base <vztmpl from the list>
```

1. `homelab templates` prints `clonable golden templates (fast):` with one
   `clone:<vmid>  <hostname>` line each, then `OS templates (full bootstrap):`
   (`host/src/main.rs:3899-3934`). Pick a base from the second list and a
   vmid that does not exist yet; 994 above is an example.
2. When the build finishes the log says
   `[template] <name> ready — set template: "clone:<vmid>" in StackDefaults/manifests for fast provisioning`
   (`core/src/ops/template.rs:314-320`), with the vmid you chose.
3. Put `template: "clone:994"` under `lxc:` in the stacks that should use it.
   The deploy's bootstrap still runs over a clone and skips what is already
   there (`core/src/ops/deploy.rs:849-859`).

### C · Provisioning and lifecycle

#### C1 · Declarative containers from `lxc-compose.yml`

**Status:** Built.

A compose stack is a directory `stacks/<name>/` with `lxc-compose.yml`, one
directory per app holding its `docker-compose.yml`, and optionally
`traefik-routes.yml` and `rootfs/`. From `stacks/syncthing/lxc-compose.yml`,
comments removed:

```yaml
stack_name: syncthing
vmid: 108
hostname: 108-app-syncthing
network:
  ip: 10.10.10.8/24
  gateway: 10.10.10.1
  bridge: vmbr0
  vlan: 10
resources:
  cores: 2
  memory_mb: 1024
  swap_mb: 0
  disk_gb: 4
  storage: local-lvm
lxc:
  template: "clone:998"
  unprivileged: true
  features: "nesting=1,keyctl=1"
boot:
  onboot: true
  order: 50
storage:
  - host_path: /appdata/syncthing/syncthing-config
    mount_point: /appdata/syncthing/syncthing-config
    host_owner_uid: 101000
    app: syncthing
apps: [syncthing]
gateway_route:
  filename: 108-app-syncthing.yml
  gateway_vmid: 104
```

All fields: `core/src/manifest.rs:11-342`. The rules the validator enforces
(`core/src/manifest.rs:607-700` and on):

- `stack_name` and app names are lowercase `[a-z0-9-]`; `vmid` is 100 to 354;
  `hostname` is `<vmid>-app-<stack_name>`; `network.ip` is CIDR.
- `memory_mb` at least 128, `cores` at least 1, `disk_gb` at least 2.
- At least one app, unless `native_only: true` with `natives` listed.
- `storage` paths live under `/appdata/` and are named `<app>-config`; the
  owning app is `app:` (deployment decision D25 in
  `docs/deployment/REGISTER.md`: one restic repository per owning app).
- `data_mounts` are directories the stack borrows (media libraries): never
  created, never backed up, and the deploy refuses when one is missing
  (`core/src/manifest.rs:53-71`, `core/src/ops/deploy.rs:328-329`).
- Unknown keys are refused, so a typo such as `latch_secret:` cannot pass
  (`client/src/spec.rs:10-21`).

A **`rootfs/` directory** maps onto the container's `/` for two places only:
`rootfs/etc/systemd/system/` (units and timers) and `rootfs/usr/local/bin/`
(pushed with mode 755). Anything else under `rootfs/` fails validation
(`core/src/manifest.rs:355-403`). A changed unit or timer reloads systemd, and
a changed timer is enabled and started; a service is never restarted by this
(`core/src/ops/deploy.rs:1098-1130`). `stacks/kyu/rootfs/` is the example in
the repository.

**Worked example: check a stack without the host.**

```bash
homelab plan stacks/syncthing
```

`plan` builds the payload and runs the same validator the host runs, with no
network (`client/src/main.rs:673-691`). It prints
`✓ valid — <stack> would deploy vmid <vmid>: <n> file(s), <m> env(s)` or
`error: validation failed: <every problem, joined with "; ">`. A stack with
`latch_secrets` still calls latch (D12).

#### C2 · Gated destroy

**Status:** Built. Command line only; the TUI has no destroy key.

```bash
homelab destroy stacks/drill
```

The client asks `Type the stack name '<stack>' to confirm destroy: ` and stops
with `name mismatch — aborted` on anything else (`client/src/main.rs:965-977`).
The host then runs, in order: `confirm`, `no-touch check`, `hostname guard`,
`backup before destroy`, `stop container`, `lift protection`,
`destroy container` (`pct destroy --purge`), `remove metrics discovery`,
`remove grafana dashboard`, `remove gateway route`, `update state`
(`core/src/ops/destroy.rs`). `/appdata` and the secrets vault are kept, so a
later deploy of the same stack gets its data back through E3.

The backup before a destroy is taken on every destroy. If it fails, the
destroy is refused with `pass --no-backup to destroy anyway, which is a decision, not a retry`.
`--no-backup` skips it and says so (`client/src/main.rs:955-964`,
`core/src/ops/destroy.rs`, step `backup before destroy`). A stack without
storage is destroyed with a warning that anything inside the container goes
with it. Tests: `core/tests/m4_ops_tests.rs:94,114,133`.

`homelab destroy` reads the whole stack, secrets included
(`client/src/main.rs:953`), so a stack with `latch_secrets` needs latch and
`HOMELAB_LATCH_ENV` even though a destroy uses no secret.

#### C3 · Boot policy fleet-wide

**Status:** Built.

`boot.onboot` (default true) and `boot.order` are set when the container is
created (`core/src/manifest.rs:216-222`, `core/src/ops/deploy.rs:556-567,642-655`)
and put back on every later deploy when they drifted (W3). The scaffold
writes `order: 99` (`client/src/scaffold.rs:75`); lower numbers start first.

#### C4 · Hot-apply resources

**Status:** Built.

Raise `memory_mb`, `cores`, `swap_mb` or `disk_gb` in the stack file, then:

```bash
homelab resize stacks/syncthing
```

The client prints `▶ resize <stack> :: <MiB> MiB / <cores> cores / <disk>G`
(`client/src/main.rs:482-503`). The host re-checks the no-touch list and the
live hostname, then applies RAM, cores and swap with `pct set` and grows the
disk with `pct resize`, without a restart (`core/src/ops/resize.rs`). Lowering
RAM or cores is refused while the container runs
(`shrink refused while running`); lowering the disk is always refused
(`disk shrink refused`). Like destroy, `resize` reads the whole stack,
secrets included (`client/src/main.rs:487`). Test:
`core/tests/m4_ops_tests.rs:1227`.

#### C5 · Template list from the live host

**Status:** Built, narrower than FEATURES.md: `homelab templates` lists what
the host has (see B8); the wizard does not use it.

#### C6 · Capacity overview

**Status:** Built, narrower than FEATURES.md.

The DASHBOARD's CAPACITY panel shows RAM used, total and free from `free -m`,
the RAM all stored stack files ask for together with its ratio to the total
(`alloc <MB> MB <ratio>× ok`), and the one-minute load against the core count
(`host/src/main.rs:4167-4180,1911-1943`, `client/src/tui/view/dashboard.rs:22-110`).
Not built: a warning in the wizard when a new stack would overcommit.

#### C7 · Native Rust services under systemd

**Status:** Built.

A native service is a binary run by systemd inside its own container, without
docker. It is described by a `service.yml`: at the top of the stack for a
single service (`stacks/almanac/service.yml`), or one per unit directory when
a container runs several (`stacks/kyu/kyu-runner/service.yml`)
(`client/src/spec.rs:1453-1498`). The unit file `<unit>.service` sits beside it.

| Field | Meaning | Source |
|---|---|---|
| `stack_name`, `vmid`, `hostname` | the container; hostname is `<vmid>-app-<stack_name>` | `core/src/native.rs:28-31,115-121` |
| `unit` | systemd unit name without `.service` | `core/src/native.rs:33` |
| `binary` | absolute path of the program | `core/src/native.rs:35` |
| `env_file` | the unit's EnvironmentFile, if any | `core/src/native.rs:38` |
| `data_dirs` | where its state lives; required unless `stateless: true` | `core/src/native.rs:44,56` |
| `update_cmd` | its own self-update command; absent means never updated by the host | `core/src/native.rs:48` |
| `release_repo`, `release_asset` | `owner/name` on GitHub and the asset (default: the unit name) | `core/src/native.rs:64-68,88-90` |
| `backup_from_newest` | archive the newest file matching this glob instead of `data_dirs` | `core/src/native.rs:79` |
| `update_policy` | `auto` or `manual` (default) | `core/src/native.rs:83` |

The verbs:

| Verb | What it does | Source |
|---|---|---|
| `homelab adopt stacks/<name>` | verifies a running, hand-built service matches its `service.yml` and records it; never starts or restarts anything | `client/src/main.rs:174-191`, `core/src/ops/native.rs:31-229` |
| `homelab install-native stacks/<name>[/<unit>] [<tag> \| --file <path>]` | installs a binary from a release (latest by default) or from a file, with the previous binary kept and a rollback armed | `client/src/main.rs:195-287`, `core/src/ops/native.rs:265-512` |
| `homelab backup-native <stack>` | archives the service's state from inside the container into restic | `core/src/ops/native.rs:515-665` |
| `homelab update-native <stack>` | runs the service's own `update_cmd` under supervision | `core/src/ops/native.rs:1174-1333` |
| `homelab release-update-native <stack>` | installs the latest release when its checksum differs from the installed binary | `core/src/ops/native.rs:931-1131` |

`backup-native`, `update-native` and `release-update-native` act on the copy
of the service files the host recorded at adoption, and on every service of
the stack in turn (`host/src/main.rs:3441-3555`). A stack the host does not
know answers `adopt it first`, followed by the adopt command
(`host/src/main.rs:3121-3124`).

Adoption refuses an inactive unit (`adoption never starts services; start it yourself and re-run`),
a unit that runs a different binary or reads a different env file
(`fix the stack file to match reality`), and missing paths
(`core/src/ops/native.rs:51-110`). It tags the container `homelab` and sets its
description (`core/src/ops/native.rs:119-183`). Tests:
`core/tests/native_tests.rs:87,111,125,139`.

**Supervised self-update** (`update-native`, and nightly for every native
service): the binary is copied to `<binary>.homelab-prev`, `update_cmd` runs,
and when the binary's checksum changed the unit is checked; a unit that does
not come up healthy gets the previous binary back from outside, and the
operation fails with `rolled back to the previous binary`. An unchanged
binary means no restart (`core/src/ops/native.rs:1211-1283`). Tests:
`core/tests/native_tests.rs:212,246`.

**Release updates and signatures.** A native release is installed only when
its `SHA256SUMS` carries a valid minisign signature (`SHA256SUMS.minisig`) and
the binary matches that list (`core/src/release_sig.rs`,
`core/src/ops/native.rs:982-1030`, `client/src/release.rs:75-76,127-168`). An
unsigned release is skipped, not failed:
`is not signed yet — skipped, tried again next night`
(`core/src/ops/native.rs:984-993`). The host's own releases (H7) are checked
against `SHA256SUMS` only, without a signature (`client/src/release.rs:57-61`).
Tests: `core/tests/native_tests.rs:1473,1508`.

**Update policy** (deployment decision B1, `docs/deployment/UPDATE_POLICY.md`
and `docs/deployment/REGISTER.md`, row B1): a service with `update_policy: auto`
gets the latest signed release installed by the nightly run before its
supervised self-update; `manual` services are never release-updated at night
(`host/src/main.rs:2393-2408`). `update_policy: auto` without a
`release_repo` fails validation (`core/src/native.rs:163-167`). The host reads
the policy from its own copy, so after changing it run
`homelab adopt stacks/<name>` to refresh that copy
(`core/src/ops/native.rs:1195-1206` says the same for `update_cmd`).

**A deploy never replaces an installed native binary.** `homelab deploy` of a
native stack ships each binary only when none is installed yet; otherwise it
logs `already installed, not shipped — upgrades go through the nightly updater or`
followed by the release-update command (`core/src/ops/deploy.rs:2175-2197`,
tests `core/tests/deploy_tests.rs:2649,2677`).

**Worked example: the rollback drill** (deployment decision B7,
`docs/deployment/REGISTER.md`, row B7). `scripts/drill-native-rollback.sh`
deploys the throwaway `stacks/drill` (one fake service, `drillsvc`), installs
a good script with `homelab install-native stacks/drill/drillsvc --file <good>`,
then a broken one the same way, and reads from the container that the unit is
active and the binary is still the good one; then it destroys the drill stack
with `--no-backup` (`scripts/drill-native-rollback.sh:1-26`). The drill
stack claims vmid 119 (`stacks/drill/lxc-compose.yml:19`); when that vmid
holds another container, the drill's deploy is refused by A2.

### D · Deployment and gitops

#### D1 · Push-sync over one secured line

**Status:** Built.

The client builds the whole payload (files, secrets, checks, route, native
binaries) and sends it over the TLS WebSocket; the host puts it into the
container and runs compose. Nothing inside a container pulls from git.

```bash
homelab deploy stacks/syncthing
```

**Worked example: what a deploy does.** The client validates first and
prints `▶ deploy <stack> :: vmid <vmid> :: <n> file(s), <m> env(s)`, plus one
`[env] <app> <- <source>` line per app with secrets
(`client/src/main.rs:692-709`, `client/src/spec.rs:316-332`). Each native
binary is sent in a message of its own first (`client/src/main.rs:710-746`).
The host then runs these steps in this order (`core/src/ops/deploy.rs`, the
`step!` calls from line 228 to 2531):

| Step | What it does |
|---|---|
| `validate` | the same validator as the client (D10) |
| `safety gates` | A1 and A2; nothing runs before this passes |
| `baseline` | first reading of every `checks.yml` check (B3) |
| `registry cache` | which image registries the house cache answers for, when one is configured |
| `hardware readiness` | W1: GPU, TUN and data mounts exist |
| `host storage` | creates `/appdata/...` directories and sets their owner |
| `auto-restore check` | E3: refills empty data directories from their latest snapshot |
| `metrics discovery` | writes this stack's Prometheus target file, when configured (F4) |
| `provision container` | creates or clones the container (C1, B8), attaches mounts, H4 devices, W3 boot policy, protection flag; starts it if stopped |
| `wait for systemd` | waits until the container reports running or degraded |
| `bootstrap docker` | installs docker when missing; skipped for native-only stacks |
| `runaway guards` | B2 and A7 |
| `log rotation` | rules for `data_mounts` that ask for them (B2) |
| `commit intent` | D4 |
| `push files` | files, `rootfs/` files and `.env` files (A5); verified by hash afterwards |
| `start apps` | `docker compose up` per app |
| `storage ownership` | each app can write its own data directory |
| `verify health` | B3 |
| `gateway route` | H1, when the stack has a route |
| `grafana dashboard`, `homepage services`, `uptime monitors` | fleet-wide generated files, each only when configured on the host |
| `orphan files` | reports files the repository no longer has (D3) |
| `garbage collect` | D3 |
| `log shipper` | F1, when configured |
| `native units` | installs unit files and missing binaries (C7) |
| `record state` | stores the manifest, the intent hash and the flags |
| `reconcile`, `service checks` | B3 |

A message larger than 256 MiB is refused by the client before it is sent,
with a message that says it is a size limit and not a network fault
(`client/src/version.rs:59-73`, `client/src/main.rs:1076-1081`).

#### D2 · Activation gating

**Status:** Not built. A stack's first deploy records it as enabled
(`core/src/ops/deploy.rs:2378`), and a deploy starts a stopped container
(`core/src/ops/deploy.rs:812-816`). The per-stack flag that exists is H8.

#### D3 · App add/remove and garbage collection

**Status:** Built.

Add an app: create `stacks/<stack>/<app>/docker-compose.yml`, add the name to
`apps:`, deploy. Remove an app: delete it from `apps:` and deploy; the host
runs `docker compose down --remove-orphans` for it and deletes
`/opt/<stack>/<app>`, logging `app '<app>' removed (config dirs kept)`
(`core/src/ops/deploy.rs:1909-1946`, test `core/tests/m4_ops_tests.rs:2206`).
Its `/appdata` directory stays.

A *file* removed from the repository is only reported by the deploy, never
deleted: `[orphans] <n> file(s) on the container that the repository no longer has`
(`core/src/ops/deploy.rs:1884-1907`). To delete them:

```bash
homelab prune-orphans stacks/syncthing
```

It asks for the stack name like a destroy, re-checks A1 and A2 on the host,
and removes each listed file with `rm -f`, never a directory. `.env` files are
never listed (`client/src/main.rs:914-948`, `host/src/main.rs:3556-3623`,
`core/src/ops/deploy.rs:130-161`). Like destroy, it reads the whole stack,
secrets included (`client/src/main.rs:923`).

#### D4 · Intent history on the host

**Status:** Built, narrower than FEATURES.md.

Every deploy writes the stack's files into `/var/lib/homelab/repo/stacks/<stack>/`
and commits them as `deploy <stack>`; secrets are not among them (A5). A
commit that fails for any reason other than "nothing to commit" fails the
deploy, because history, mirror and plan all depend on it
(`core/src/ops/deploy.rs:907-970`). There is no revert verb: the deploy always
sends the files from your checkout, so rolling a configuration back means
putting the old files back in the repository and deploying. The TUI's plan
(D6) compares against this history.

#### D5 · Offsite mirror of the intent repository

**Status:** Built. Off until `mirror_remote = "<git url>"` is set in
`/etc/homelab/host.toml`.

After every successful operation, and on every 20-minute scheduler tick, the
host pushes all branches of the intent repository to the git remote named
`mirror`, adding it on first use (`host/src/main.rs:2787-2798,3040,2261`,
`core/src/ops/mirror.rs`). A failed push is logged as
`mirror push failed (will retry)` and never fails the operation. Tests:
`core/tests/m4_ops_tests.rs:1015,1043`.

#### D6 · Change plan before apply

**Status:** Built.

On the command line, `homelab plan stacks/<name>` validates without the host
(see C1). In the TUI, `p` on a stack found under `stacks/` asks the host for
the files it last applied and shows, per app, `SKIP` (no changes) or `UPDATE`
with the changed files and up to three added and three removed lines each,
`REMOVE` for files the host has and the checkout does not, and the payload
size. A stack without a local directory gets a `CREATE` plan. `ENTER` deploys
exactly the planned payload, `ESC` cancels (`client/src/tui/model.rs:669-688,1612-1744`,
test `client/tests/tui_snapshot_tests.rs:1327`).

#### D7 · Preset catalog

**Status:** Built.

A preset is a directory `presets/<name>/` with a `preset.yml` and one
subdirectory per app holding the files to copy. When the files are copied,
`__STACK__`, `__VMID__`, `__HOSTNAME__` and `__IP__` are replaced
(`client/src/scaffold.rs:101-125,308-314`). `preset.yml` fields:
`description`, `ram_mb`, and optional `cores`, `disk_gb`, `features`,
`unprivileged`, `gpu`, `vpn` (`client/src/scaffold.rs:109-140`). Directories
starting with `_` are skipped; `custom` is listed last; with no `presets/`
directory the built-in fallback list is used
(`client/src/scaffold.rs:155-202`).

```bash
homelab presets
```

prints one line per preset: name, RAM in MiB, description and its apps, with
`(built-in fallback)` when it came from the fallback list
(`client/src/main.rs:648-669`). Run it from the repository root. How to add a
preset: [PRESET_GUIDE.md](PRESET_GUIDE.md); converting a vendor compose file:
[LLM_COMPOSE_CONVERSION.md](LLM_COMPOSE_CONVERSION.md).

#### D8 · Core apps in every new stack

**Status:** Built, with an empty list. The mechanism copies every app named in
`core_apps` from `presets/_core/<app>/` into a new stack; the list is empty
since the deploy installs the log shipper itself (F1)
(`client/src/scaffold.rs:34-40,78`).

#### D9 · Managed updates with a per-app policy

**Status:** Built, narrower than FEATURES.md.

```bash
homelab update stacks/syncthing            # every app
homelab update stacks/syncthing syncthing  # one app
```

or `SHIFT+U` in the TUI. Per app (`core/src/ops/update.rs:121-270`):

1. **policy**: the nightly run only touches apps whose container carries the
   label `com.homelab.update.policy=auto`; an update you start yourself
   touches every app you named.
2. **capture** the running images (B6).
3. **busy check**: only Jellyfin is asked whether anybody is watching, and an
   answer it cannot read counts as busy, so the app is skipped
   (`core/src/ops/busy.rs`).
4. **pull** while the app still runs.
5. **stop-first**: containers labelled `com.homelab.update.stop-first=true`
   are stopped with a 60-second grace first.
6. **up** with `docker compose up -d --remove-orphans`.
7. **verify**, and roll back when the app is not running (B6).

The command sends only the stack file, so no secrets are needed
(`client/src/main.rs:779-807`). Tests: `core/tests/m4_ops_tests.rs:662,694,737`.
Which apps are `auto` is recorded in `docs/deployment/UPDATE_POLICY.md`; for
example `stacks/syncthing/syncthing/docker-compose.yml:22` carries
`com.homelab.update.policy=auto`.

Not built: registry polling and update badges, and the
`auto-after-N-days` policy (`docs/deployment/UPDATE_POLICY.md:26-28`).

#### D10 · Pre-flight validation

**Status:** Built, narrower than FEATURES.md.

One validator, `core/src/manifest.rs`, runs in the client before `plan`,
`deploy` and `import` finish (`client/src/main.rs:469-471,679,698`) and on the
host as the first deploy step. Backup, restore, update and resize run the
manifest part of it (`core/src/manifest.rs:445-458`). It reports every
problem at once, joined with `; `. Not built: `docker compose config` as a gate
before anything runs.

#### D11 · Stack export and import

**Status:** Built.

```bash
homelab export stacks/syncthing /tmp/syncthing-bundle.yml
homelab import /tmp/syncthing-bundle.yml notes 150
```

`export` writes one YAML with the manifest and every file, never a `.env`,
and prints `✓ exported — <out> (<n> file(s), no secrets)`
(`client/src/main.rs:437-457`, `client/src/spec.rs:1315-1338`). It builds the
full payload first, so a stack with `latch_secrets` calls latch even though
the secrets are then left out. `import` writes `stacks/<new-name>/` relative to
the current directory, refuses an existing one, and replaces the old identity
everywhere: name, vmid, hostname, IP (`10.10.10.<vmid - 100>`, keeping the
prefix length), `/appdata/<old>/` paths and `<old>_net` network names
(`client/src/spec.rs:1344-1407`). It then validates the result and prints
`✓ imported — <dir> (vmid <vmid>) :: add .env files if the apps need secrets, then deploy`
(`client/src/main.rs:458-481`). Test: `client/tests/tui_snapshot_tests.rs:715`.
The paths and the vmid above are examples.

#### D12 · Secrets from latch

**Status:** Built.

Name the apps in the stack file and choose the latch environment:

```yaml
latch_secrets: [homepage]
```

```bash
HOMELAB_LATCH_ENV=prod homelab deploy stacks/home
```

`HOMELAB_LATCH_ENV` can also live in `~/.config/homelab/env` or `./.env`
(`client/src/main.rs:68-86`). For each named app without a local `.env`, the
client runs `latch cat <stack>/<app>/.env --env <env> --expand` from the
parent of the stack directory, which makes `stacks/` the latch project root,
and keeps the result in memory (`client/src/spec.rs:334-412`). A local
`stacks/<stack>/<app>/.env` wins over latch, and every app's source is printed
(`client/src/spec.rs:96-120,316-332`):

```text
[env] homepage <- latch
[env] homepage <- local .env (latch skipped)
```

Refusals, all before anything is sent (`client/src/spec.rs:346-408`):

- `latch_secrets names '<app>' but the stack has no such app`
- `latch_secrets is set but HOMELAB_LATCH_ENV is not`
- `cannot run latch for app '<app>'`
- `latch cat <path> --env <env> failed: <latch's message>`
- `latch returned empty content for app '<app>'`

Verbs that build the full payload call latch: `plan`, `deploy`, `export`,
`destroy`, `resize`, `prune-orphans`, and every per-stack action in the TUI
(`client/src/tui/model.rs:1296-1299`). On the command line, `backup`,
`restore` and `update` send only the stack file and need no secrets
(`client/src/spec.rs:68-84`, `client/src/main.rs:749-807`). Test:
`client/tests/latch_secrets_tests.rs:62`. Stacks that use it today include
`stacks/home/lxc-compose.yml:68` and `stacks/gateway/lxc-compose.yml:130`.

### E · Backup and recovery

#### E1 · Restic backups per owning app, with retention

**Status:** Built.

```bash
homelab backup stacks/syncthing
```

or `SHIFT+B` in the TUI. Only the stack file is sent; the backup runs on the
host against `/appdata` (`client/src/main.rs:749-757`). Repositories are
`<restic_base>/<owner>-config`, one per owning app (deployment decision D25),
with `restic_base` defaulting to `rclone:gdrive:homelab-backups` and the
password in `/var/lib/homelab/secrets/restic.pw` (`core/src/ops/backup.rs:89-133`).

**Worked example: which repositories a stack writes.** For the syncthing stack
file in C1, the one storage entry has `app: syncthing`, so its data goes to
`rclone:gdrive:homelab-backups/syncthing-config`. A storage entry without
`app:` belongs to the stack itself (`core/src/manifest.rs:337-342`). The steps
(`core/src/ops/backup.rs:297-708`):

| Step | Effect |
|---|---|
| `safety gates` | A1, A2 |
| `owner conflict` | when two stacks claim the same owner, the stack recorded first keeps the repository and the other is refused |
| `in use?` | when Jellyfin is in use or cannot tell, the backup stands aside (`deferred`), stops nothing and records no time |
| `declared-empty paths` | a `no_data: true` path that holds files stops the backup |
| `declared paths exist` | a missing path stops it, saying to deploy first |
| `init repos`, `clear stale locks` | idempotent preparation |
| `quiesce` | stops containers labelled `com.homelab.backup.pause=true` and remembers which |
| `snapshot` | one restic snapshot per owner; a snapshot with no files is an error |
| `resume` | starts exactly what was stopped, then `compose up -d` per app; runs even when the snapshot failed |
| `retention` | G8 tiers, or the stack's own (W2) |

Two storage flags change this (`core/src/manifest.rs:228-262`):
`no_data: true` means the app keeps nothing (no repository, and the path must
stay empty); `no_backup: "<reason>"` means the contents are deliberately not
kept, and the reason must be at least 10 characters
(`core/src/manifest.rs:580-604`). Tests: `core/tests/m4_ops_tests.rs:3264,3358`.

#### E2 · Restore

**Status:** Built.

```bash
homelab restore stacks/syncthing              # latest snapshot
homelab restore stacks/syncthing <snapshot-id>
```

The client prints `▶ restore <stack> from '<snapshot>'`
(`client/src/main.rs:758-778`). The host runs `safety gates`, then
`validate snapshot` (the id must be in every repository of the stack, and
nothing is stopped when it is not), `quiesce stack` (`docker compose down` per
app), `restore data` (`restic restore <snapshot> --target /` per owner),
`resume stack` (always, even when the restore failed) and `verify health`
(`core/src/ops/backup.rs:803-962`). Tests: `core/tests/m4_ops_tests.rs:598,634`.
In the TUI, `SHIFT+R` restores the latest snapshot after you type the stack
name; it is refused for native-only stacks (section 1.2). Read section 1.3
before restoring from the TUI.

A native service with `backup_from_newest` is archived from its own newest
copy; such a copy is a complete database, to be put back as the live file with
any `-wal`/`-shm` beside it deleted (`core/src/native.rs:69-79`).

#### E3 · Auto-restore on empty data during deploy

**Status:** Built.

Before the apps start, every storage path that is empty and has a snapshot is
restored from its latest one; each path is judged on its own, and
`no_data: true` paths are skipped. A restore that fails does not stop the
deploy: it logs
`[e3] AUTO-RESTORE FAILED for <path> — deploy continues with that dir EMPTY; restore it by hand if it held data`
(`core/src/ops/deploy.rs:355-453`). Tests:
`core/tests/m4_ops_tests.rs:1397,1432,1504,1534`.

**Worked example: rebuild a container with its data.** After
`homelab destroy stacks/<name>` the `/appdata` directories still exist, so the
next deploy finds them full and restores nothing. When a data directory is
empty (a new host, or a deleted directory), the same deploy finds it empty,
finds a snapshot, and restores it before the app starts.

#### E4 · Nightly scheduler and quiesce labels

**Status:** Built.

The host looks every 20 minutes. In the configured hour (host local time) it
plans the night (`host/src/main.rs:2257-2333,2000-2035`):

1. Backups of every **enabled** stack whose last backup is more than 20 hours
   old (`host/src/main.rs:1952-1954,2008-2013`), three at a time by default
   (`backup_concurrency`, `host/src/main.rs:164-165,675-677`), all under the
   one operation lock, so they never overlap a deploy
   (`host/src/main.rs:2172-2188`).
2. Then, one stack at a time: image updates for `auto` apps (D9); for native
   stacks the release update of `auto` services and the supervised
   self-update of every service (C7).
3. The host's own backup (`host-meta-config` repository: vault, state,
   TLS files, intent repository, `/etc/homelab/host.toml`,
   `core/src/ops/backup.rs:967-1010`).
4. A restore drill of one repository in turn, when the last passed drill is
   older than `restore_drill_interval_s` (default 90 days,
   `core/src/ops/restoredrill.rs:24`): it restores into a scratch directory,
   judges the result by its file count and largest file, and deletes it
   (`host/src/main.rs:2040-2083,2541-2582`).
5. Device configuration backups and ZFS jobs (E8), when configured.
6. A fleet check of what the host can see, with a notification only when a
   finding is alarming (`host/src/main.rs:2628-2694`).

With no nightly hour set, nothing runs (`host/src/main.rs:2264-2267`). A stack
whose night fails (backup failed or an update failed) is parked (H8). A
deferred backup is neither a failure nor a backup
(`core/src/ops/backup.rs:34-86`).

The quiesce label is set in a compose file:

```yaml
    labels:
      - "com.homelab.backup.pause=true"
```

as in `stacks/gateway/crowdsec/docker-compose.yml:23`.

#### E5 · Offsite to Google Drive

**Status:** Built, narrower than FEATURES.md. The backup target is one base,
`restic_base`, which defaults to Google Drive through rclone
(`core/src/ops/backup.rs:127`) and can be changed in `host.toml`
(`host/src/main.rs:478`). The code keeps no second, local copy. The doctor's
`offsite (Drive)` check reports an expired token (`core/src/doctor.rs:141-158`).

#### E6 · PBS / vzdump safety net

**Status:** Not in this codebase (FEATURES.md: a separate infrastructure
project).

#### E7 · Disaster-recovery runbook generator

**Status:** Built, narrower than FEATURES.md: it runs when you run it, not
after every change.

```bash
homelab runbook                    # writes docs/DR_RUNBOOK.md
homelab runbook /tmp/dr.md         # or anywhere else
```

Run it from the repository root: it reads `stacks/` from the current
directory (`client/src/main.rs:899-913`). It writes plain markdown: recovery
Layers 0 to 5 (what runs where, the daemon, a stack without the daemon, the
daemon's own state, a stack's data, ZFS replicas), one section per stack
(container, apps or native units, how to rebuild it, which repositories hold
its data, which paths are deliberately not backed up), and a full-host rebuild
order (`client/src/spec.rs:496-740,745-1311`). It prints
`✓ runbook written — <out> (<n> stack(s))`. The current output is
[DR_RUNBOOK.md](DR_RUNBOOK.md).

#### E8 · ZFS snapshots and replication

**Status:** Built. Off until jobs are declared in `/etc/homelab/host.toml`:

```toml
[[zfs_jobs]]
source = "HDD2TB"
target = "HDD18TB/replica/HDD2TB"
```

(the shape of the test fixture at `host/src/main.rs:888-890`). Then:

```bash
homelab zfs-replicate
```

or wait for the nightly run. Snapshots are recursive and named
`homelab-YYYYMMDD-HHMM` (`core/src/ops/zfs.rs:42,96-99`). With a common
snapshot the job sends incrementally (`zfs send -RI`); with an empty target it
seeds (`zfs send -R`); with no common snapshot and a target that already holds
snapshots it **refuses** and tells you how to wipe the target yourself
(`core/src/ops/zfs.rs:255-292`, test `core/tests/zfs_tests.rs:168`). No jobs
at all is an error, not a success (`core/src/ops/zfs.rs:171-176`). Retention
uses the same tiers as restic (G8).

### F · Observability

#### F1 · Logs to Loki fleet-wide

**Status:** Built, with Grafana Alloy in place of promtail.

When `loki_url` is set in `/etc/homelab/host.toml`, every deploy, native or
compose, installs Alloy in the container (`core/src/ops/deploy.rs:1958-1967`,
`core/src/ops/logshipper.rs`). An install that fails does not fail the
deploy; it logs a warning that the container is shipping no logs
(`core/src/ops/deploy.rs:1968-1978`). A stack may open syslog receivers for
devices that cannot run a shipper, with `syslog_receivers:` entries of `host`,
`listen` (port 1024 or higher), `protocol` (`udp`/`tcp`) and `format`
(`rfc5424`/`rfc3164`) (`core/src/manifest.rs:95-137,509-563`);
`stacks/gateway/lxc-compose.yml:144` is the example.

#### F2 · Live log streaming in the TUI

**Status:** Built. The LOG_STREAM tab shows every line the host broadcasts
while it works: step logs, commands, results (`client/src/tui/model.rs:440-451`).
The last 500 lines or so are kept (`client/src/tui/model.rs:377-385`). Keys
are in 1.4. It streams the orchestrator's own work, not the containers' logs;
those are in Loki (F1).

#### F3 · Notifications

**Status:** Built. The receiving side (the hub and Home Assistant) is outside
this repository.

After every host operation, and once at start, the host POSTs one JSON event
(`core/src/notify.rs:15-25`, `host/src/main.rs:2833-2936`):

```json
{"source": "homelab-host", "op": "<op>", "label": "<label>", "ok": false,
 "error": "<what> :: <why>", "version": "<host version>"}
```

- The target is the **webhook** set in the SETTINGS tab (G8), sent with
  `notify_auth_bearer` from `host.toml` when set. When it does not answer 2xx,
  `notify_fallback_webhook` is tried, with its own bearer
  (`host/src/main.rs:2876-2934`).
- The same failure (same operation, same error text) is sent at most once per
  20 hours; successes are always sent (`core/src/notify.rs:27-58`,
  `host/src/main.rs:1807-1809`).
- At start the op is `host-online` with label `boot`, and `ok` is false when
  the journal shows interrupted operations (`host/src/main.rs:1825-1846`).
- A stack parked by a failed night sends its own event
  (`host/src/main.rs:2800-2828`).
- Whether the last event arrived is stored, so a broken notification path
  shows up in `homelab check` (`host/src/main.rs:2938-2961`).

#### F4 · Metrics stack

**Status:** Built as an ordinary stack. `stacks/metrics` runs prometheus,
alertmanager and pve-exporter (`stacks/metrics/lxc-compose.yml:53`). With
`metrics_targets_dir` set on the host, every deploy writes the stack's
Prometheus target file and destroy removes it
(`core/src/ops/deploy.rs:455-481`, `core/src/ops/destroy.rs`). With
`grafana_dashboards_dir` set, every deploy writes the stack's generated Grafana
dashboard into the gateway (`core/src/ops/deploy.rs:1608-1666`).
`homelab dashboard <stack> <app>...` prints the same dashboard JSON locally,
without a token, for a container not yet under management
(`client/src/main.rs:856-879`).

#### F5 · Health API

**Status:** Built, narrower than FEATURES.md. `GET /api/health` answers `ok`
and `GET /api/version` the version, both without a token
(`host/src/main.rs:1863-1867`). Host metrics travel over the authenticated
WebSocket (C6), not over HTTP.

```bash
curl -sk https://10.10.10.250:8443/api/health
```

(the address from `config/client.toml`; `-k` because the certificate is
self-signed).

#### F6 · Doctor

**Status:** Built.

```bash
homelab doctor
```

or the DOCTOR tab. Checks (`core/src/doctor.rs:45-188`): host disk free
(`Warn` under 20 %, `Fail` under 10 %), whether `state.json` parses, per stack
whether the container exists and how old its last backup is (`Warn` over 48
hours or never), the Google Drive token when configured, mirror lag when
known, and interrupted operations. Output (`host/src/main.rs:4136-4163`):

```text
doctor: <Ok|Warn|Fail>
  [<Ok|Warn|Fail>] <check> — <detail>
        ↳ <remedy>
```

The command exits 1 only when a check is `Fail`.

### G · UX and TUI

#### G1 · The control deck

**Status:** Built. Section 1 covers the keys. A splash screen opens on any
key or by itself after a moment (`client/src/tui/model.rs:403-405,612-615`).
Effects cycle with `F2`; with effects off there is no reveal animation
(`client/src/tui/model.rs:344-349`). Screens are covered by snapshot tests in
`client/tests/tui_snapshot_tests.rs`.

#### G2 · New-stack wizard

**Status:** Built.

`n` on DASHBOARD or STACKS. Steps (`client/src/tui/model.rs:1756-1960`):

1. **Preset**: `UP`/`DOWN`, `ENTER`. The list comes from `presets/` (D7).
2. **Name**: the preset's name is suggested; the first letter typed replaces
   it. Lowercase letters, digits and `-`.
3. **Resources**: `UP`/`DOWN` pick RAM, cores, disk, swap or vmid;
   `LEFT`/`RIGHT` change it. RAM steps 256, 512, 1024, 2048, then by 1024 up
   to 32768; cores 1 to 16; disk by 2 GiB (or type digits) from 2 to 999; swap
   by 256 up to 4096, and follows RAM (a quarter, between 512 and 2048) until
   you change it; vmid 108 to 354 (`client/src/tui/model.rs:202-216,1806-1900`,
   `client/src/scaffold.rs:88-93`). The proposed vmid is the lowest from 108
   that no stack in the fleet view uses; Proxmox itself is not asked
   (`client/src/tui/model.rs:1746-1754`).
4. **Storage** (only when the preset has `/appdata` paths): `SPACE` marks a
   path as "keeps nothing" (`no_data: true`), `ENTER` continues.
5. **Review**: `ENTER` writes `stacks/<name>/`; `ESC` goes back a step at
   every stage.

The same scaffold from the command line:

```bash
homelab new notes --preset syncthing --vmid 150
homelab new notes --preset syncthing --vmid 150 --ram 1024 --cores 2 --disk 8 --swap 512 \
  --no-data /appdata/notes/syncthing-config
```

Defaults: RAM from the preset, cores from the preset or 2, disk from the
preset or 8, swap by the formula (`client/src/main.rs:598-623`). It prints
`✓ scaffolded <dir> — <n> file(s)`, the file list, and
`next: read the compose files, then` followed by the plan command
(`client/src/main.rs:629-644`). The stack file gets the address
`10.10.10.<vmid - 100>`, `template: "clone:998"`, `protection: true`,
`order: 99`, and one storage entry per `/appdata` bind in the copied compose
files (`client/src/scaffold.rs:368-369,57-86,470-500`). Run it from the
repository root. The name `notes` and vmid 150 are examples.

**After the wizard, deploy from the command line.** The wizard's status line
says `press SHIFT+D to deploy` (`client/src/tui/model.rs:1945-1949`), but
`SHIFT+D` acts on the stack under the cursor, and the list holds only stacks
the host already manages (`client/src/tui/view/stacks.rs:38-40`,
`client/src/tui/model.rs:1287-1295`). A new stack is not in it yet. Use:

```bash
homelab plan stacks/notes
homelab deploy stacks/notes
```

#### G3 · Command palette

**Status:** Built. `CTRL+K` (or `CTRL+P`). Typing filters the actions by
substring, case-insensitive; `UP`/`DOWN` choose, `ENTER` runs, `ESC` closes
(`client/src/tui/model.rs:1186-1230`). The actions: the six tabs, refresh,
run doctor, backup, update, restore (asks first), guards, adopt,
install-native, fleet check, incidents, cycle effects, help and quit
(`client/src/tui/model.rs:1104-1184`). Test:
`client/tests/tui_snapshot_tests.rs:1438`.

#### G4 · Remote shell tab

**Status:** Built as a line-based REPL, not a terminal. Each `ENTER` sends
one A6 exec to the target stack's container and prints the answer; while
remote exec is off in `host.toml`, every line gets the A6 refusal
(`client/src/tui/model.rs:1016-1074`). Each line is its own `sh -c`, so a
`cd` does not carry over to the next line.

#### G5 · Maintenance window mode

**Status:** Won't (FEATURES.md).

#### G6 · Visual data transfers

**Status:** Built, narrower than FEATURES.md. The host reports the bytes of
each file it pushes into a container (`core/src/ops/deploy.rs:1091-1096`); the
TUI draws them in DATA_TRANSFERS and lets each fade after a while
(`client/src/tui/model.rs:399-402,474-489`), and the command line prints
`⇅ <label> <done>/<total> bytes` (`client/src/main.rs:1230-1238`). Backup,
restore and image-pull bytes are not reported.

#### G7 · Demo mode

**Status:** Won't as a feature. `homelab tui --offline` (also `--demo`) runs
the TUI against a built-in fake host (`client/src/main.rs:141,155-157`).

#### G8 · Settings and tiered retention

**Status:** Built.

The SETTINGS tab edits three things on the host: the nightly hour (`off` or
0 to 23), the retention tiers, and the notification webhook. Keys are in 1.4.
`SHIFT+S` sends them; the host refuses an hour above 23 and an empty tier
list, otherwise writes them into `host.toml` and applies them at once:
`settings saved and applied` (`host/src/main.rs:4064-4102`,
`client/src/tui/view/settings.rs:42`).

A tier keeps one snapshot per `every_days` within its span; tiers follow each
other from new to old, and a tier without a span lasts forever. The newest
snapshot is never forgotten (`core/src/retention.rs:10-114`, test
`core/src/retention.rs:175`). The default is daily for 7 days, every 14 days
up to 60 days later, then every 60 days forever (`core/src/retention.rs:20-37`).

**Worked example: add a quarterly tier.** On SETTINGS, press `a`: a tier
"every 30 days, for 90 days" is inserted before the tier that lasts forever
(`client/src/tui/model.rs:979-994`). Move to its rows with `DOWN` and use
`LEFT`/`RIGHT` to step through the allowed values (every: 1, 2, 3, 7, 14, 21,
30, 45, 60, 90, 120, 180; span: 7 to 730 days, then forever,
`client/src/tui/model.rs:925-926`). Press `SHIFT+S`. To read the result from
the command line:

```bash
homelab config
```

which prints (`client/src/main.rs:1239-1258`):

```text
nightly run : <HH:00 | off>
webhook     : <url | off>
retention 1 : every <n> days for <m> days
retention 2 : every <n> days forever
```

The webhook line shows the full URL; do not paste this output where others
can read it.

#### G9 · Own Rust services as images

**Status:** Built as files, no orchestrator code. `templates/rust-service/`
holds a `Dockerfile`, `release-image.yml` and a README for publishing an image
next to each GitHub release; `presets/rust-service/` is a preset with the
service and RabbitMQ that the wizard lists like any other. For a Rust service
run as a native binary instead, see C7.

### H · Network and hardware

#### H1 · Traefik route fragments

**Status:** Built.

A stack with a public name carries `traefik-routes.yml` and declares:

```yaml
gateway_route:
  filename: 108-app-syncthing.yml   # must be <vmid>-app-<stack>.yml
  gateway_vmid: 104                 # default 104
```

The client refuses any other filename, because destroy removes exactly
`<vmid>-app-<stack>.yml` (`client/src/spec.rs:122-153`). The host writes the
fragment only to the gateway vmid and only into the routes directory
(defaults 104 and `/opt/traefik-config/routes`, both settable in `host.toml`)
(`core/src/safety.rs:24-37,90-110`, `host/src/main.rs:461-466`). When that
directory is under `/appdata/`, it is written on the host side; otherwise it
is pushed into the gateway (`core/src/ops/deploy.rs:1584-1606`). Traefik's
file watch picks it up.

#### H2 · OPNsense Kea DHCP reservations

**Status:** Removed. Built and then removed (FEATURES.md amendment of
2026-09-02; deployment decision D99 in `docs/deployment/REGISTER.md`).
Containers take their address statically from the stack file.

#### H3 · Cloudflare DNS automation

**Status:** Won't (FEATURES.md): a wildcard DNS record already covers every
new hostname.

#### H4 · Hardware passthrough

**Status:** Built.

```yaml
lxc:
  gpu: true    # /dev/dri card and render node, with the host's own group ids
  vpn: true    # /dev/net/tun
```

(`core/src/manifest.rs:201-206`). The deploy first checks that the host has
the devices (W1), then passes them in when it creates the container
(`core/src/ops/deploy.rs:589-618,673-698`). A preset can set them:
`presets/jellyfin/preset.yml` has `gpu: true`. Directories the stack only
borrows (media libraries) are `data_mounts`, see C1.

#### H5 · Host self-update with rollback

**Status:** Built. The part that restores the old binary when the new one
never starts lives in the host's systemd setup, outside this repository's
code (`core/src/ops/selfupdate.rs:1-7`).

```bash
homelab self-update <path-to-homelab-host-binary>
```

The client sends the file (`▶ self-update :: shipping <path> (<n> KiB)`,
`client/src/main.rs:835-855`). The host saves it to
`/var/lib/homelab/staged-host` and runs (`core/src/ops/selfupdate.rs:39-131`):

1. `selfcheck candidate`: `<staged> --selfcheck` must exit 0 and print its
   version (`host/src/main.rs:1784-1789`).
2. `backup current`: copies `/usr/local/bin/homelab-host` to `.prev`.
3. `install candidate`.
4. `arm rollback marker`: writes `/var/lib/homelab/selfupdate.pending`.
5. `schedule restart`: restarts the service in 2 seconds, so the answer still
   reaches you.

The new binary removes the marker after 5 seconds of serving and logs
`self-update accepted — now running v<version>` (`host/src/main.rs:1882-1891`).
Normally you use H7 instead, which fetches and verifies a release first.

#### H6 · Fleet OS patching

**Status:** Built, narrower than FEATURES.md.

```bash
homelab patch
```

Every stack in the host's state, one at a time, runs `apt-get update`,
`apt-get dist-upgrade -y` (keeping existing config files), `autoremove` and
`clean`, with 30 minutes per container. No-touch vmids are skipped; the first
failure stops the run (`core/src/ops/patch.rs`, `host/src/main.rs:3371-3385`,
test `core/tests/m4_ops_tests.rs:817`). Not built: a reboot indicator, and a
TUI key or palette action.

#### H7 · Release-driven host update

**Status:** Built. Needs the GitHub CLI `gh`, signed in.

At start the TUI asks GitHub for the newest release of `kennypassenier/homelab`
(`client/src/tui/mod.rs:50-55`, `client/src/release.rs:10-27`). When it is
newer than the connected host, the ticker says
`⬆ HOST UPDATE <tag> available — press u (checksum-verified, auto-rollback armed)`
(`client/src/tui/view/mod.rs:601-604`), and `u` opens a window titled
`UPDATE HOST → <tag>` (`client/src/tui/model.rs:780-792`). A malformed or
equal version never counts as newer (`client/src/release.rs:30-43`).

```bash
homelab release-update           # newest release
homelab release-update v3.58.2   # a named tag
```

Both download the `homelab-host` asset and `SHA256SUMS` with `gh`, refuse on
a checksum mismatch (`CHECKSUM MISMATCH for ...`), print
`✓ checksum verified — shipping over the line` and hand the binary to the H5
steps (`client/src/main.rs:808-834`, `client/src/release.rs:57-61,166`). The
host release is checked against `SHA256SUMS` only; there is no signature check
for it (see C7). The tag above is an example. Tests:
`client/tests/tui_snapshot_tests.rs:1463,2579`.

#### H8 · Per-stack enabled flag (park / unpark)

**Status:** Built.

```bash
homelab disable syncthing
homelab enable syncthing
```

or `e` in the TUI. The argument is the stack **name**, not a path
(`client/src/main.rs:423-436`). Disable sets `onboot` to 0 and marks the stack
parked; enable restores `onboot` to what the stored stack file says. Neither
starts or stops a container (`core/src/ops/enable.rs`). Tests:
`core/tests/m4_ops_tests.rs:2491,2512,2533`.

A parked stack shows `[OFF]` in the lists and `○ parked [e] to re-enable` in
its detail (`client/src/tui/view/stacks.rs:62-64,117-121`), and the nightly
run skips it and logs `scheduler: stack <name> is disabled — skipped`
(`host/src/main.rs:2365-2371`).

A failed nightly run parks the stack by itself: only the flag, `onboot` is
left alone. The host logs
`nightly run for <name> FAILED — stack auto-disabled (H8); investigate, then re-enable with`
followed by the enable command, and sends a notification
(`host/src/main.rs:2483-2511`). A redeploy keeps the flag as it was
(`core/src/ops/deploy.rs:2362-2378`), but a deploy does start a stopped
container (`core/src/ops/deploy.rs:812-816`).

### W · Added 2026-08-31

#### W1 · Host hardware readiness

**Status:** Built.

Before anything is created, a stack with `gpu: true` has the host's `/dev/dri`
devices and their group ids read; a missing device or unreadable group stops
the deploy with a message naming the stack and the device. `vpn: true` does
the same for `/dev/net/tun`, and every `data_mounts` path must exist
(`core/src/ops/hardware.rs:37-165`, `core/src/ops/deploy.rs:318-337`). The
log shows `[w1] <card> gid <n> · <render> gid <m>`. A stack that asks for no
hardware is never probed. Tests: `core/tests/deploy_tests.rs:1078,1114,1142,1160`.

#### W2 · Per-stack backup retention

**Status:** Built.

```yaml
retention:
  - every_days: 1
    span_days: 4
  - every_days: 30
```

in `lxc-compose.yml` replaces the fleet-wide tiers for this stack, for manual
and nightly backups alike; the backup logs
`[w2] <stack> keeps snapshots by its own policy (<n> tier(s)), not the fleet-wide one`
(`core/src/manifest.rs:72-82`, `core/src/ops/backup.rs:647-667`). The tiers
above are an example. Test: `core/tests/m4_ops_tests.rs:266`.

#### W3 · Boot order and resource reconciliation

**Status:** Built.

On every deploy of an existing container, `onboot` and the startup order are
compared with the stack file and put back, logged as
`[w3] boot policy drifted — <what differs>` (`core/src/ops/deploy.rs:756-779`).
Memory, cores and disk are not touched by a deploy; `homelab check` reports a
difference and names the remedy: `homelab resize` to raise, a rebuild to
lower. Tests: `core/tests/deploy_tests.rs:1003,1042`,
`core/tests/fleetcheck_tests.rs:539,567,592,624`.

---

## 3 · Verbs without a feature ID

These verbs grew out of the deployment project and carry no ID in
FEATURES.md.

### `homelab check`: the repository against the machine

```bash
homelab check                         # reads ./stacks
homelab check ~/Projects/homelab/stacks
```

The client sends the vmid every stack file claims; the host adds what it can
see and compares (`client/src/main.rs:310-338`, `host/src/main.rs:3728-3761`).
From outside the repository with no path, it says the stack-file half is
`SKIPPED` and checks only the host's half (`client/src/main.rs:317-327`). The
answer (`host/src/main.rs:3189-3209`):

```text
fleet check: <n> finding(s)
  [<broken|drift|noted>] <subject> — <what>
      remedy: <remedy>
```

or `fleet check: repo and reality agree`, which is the only answer that exits
0. Among the findings (`core/src/ops/fleetcheck.rs:510-720` and the
`evaluate_*` functions after it): a template a stack clones that does not
exist, a record whose vmid does not exist, parked stacks, backups that never
ran or are old, a stack file that claims a vmid somebody else owns, a route to
nothing, boot policy and resource drift (W3), deploys recorded as incomplete,
and a notification path whose last delivery failed. The host runs the same
check after every nightly run and notifies only when a finding is alarming
(`host/src/main.rs:2628-2694`).

### `homelab checks`: what only a person can confirm

The `manual:` lines of every `checks.yml` (B3) are registered when their
stack is deployed, each with a short id (`core/src/ops/deploy.rs:2662-2697`,
`core/src/ops/manualchecks.rs:29-43`).

```bash
homelab checks                              # list them
homelab checks answer <id> ok               # record an answer
homelab checks answer <id> nok the subtitles drift after an hour
```

The listing is grouped per stack, one line per question with its id and
status (`unanswered`, `ok, <n>d ago` or `NOT OK`), and ends with the count of
answered and open questions (`core/src/ops/manualchecks.rs:221-250`). `ok`,
`yes` and `ja` mean yes; `nok`, `no` and `nee` mean no; everything after the
verdict is the note (`client/src/main.rs:388-421`). An unknown id answers
`no manual check has id <id>` (`host/src/main.rs:3878-3882`). Redeploying
never resets an answer (`core/src/ops/manualchecks.rs:45-50`).

### `homelab forget <stack>`: drop a stale record

Removes a stack from the host's state without touching any container, and only
when no container in `pct list` still carries its hostname; otherwise it
refuses with `this record is current, not stale`
(`host/src/main.rs:3624-3686`). Use it after a container was removed by hand.

### `homelab backup-host-meta` and `homelab backup-devices`

`backup-host-meta` takes the host's own backup (see E4 step 3) now instead of
at night (`host/src/main.rs:3392-3416`). `backup-devices` asks every device
listed under `[[device_backups]]` in `host.toml` for its own configuration
and stores it with restic; without that list it answers
`no device_backups configured in host.toml` (`host/src/main.rs:3809-3847`).

### `homelab status`, `homelab incidents`, `homelab testplan`

`status` prints `pct list` and the raw `state.json` (`host/src/main.rs:3227-3240`).
`incidents` lists the incident bundle directories (A3). `testplan` rebuilds
`docs/deployment/TEST_PLAN.md` from the test files and the realization plan;
run it from the repository root (`client/src/main.rs:882-898`).

---

## 4 · The front page

The Homepage dashboard's `services.yaml` is generated, not maintained. When
`homepage_services_file` is set in `host.toml`, every deploy of any stack
reads every route fragment in the gateway's routes directory and rewrites the
file with one tile per destination (`core/src/ops/deploy.rs:1668-1809`). A
service with a route appears by itself; a service without one does not. The
generated file starts with a comment saying that edits to it are overwritten
on the next deploy (`core/src/ops/homepage.rs:362-373`).

What you decide lives in one file, `stacks/home/homepage/services-overlay.yml`,
joined to the generated list on `href` (`core/src/ops/homepage.rs:194-308`).
From that file:

```yaml
group_order: [Media, Papierwerk, Huis, Eigen, Infrastructuur]

- href: https://fin.kp-soft.dev/
  group: Media
  name: Jellyfin
  extra: |
    icon: jellyfin.svg
    description: Films en series
```

| Key | Effect | Source |
|---|---|---|
| `group_order` | these headings first, in this order; the rest alphabetically | `core/src/ops/homepage.rs:502-512` |
| `href` | the join key; a trailing `/` does not matter | `core/src/ops/homepage.rs:231-236` |
| `group` | the heading; without a block the stack's name is used | `core/src/ops/homepage.rs:436-438` |
| `name` | the tile's name; default the router's name | `core/src/ops/homepage.rs:439-441` |
| `hide: true` | keeps this route off the page | `core/src/ops/homepage.rs:224-228,429-432` |
| `extra: \|` | lines handed to Homepage as they are, indented four spaces | `core/src/ops/homepage.rs:217-218,445-466` |

**Widgets.** For `jellyfin`, `sonarr`, `radarr`, `prowlarr`, `bazarr`, `seerr`
and `paperless` the widget is generated. Its API key is read from the app's
own configuration on every deploy, except for paperless, which reads
`HOMEPAGE_VAR_PAPERLESS` from Homepage's own environment
(`core/src/ops/homepage.rs:137-188`, `core/src/ops/deploy.rs:1749-1792`). A
`widget:` written under `extra` wins over the generated one
(`core/src/ops/homepage.rs:445-466`).

**Recipes.**

- *Change a description or icon*: edit the block's `extra`, then
  `homelab deploy stacks/home`. The host reads the overlay from its own intent
  repository (`core/src/ops/deploy.rs:1716-1748`), so the change takes effect
  when `stacks/home` is deployed; `stacks/home` uses latch (D12).
- *Move a service to another heading*: change `group`; add a new heading to
  `group_order` or it sorts alphabetically after the named ones.
- *A tile appears under a lowercase heading with a plain link*: that route has
  no block yet (`core/src/ops/homepage.rs:468-476`). Add one with its `href`.
- *A link to something with no route*: a block whose `href` joins nothing is
  kept, under its `group` or `Overig` (`core/src/ops/homepage.rs:482-500`).

**Reading the deploy log.** The step logs
`[t51] overlay: <n> entr(y/ies) from <path>`,
`[t51] widget keys read from the applications themselves: <n>` and
`[t51] <file> — <n> stack(s) on the front page`
(`core/src/ops/deploy.rs:1740-1800`). A drop in the key count means an app
moved its configuration.

---

## 5 · Where the code and FEATURES.md part ways

Measured by reading the code for this guide; each item names the line that
shows it.

| # | What | Source |
|---|---|---|
| 1 | D2 activation gating is not built; a first deploy records the stack enabled | `core/src/ops/deploy.rs:2378` |
| 2 | A3 does not disable a stack after a failed deploy, and a missing `.env` does not abort | `core/src/ops/deploy.rs:99,1139-1151` |
| 3 | B6 keeps no digest history; the rollback uses images captured during the same update | `core/src/ops/update.rs:145-151` |
| 4 | D9 has no registry polling, update badges or `auto-after-N-days` | `docs/deployment/UPDATE_POLICY.md:26-28` |
| 5 | D10 has no `docker compose config` gate | `core/src/manifest.rs:445-458,607` |
| 6 | E5 writes one backup target; no separate local copy | `core/src/ops/backup.rs:107-133` |
| 7 | E7 runs on demand, not after every change | `client/src/main.rs:899-913` |
| 8 | G6 reports only file-push bytes | `core/src/ops/deploy.rs:1091` |
| 9 | H6 has no reboot indicator and no TUI action | `core/src/ops/patch.rs`, `client/src/tui/model.rs:1104-1184` |
| 10 | C6 has no overcommit warning in the wizard | `client/src/tui/model.rs:1806-1900` |
| 11 | Fixed in v3.58.4 (gap-19): without a local `stacks/` directory, TUI backup and restore now refuse instead of acting on a manifest with no storage (section 1.3) | `client/src/tui/model.rs`, `start_stack_op` |
| 12 | After the wizard, the status line says `SHIFT+D`, which deploys the stack under the cursor, not the new one | `client/src/tui/model.rs:1945-1949,1287-1295` |
| 13 | Fixed in v3.58.4 (gap-21): the placeholders name `r` | `client/src/tui/view/stacks.rs`, `client/src/tui/view/doctor.rs` |
| 14 | `homelab new` and `homelab testplan` need a token although they never connect | `client/src/main.rs:144-150` |
| 15 | `destroy`, `resize` and `prune-orphans` need latch for stacks with `latch_secrets` although they send no secret | `client/src/main.rs:487,923,953` |
| 16 | Fixed in v3.58.4 (gap-29): the help line for `export\|import` says it writes or reads a stack-definition bundle | `client/src/main.rs` |
| 17 | `homelab guards` and `homelab patch` skip the hostname guard (A2) | `host/src/main.rs:3687-3700`, `core/src/ops/patch.rs` |
| 18 | The DASHBOARD's online dot and app states are not measured: the host reports every recorded stack online and every app running | `host/src/main.rs:4216-4229` |
