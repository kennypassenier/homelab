# Target layout — which service runs where

Phase 4 draft, **revision 2 (2026-08-30)**, rewritten after the
`architecture-critic` pass. **Not approved.**

Revision 1 proposed functional renames, adoption of the four ansible-era
containers in place, and a messaging container holding three services. The
critic found five blocking objections, every one of which I verified against
the code or the live fleet before accepting it. Three of them killed parts of
revision 1 outright. What follows is what survives, with the reasoning kept
rather than quietly replaced.

## What the critic changed, and why

**No stack is renamed.** The restic repository path is derived from the stack
name (`core/src/ops/backup.rs:30`, `format!("{}/{}-config", base, stack)`) and
so is the hostname guard. Renaming `metrics` → `observability` would start an
empty repo, orphan the existing history, leave the old key in `state.json`
pointing at a hostname that no longer exists, fail `guard_target` on the next
nightly run and get the stack auto-disabled — which is finding **F7**, word
for word, three more times. Functional names stay documentation; the stack
name stays the identifier. A rename operation (state key move, repo copy,
hostname set) would have to be built first, and nothing needs it enough.

**The four ansible-era containers are rebuilt beside and cut over, not
adopted in place.** In `core/src/ops/deploy.rs` the `pct set -mp<i>` loop
exists on both provisioning paths and both sit inside `if !exists { … }`. A
deploy onto an existing container therefore configures **no mountpoints at
all**: docker would create `/appdata/media/jellyfin-config` on the container's
own rootfs, Jellyfin would start, the deploy would go green, and restic on the
host would snapshot an empty directory. O4's whole promise fails silently on
the stacks that matter most. Verified in the source; `validate()` only checks
that the bind is *declared*, never that it is *mounted*.
Consequence, stated because it is not free: cut-over gives every rebuilt stack
a **new vmid and a new IP**, which triggers D16 — Kenny gets the list of what
he has to reconfigure before it happens, SuperSync being the known case.
And it makes **E5 a hard prerequisite**: CT 190 and 191 must release
10.10.10.14 and .15 before anything can be built beside.

**HTTPSwitchboard goes to CT 113, not to the messaging container.** Three
separate reasons, any one of which is enough. Its default listen address is
`0.0.0.0:8080` and so is kyu's — and its `--healthcheck` with no argument
probes `http://127.0.0.1:8080/healthz`, which on a shared container is *kyu's*
health endpoint answering 200 for a switchboard that may be dead. It already
ships a complete compose preset with a real container healthcheck, which
`FEATURES.md` E2 (frozen) assumes exists. And `SCOPE.md` G2 names the grouping
as "kyu + kyu-runner" — putting a third service there was my addition, not
Kenny's. Moving it also means "CT 109 is down" becomes an alert that can leave
the house instead of one that dies with its own carrier.

## The proposal

| vmid | stack (identifier) | services | why together |
|---|---|---|---|
| 104 | `platform` | traefik, cloudflared, crowdsec, goaccess | one failure domain: the way in. goaccess and crowdsec both parse Traefik's access log off the same disk |
| 105 | `downloader` | gluetun, qbittorrent | `network_mode: service:gluetun` makes the pair indivisible — that pair IS the kill switch |
| 106 | `media` | jellyfin, sonarr, radarr, bazarr, prowlarr, seerr, flaresolverr, recyclarr | Kenny's explicit wish; recyclarr configures sonarr/radarr so it belongs beside them |
| 108 | `synctest` | syncthing | keeps its name; see the open question below |
| 109 | `kyu` | kyu, kyu-runner | Kenny's G2 wording exactly. The vendor designed for it: kyu-runner's unit ships `After=kyu.service` and `hub_url = http://127.0.0.1:8080` |
| 111 | `productivity` | vikunja, supersync, postgres | Kenny's own task and sync data |
| 112 | `almanac` | almanac | self-updating and self-reverting; deliberately left alone |
| 113 | `metrics` | prometheus, alertmanager, pve-exporter, **http-switchboard**, and — open — grafana, loki, uptime-kuma | everything that measures, plus the translator that sits directly beside its only customer |
| — | removed | 107 (empty), 190 and 191 (scratch, and a prerequisite) | |

Every container additionally carries node_exporter, cadvisor and promtail from
the golden template (O2).

## Still open, and deliberately not settled here

