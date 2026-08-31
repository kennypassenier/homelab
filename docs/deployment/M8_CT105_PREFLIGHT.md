# CT 105 · downloader — the pre-flight

Kenny's condition for this container and for media (D61): the rebuild must
give the same result byte for byte, and his running downloads must survive.
He chose (form Q1, 2026-08-31) to have this round done first and to decide
about the replacement afterwards. Everything below is read-only; nothing was
stopped, changed or moved.

`MIGRATION_INVENTORY.md` was compiled on 11 August and its own rule says a
mount it does not list makes the plan stale. Re-derived today from
`docker inspect` and `pct config`:

## Mounts — the 11 August table still holds, with nothing new

| App | Mount | Verdict | Checked |
|---|---|---|---|
| qbittorrent | `/opt/downloader-config/qbittorrent` → `/config` | **MIGRATE** | 14 610 185 bytes |
| qbittorrent | `/mnt/data/18TB/downloads` → `/downloads` | **RECREATE the mount** | data never moves |
| gluetun | *(none)* | env-only | the `.env` IS the config |
| cadvisor · promtail | agent mounts + positions | **RECREATE** | scaffolded fresh |

No mount exists that the old table does not list. The plan is not stale.

## Three findings that change how this must be built

**1 · gluetun and qBittorrent cannot be separate apps.** qBittorrent has no
network stack of its own: it runs inside gluetun's namespace
(`network_mode: service:gluetun`), which is the kill switch — if the VPN
container stops, qBittorrent loses all connectivity rather than falling back
to the naked line. Docker only accepts `service:` between services in the
**same compose project**, so the two must share one app directory. That is
allowed and already precedented: `paperwork/paperless-db` defines two
services in one file. Splitting them "for tidiness" would silently turn the
kill switch off — the container would still run, and it would download
without the VPN.

**2 · The container must stay privileged, and that deletes the riskiest
step.** The downloads tree is owned by host uid 1000 with mode 777. CT 105 is
privileged today, so in-container uid 1000 IS host uid 1000 and qBittorrent
(PUID=1000) writes there directly. The H3 addendum's `chown -R 101000:101000`
exists only for the case where the rebuild makes the container
**un**privileged — and there is no reason to. Cloning the privileged template
(997) keeps the ownership question from arising at all, which is both the
safer path and the one that matches Kenny's "nothing must change".

**3 · Only the 18 TB mount is actually used.** Every save path in
qBittorrent's config is under `/downloads` — `completed`, `incomplete`, and
both categories (`radarr`, `sonarr`) on their default path. The container
never touches the 12 TB dataset, although the LXC mounts it. It is carried
across anyway: a data mount costs nothing, and dropping one that a torrent
might one day be pointed at is a change with no upside.

## The numbers, recorded before anything stops

| Measurement | Value |
|---|---|
| Torrents in the session | **80** `.fastresume` files (76 `.torrent`) |
| qBittorrent config | 14 610 185 bytes |
| Downloads tree | 590 664 118 070 bytes (≈ 590 GB) |
| Files in `incomplete/` | 355 |
| Files in `completed/` | 0 |
| Tree ownership | uid 1000, mode 777 |
| Container | privileged, 2 cores, 2048 MB, 512 MB swap, 10 G rootfs, onboot, order 99 |
| WebUI | port 8080, published by gluetun |
| VPN | Surfshark WireGuard via `ch-zur.prod.surfshark.com`, address 10.14.0.2/32 |

**The VPN key is already safe:** `downloader/gluetun/.env` in the vault holds
`GLUETUN_WIREGUARD_PRIVATE_KEY`, verified byte-identical to the value the
running container uses. Losing that file would leave the downloader without a
tunnel, so it was checked before anything else.

## The one measurement that could not be taken

qBittorrent's API refuses without credentials (403 from inside the container),
so the **per-torrent states and progress** are not recorded here — only the
count. Two ways to close that gap, and it is Kenny's call:
either he gives the WebUI login so the list can be captured and compared
automatically, or he opens the UI himself before and after and confirms the
torrents are all there and none restarted from zero.

Until one of those happens, the acceptance test for his first guarantee —
"die downloads moeten intact blijven" — rests on the file count and the
session directory being carried across whole, which is weaker than the
comparison the other guarantees get.
