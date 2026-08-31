# Target layout — which service runs where

Phase 4 output. **Frozen 2026-08-30** after three rounds and an
`architecture-critic` pass. Changes go through mini-rounds only.

Revision 1 was mine. Revision 2 answered the critic, who found five blocking
objections. Revisions 3 and 4 answered Kenny, who declined to freeze twice and
was right both times: the port collision was configuration rather than a
reason to separate services, and reordering the containers the way I proposed
would have moved qBittorrent and Jellyfin for no benefit he would ever feel.

Merging metrics and observability — his question — freed the container that
made the reordering unnecessary.

## The layout

| vmid | stack | services | change |
|---|---|---|---|
| 104 | `gateway` | traefik, cloudflared, crowdsec, goaccess | renamed from `platform`; the code already said `gateway_vmid: 104` |
| 105 | `downloader` | gluetun, qbittorrent | unchanged |
| 106 | `media` | jellyfin, sonarr, radarr, bazarr, prowlarr, seerr, flaresolverr, recyclarr | recyclarr added |
| 107 | `metrics` | prometheus, alertmanager, pve-exporter, grafana, loki | moves here from 113; absorbs grafana and loki from 104 |
| 108 | `uptime` | uptime-kuma | new stack; leaves 104 so it no longer watches its own host |
| 109 | `messaging` | kyu, kyu-runner, http-switchboard | two services added |
| **110** | *(reserved, unusable)* | — | 10.10.10.10 is Kenny's workstation |
| 111 | `productivity` | supersync, postgres | vikunja dropped |
| 112 | `almanac` | almanac | unchanged |
| 113 | `syncthing` | syncthing | moves here from 108, renamed from `synctest` |
| 114 | `paperwork` | actual, stirling, paperless, paperless-db | added 2026-08-31, see the amendment below |
| 115 | `home` | homepage | added 2026-08-31, see the amendment below |
| — | removed | — | 107 (empty), 190 and 191 (scratch) |

Every container additionally carries node_exporter, cadvisor and promtail from
the golden template (O2).

**What actually moves: syncthing and the measuring services.** Everything
Kenny addresses from his desktop — qBittorrent, Jellyfin, the arr-suite,
SuperSync — keeps its address. That was his objection to revision 3 and it is
fully answered rather than argued away.

## Why metrics and observability are one stack, and Uptime Kuma is not

Kenny asked whether they could merge. They can, and the reasons are his own
grouping rule applied honestly: it is one function, Grafana is empty without
Prometheus and Loki, and the objection I had raised — different backup
profiles — was dissolved by his own B2 answer, since repositories are now per
app rather than per stack.

Uptime Kuma stays out because its entire value is working when the rest does
not. Beside Prometheus, one container failure removes every automated watcher
at once and nothing is left to say so. Today it is worse: it shares a container
with Traefik, so it cannot report that the edge fell over.

Kenny's naming: the merged stack is `metrics`, not `observability`.

⚔ **Counter-argument, kept:** that is one more container for one small service,
and a watcher running on the same hypervisor is not truly independent — a host
failure blinds everything regardless. Real independence would need something
outside this house.


## Questions that were open, and how they closed

- **Does observability leave the edge?** Yes. Kenny chose an own container at
  A5, then asked at C2 whether metrics could join it. They did, which freed a
  container and removed the need to renumber anything he uses.
- **CT 108's identity.** Settled: syncthing is production and moves to 113
  under its own name. The tiebreaker was that `sync.kp-soft.dev` had been
  broken all along — the route pointed at 10.10.10.10, which is Kenny's
  workstation, where nothing listens on 8384. Syncing itself was never
  affected: the desktop dials out. The route is repaired to the container
  (Kenny, B4).
- **vmid 110.** Permanently reserved and unusable, because that address
  belongs to the workstation. `presets/syncthing/` targets exactly vmid 110
  and must be corrected.

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

## Standing risks accepted with this layout

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

## What the migration costs