**Does observability leave the edge?** Revision 1 said yes. The critic's
objection stands and is not resolved: CT 113 has **1 GB of RAM and 16 GB of
disk** and already holds Prometheus at 90-day retention. Moving Grafana, Loki
and Uptime Kuma there means one full disk takes down every automated watcher
at once — including the one that would have warned about the disk. That is
not obviously better than the split it replaces; it may be worse.
The option neither revision listed: **bound Loki's data path with a quota**
(its own dataset under `/appdata`). That fixes the stated failure mode
wherever Loki runs, and turns co-location into a question about convenience
rather than about survival. This goes to the gate form as a real choice, not
as a recommendation dressed up as one.

**CT 108's identity.** Named `synctest`, running the real syncthing peer. The
critic found the tiebreaker and it is not flattering: `sync.kp-soft.dev` is
**broken right now**. The route file sends it to `10.10.10.10:8384`, where
nothing answers; syncthing is on `10.10.10.8:8384`, which returns 200.
Verified live 2026-08-30. That is a second dead route beside the MQTT one, and
it has been that way unnoticed. Fixing the route is R8; deciding whether the
stack is a test or production is the gate's question.

## Code that has to change before this layout can exist

Named here so the gate form can price them, each verified in the source:

1. **`native.rs` refuses an empty `data_dirs`** (line 73). kyu-runner is
   deliberately stateless — its unit says so and uses `DynamicUser=yes` — so
   it cannot be declared as a native service today. Either the check learns to
   accept an explicit "no state, by decision", or kyu-runner needs a fabricated
   directory, which is worse.
2. **A native stack holds one service** (`StackState.native` is a single
   option). CT 109 needs two. This is T5.
3. **T5 option B is impossible, not merely awkward.** `native.rs` forces
   `hostname == "<vmid>-app-<stack_name>"` and `guard_target` re-checks it live.
   Two stacks on vmid 109 would need two different hostnames on one container.
   The Phase 3 form presents B as a trade-off; it is ruled out by code.
4. **The restore timeout is hardcoded at 1800 s** (`backup.rs:329`) while the
   backup timeout was already raised to four hours for exactly this reason.
   A media or observability restore over Google Drive dies at thirty minutes —
   on the operation you least want to find broken. B3's drills would hit it.
5. **The backup target is outside the configuration surface.** `restic_base`,
   `password_file` and `snapshot_timeout_s` live in `BackupCfg::default()` and
   appear nowhere in `FileConfig`. SCOPE G7 requires a target on the HDDs *and*
   on Google Drive; the code can address exactly one, as a string literal.
6. **Boot policy cannot be applied to a container the orchestrator did not
   create** — `--onboot` and `--startup order=` appear only in the create and
   clone branches. Cut-over solves this incidentally, which is one more
   argument for it.
7. **Nothing validates ports on a shared container**, and nothing knows that
   the golden template will bake cadvisor onto `:8081` — which is the port
   kyu-runner's own shipped example uses for its health endpoint.
8. **CT 109 cannot be resized by the orchestrator**: `resize::hot_apply` takes
   a `&StackManifest` and native stacks store `manifest: None`.

## Counter-arguments to carry into the gate form

- Backing up Loki's chunks and Prometheus's TSDB to Google Drive nightly will
  meet the four-hour ceiling over a residential uplink; one timeout parks the
  stack via H8's auto-disable, and the "loud message" about it travels the
  alert chain that same container serves.
- Health endpoints answer "the process is alive", never "it is doing its job".
  HTTPSwitchboard has the two-endpoint split (`/healthz` vs `?strict=1`) and
  under the orchestrator's supervision only `systemctl is-active` is asked.
  R4's Uptime Kuma monitor must watch the strict endpoint, and that belongs in
  this layout rather than in R4's one-liner.
- The host daemon's own `/api/health` returns the literal string `ok`. If
  `state.json` becomes unparseable the scheduler skips every tick and all eight
  stacks stop being backed up, while that endpoint still says ok — and there is
  no `homelab-host` scrape target today.
- After a power cut Home Assistant needs minutes, and it answers 200 to unknown
  webhook ids the whole time. Alertmanager's alerts repeat and survive; one-shot
  homelab notifications do not. Every subscription this project creates needs an
  explicit policy, the way kyu-runner's shipped config does.
- Recyclarr must run in scheduled mode, never one-shot: `verify health` fails a
  deploy when no service is running, and the nightly backup's resume step runs
  `docker compose up -d` for every app.
