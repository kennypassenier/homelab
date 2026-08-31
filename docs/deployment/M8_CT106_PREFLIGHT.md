# CT 106 · media — the pre-flight

Kenny's condition for this container and the downloader (D61): the rebuild
must give the same result byte for byte — nothing to reconfigure in the
Jellyfin client, the *arr suite still working, every library untouched
inside the programs. This round is read-only; nothing was stopped or moved.

`MIGRATION_INVENTORY.md` was compiled on 11 August and its own rule says a
mount it does not list makes the plan stale. Re-derived 2026-09-01 from
`docker inspect`:

## Mounts — the old table still holds, with one addition worth naming

| App | Mount | Verdict |
|---|---|---|
| jellyfin | `/opt/jellyfin-config` → `/config` | **MIGRATE** — 18.15 GB, see below |
| jellyfin | docker volume → `/cache` | **RECREATE** — transcode cache, regenerates |
| radarr · sonarr · prowlarr · bazarr · seerr | `/opt/<app>-config` → `/config` | **MIGRATE** |
| flaresolverr | docker volume → `/config` | **RECREATE** — 4 KB, empty |
| jellyfin · sonarr · radarr · bazarr | `/mnt/data/{18TB,12TB}` | **RECREATE the mount** — data never moves |
| cadvisor · promtail | agent mounts | **RECREATE** |

Nothing exists that the old table does not list. **bazarr** mounts both data
disks, which the 11 August table did not spell out per app — no new mount, but
worth stating, because unlike the downloader this stack really does use the
12 TB disk.

## Sizes, and the one question they raise

| Directory | Size |
|---|---|
| `/opt/jellyfin-config` | **18 152 086 514** bytes |
| ├ `data/` | 9 303 113 238 — the watch-state database. **Never skip.** |
| ├ `metadata/` | 8 799 362 398 — downloaded artwork and nfo. Regenerable, slowly. |
| ├ `plugins/` | 38 824 220 |
| ├ `config/` | 9 754 818 — the settings themselves |
| └ `log/` + `temp/` | ~1 MB |
| `/opt/radarr-config` | 1 943 325 930 |
| `/opt/sonarr-config` | 557 593 784 |
| `/opt/prowlarr-config` | 73 388 616 |
| `/opt/bazarr-config` | 72 053 992 |
| `/opt/seerr-config` | 7 352 820 |

**Roughly 20.8 GB to copy**, against 14.6 MB for productivity and the
downloader's 14 MB. This is a different size of operation and the copy is the
long pole, not the container rebuild.

**And it raises the question standing rule 28 asks:** `metadata/` is
regenerable — Jellyfin re-downloads it from the scrapers — so 8.8 GB of it
would ride in every restic snapshot forever, and a restore would hand back
artwork that may since have been re-fetched anyway. Excluding it makes the
backup less than half the size; the cost is a long re-scrape after a restore,
during which posters are missing. That is Kenny's call, not mine, and it is
the one design question this pre-flight produced.

## The numbers, recorded before anything stops

| Measurement | Value |
|---|---|
| Jellyfin movies | 896 |
| Jellyfin series | 203 |
| Jellyfin episodes | 5462 |
| Jellyfin collections | 58 |
| Sonarr series | 209 |
| Radarr movies | 955 |
| Prowlarr indexers | 5 |
| Container | privileged, 6 cores, 8192 MB, 80 G rootfs, onboot, order 99 |

## What the rebuild needs that the other stacks did not

- **The GPU.** `gpu: true`, which W1 now checks: it reads the group ids from
  the host instead of assuming them. Measured here: `/dev/dri/card0` is group
  44 and `/dev/dri/renderD128` is **993**, not the 104 the code carried before
  (F110). This container is exactly where that wrong number would have landed.
- **Privileged**, so it clones template 997, not 998 — the same as the
  downloader.
- **Both data disks**, via `data_mounts:` (D59).
- **The in-container paths are frozen**: `/data/18TB`, `/data/12TB`,
  `/config`. Every root folder in the *arr databases and every Jellyfin
  library is stored as one of these, so changing one breaks all of them at
  once.
- **The image cache** (D60) is live and this is the stack it was built for:
  nine images, of which two come from Docker Hub and seven do not.

## The restore drill — done before touching anything (2026-09-01)

Kenny's condition for going ahead: *"We gaan enkel verder als je absoluut
zeker bent dat we bij een fout deze container helemaal kunnen herstellen. Dit
is de belangrijkste container. Ik eis hier perfectie."*

A promise is not a net, so the net was exercised instead of asserted. A fresh
vzdump of CT 106 was taken (24 446 182 348 bytes, 2 m 23 s) and **restored to
a throwaway vmid**, isolated first — data mountpoints removed so it could not
touch the libraries, its network on an unused VLAN and a different address so
it could not touch anything else, `onboot` off.

It booted. Every configuration directory came back at its expected size, and
every database opened:

| Database | Integrity | Contents | Live value |
|---|---|---|---|
| `sonarr.db` | ok | Series = **209** | 209 |
| `radarr.db` | ok | Movies = **955** | 955 |
| `bazarr.db` | ok | shows = **209** | — |
| `jellyfin.db` | ok | BaseItems 62 372 · UserData 15 625 · Users 1 | — |

The two that can be compared against the live system match exactly. The drill
container was destroyed afterwards; the fleet is unchanged.

**So the answer to his question is yes, and this is what it rests on:**

1. The **libraries themselves are never touched.** Films and series live on
   the two ZFS datasets and are only ever mounted in. A container rebuild
   cannot reach them.
2. Until the destroy step, the old container is intact and rollback is
   starting it again.
3. After it, the net is the vzdump above — **proven to restore, boot and
   carry readable databases with the right counts**, on this machine, tonight.
4. Plus the copied configuration on `/appdata` and, once taken, its restic
   snapshot.

What this drill does NOT prove: that Jellyfin's own application layer comes up
clean on a restored copy, because the drill copy deliberately had no network
and no media mounts. That check belongs to the real rebuild, against the
counts recorded above.