Only three stacks change identity: `metrics` moves 113 → 107, `syncthing`
moves 108 → 113 and is renamed from `synctest`, and `platform` is renamed to
`gateway` on the same vmid. Kenny's B2 answer — one restic repository per app
rather than per stack — means none of that costs backup history, which is
exactly why B2 was a prerequisite rather than a side question.

The `synctest-config` repository is the one genuine migration: its contents
belong to an app now called `syncthing`, so its history moves rather than
being abandoned.

## How a container is replaced (Kenny, C4)

He wants the replacement to be complete: same vmid, same IP, nothing for him
to reconfigure. That is possible because configuration now lives on the host
under `/appdata`, which makes the container itself nearly empty — a disposable
box to run software in.

1. Copy the configuration out of the old container into
   `/appdata/<stack>/<app>-config` on the host. The old container keeps running.
2. Build the recipe once on a throwaway vmid and let it work end to end, so
   the recipe is proven before anything is destroyed.
3. vzdump the old container as the safety net.
4. Destroy it and deploy the stack on the same vmid and IP. The configuration
   is already on the host, so it starts with it.
5. Verify. If it fails, restore the vzdump.

Outage: the few minutes between steps 4 and 5 — container creation and start,
with nothing to copy because the data never lived inside.

⚔ **Counter-argument, kept:** there is still a window where the old container
is gone and the new one has not proven itself. Building beside and switching
over has no such window, at the price of a changed address. Kenny chose
identity over that window, with step 2 as the compensation.


## Amendment — 2026-08-31: CT 114 `paperwork`

This layout was frozen with ten guests. Kenny asked for three more services
on 2026-08-31 — Actual Budget, Stirling-PDF and paperless-ngx — and the
mini-round (N1) put them on their own container rather than into CT 111
`productivity`.

The reasoning that decided it, in the order it mattered:

1. **CT 111's disk had to grow either way.** Its rootfs is 8 GB with 2.7 GB
   used, and the three images alone are about 5 GB — stirling-pdf carries
   LibreOffice, paperless carries the OCR toolchain. The "cheaper" option
   was not cheaper.
2. **Blast radius.** A paperless restore, or a postgres major-version
   migration, should not be able to take supersync down with it.
3. **Functional bundling**, Kenny's own rule: these three are personal
   administration. supersync is a sync service. They share a purpose with
   each other and none with it.

vmid 114 and 10.10.10.14 were free because CT 190 and 191 were destroyed on
2026-08-31 (D40). The container follows the prefix + vmid − 100 address
convention like every other stack.

`paperless-db` is declared as its own app so the postgres directory is named
after its owner (O7) and lands in its own restic repository: a document
archive and its index are restored together or not at all.

All three are routed through Traefik (N2). Every `*.kp-soft.dev` hostname is
guarded by Cloudflare Access — verified 2026-08-31, `grafana` and `fin` both
302 to `mendax1.cloudflareaccess.com`, and after deployment `budget`, `pdf`
and `docs` do the same.


## Amendment — 2026-08-31: CT 115 `home`

Kenny asked for one page listing every service. Homepage (v2.1.2) got its own
container rather than joining the gateway (P1).

The gateway was the obvious home — a start page is the front door and belongs
with the reverse proxy, and CT 104 has 4.5 GB of memory free. Kenny chose the
separate container, and the reason holds: CT 104 is unmanaged until M8, so
anything placed there gets no backup, no log caps and no monitor. Three
separate failures on the night of 2026-08-31 came from exactly that gap.

Homepage was the right tool for a specific reason. Most start pages keep
their tiles in a database edited through a web interface, so the layout of
the house lives somewhere that is not this repository and does not survive a
rebuild. Homepage reads flat files: the page is in git, in the backup, and a
change to it reads as a diff.

Nine widgets carry live data (Jellyfin, Seerr, Sonarr, Radarr, Prowlarr,
Bazarr, Paperless, Proxmox, Grafana), all verified answering from inside CT
115 before the page was declared working. qBittorrent is a plain link: its
WebUI password is hashed and unreadable and its API key answered 403 to every
header shape tried, so a widget needs either the password or a LAN auth
whitelist — weakening a running service's authentication is Kenny's call.
