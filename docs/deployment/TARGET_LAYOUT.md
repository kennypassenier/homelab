# Target layout — which service runs where

Phase 4 draft, **revision 3 (2026-08-30)**. **Not approved.**

Revision 1 was mine. Revision 2 answered the `architecture-critic`. Revision 3
answers Kenny at the Phase 4 gate, where he declined to freeze and reopened
five things. Two of his objections were right against me:

**The port collision was not a reason to separate services.** He asked whether
anything besides the port kept http-switchboard away from kyu, and guessed it
would be an easy fix. It is: `http-switchboard` takes `--listen` and an
explicit `--healthcheck <url>`, kyu reads `KYU_LISTEN` from its environment
file, and kyu-runner has `healthz_listen` in its config. All three are
configuration, in three files, with no code change in any project. My
objection justified *care*, not a different container.

**10.10.10.10 is Kenny's own desktop.** He asked whether the syncthing route
was really wrong. It is — but not for the reason revision 2 gave. The address
is his workstation (`eno1`, verified from the machine itself), and from the
Proxmox host ports 8384, 22000 and 22 are all closed, so `sync.kp-soft.dev`
resolves to a GUI nothing can reach. Syncing itself is unaffected: the desktop
dials out to the hub, and outbound needs no open inbound port.
The larger consequence nobody had noticed: **vmid 110 can never be used.**
The convention gives a container the last octet `vmid - 100`, and
`presets/syncthing/` targets exactly vmid 110. Creating it would collide with
the workstation.

## The proposal

Ordered the way Kenny asked: metrics between the edge and the downloader, the
rest shifting up. Nine stacks fit exactly into 104-113 with 110 held back.

| # | vmid | stack | services | why here |
|---|---|---|---|---|
| 1 | 104 | `edge` | traefik, cloudflared, crowdsec, goaccess | nothing else reaches the outside world until this is up |
| 2 | 105 | `metrics` | prometheus, alertmanager, pve-exporter | Kenny's placement: measuring comes before the things measured |
| 3 | 106 | `downloader` | gluetun, qbittorrent | `network_mode: service:gluetun` — indivisible |
| 4 | 107 | `media` | jellyfin, sonarr, radarr, bazarr, prowlarr, seerr, flaresolverr, recyclarr | the arr-suite and Jellyfin interact constantly |
| 5 | 108 | `observability` | grafana, loki, uptime-kuma | own container per Kenny's A5 answer; no longer shares a fate with the edge |
| 6 | 109 | `messaging` | kyu, kyu-runner, http-switchboard | everything that moves a message, per Kenny's A4 answer |
| — | **110** | *(reserved, unusable)* | — | 10.10.10.10 belongs to Kenny's desktop |
| 7 | 111 | `productivity` | supersync, postgres | vikunja dropped — see below |
| 8 | 112 | `almanac` | almanac | self-updating, self-reverting |
| 9 | 113 | `syncthing` | syncthing | promoted from `synctest` to production |

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

## Port map for CT 109 (Kenny's A4 answer)

All three services keep their defaults where they can and move where they
must. Nothing needs a code change in any project — three configuration lines.

| Port | Service | How it is set | Whose file |
|---|---|---|---|
| 8080 | kyu | unchanged default | — |
| 8081 | cadvisor | baked into the golden template (O2), fleet-wide | homelab |
| 8082 | kyu-runner `/healthz` | `healthz_listen = "10.10.10.9:8082"` | `kyu-runner/deploy/config.toml` |
| 8083 | http-switchboard | `--listen 0.0.0.0:8083` in the unit's ExecStart | homelab-written unit |
| 8083 | http-switchboard healthcheck | `--healthcheck http://127.0.0.1:8083/healthz` — **the argument is not optional here**: without it the check probes 8080, which is kyu, and a dead switchboard reports itself healthy | homelab-written unit |
| 9100 | node_exporter | baked into the golden template | homelab |

The one thing worth sending to the kyu-runner project: its shipped example
config points `healthz_listen` at 8081, which the golden template now gives to
cadvisor on every container in the fleet. Same file also points `webhook_url`
at `homeassistant.lan`, which violates scope constraint C3 (LXC IPs, never
names).

⚔ **Counter-argument to co-locating all three, kept rather than dropped.**
After this, every path by which Kenny learns anything is wrong runs through one
container: Alertmanager (105) → http-switchboard (109) → Home Assistant, and
Y2 routes the homelab's own operation notifications through kyu (109) as well.
When the orchestrator restarts kyu during an update, the report of that
operation — including the "rollback also failed, this needs hands now" text —
is posted fire-and-forget with a 5-second timeout and no retry, so it is simply
lost in that window. The mitigation is to broaden Y2's exception from "kyu
itself is down" to **any operation whose target is the messaging stack**, which
keeps those on the direct path.

## Vikunja

Dropped. Kenny: "vikunja wordt niet meer gebruikt". Confirmed on the machine —
its database has not been written since 2026-07-08, and the only traffic in a
week of logs is Traefik and Uptime Kuma probing `/`. The container keeps
SuperSync and its Postgres, which the same check confirms is SuperSync's
database and nothing else's (9 connections, all from supersync).

Its data is not deleted: the productivity stack's existing restic history
holds it, and B4 keeps the v1 repos until the new backups are proven.

## What the reordering costs

Every stack that changes vmid changes its IP, and — under today's code — its
stack name too, because the hostname must be `<vmid>-app-<stack>`. That means
the reordering runs straight into the problem Kenny raised at A3: a stack whose
name changes gets a brand-new, empty restic repository.

Four stacks move: `metrics` 113 → 105, `downloader` 105 → 106, `media`
106 → 107, `syncthing` 108 → 113. Two are being rebuilt anyway (105, 106 in
their old numbering), so their cost is already paid. The other two are
currently homelab-managed and would lose their backup history unless A3 is
settled first.

**So A3 is not a side question — it is a prerequisite for the ordering.**
