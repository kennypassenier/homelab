# CT 111 · the numbers before the rebuild

Recorded 2026-08-31 with everything running, because a count taken after a
migration proves nothing about what the migration did (D61).

## What runs today

| | |
|---|---|
| Container | `lxc-productivity-stack`, vmid 111, 10.10.10.11, unprivileged |
| Resources | 2 cores, 2048 MB RAM, 512 MB swap, 8 G rootfs, onboot, startup order 99 |
| Apps | vikunja, supersync, supersync-postgres, cadvisor, promtail |
| Reachable | `tasks.kp-soft.dev` → :3456 · `sp.kp-soft.dev` → :1900, both HTTP 200 |

## The state that must survive

| Thing | Measurement | Where it lives now |
|---|---|---|
| Vikunja database | `vikunja.db`, 557 056 bytes | `/opt/vikunja-config` (6 602 968 bytes total) |
| SuperSync database | 8543 kB reported by postgres | `/opt/supersync-config/postgres` (66 080 651 bytes) |
| SuperSync rows | operations 70 · users 1 · passkeys 1 · sync_devices 2 · user_sync_state 1 · _prisma_migrations 28 | in that database |
| SuperSync `/app/data` | **empty**, root-owned, process runs as uid 1001 | not carried across — see the stack file |

## Acceptance — done 2026-08-31, 20:35

- [x] postgres reports the same seven tables with the same row counts:
      `_prisma_migrations` 28 · `operations` 70 · `passkeys` 1 ·
      `pending_passkey_registrations` 0 · `sync_devices` 2 ·
      `user_sync_state` 1 · `users` 1. Database size 8543 kB, unchanged.
- [x] The copy was verified before the old container was destroyed: 1323
      files, identical names and sizes on both sides. The first comparison
      *failed* and that was the check working — the two `find | sort` runs
      disagreed on where `PG_VERSION` belongs, a locale collation difference,
      so the comparison was redone under `LC_ALL=C` and matched exactly.
- [x] `sp.kp-soft.dev` answers 200 through Traefik, at the same address, and
      SuperSync answers 200 directly on 10.10.10.11:1900.
- [x] Same vmid, same IP, same hostname convention, `protection` on, `onboot`
      on, boot order applied, the database bind-mounted from the host.
- [x] First restic backup of `supersync-db-config`: 1323 files,
      66 063 275 bytes, snapshot `2a636b3c`. The same file count and byte
      total as the verified copy, so the backup covers what the copy carried.
- [x] **Confirmed by Kenny, 2026-08-31:** "supersync werkt nog altijd op
      beide apparaten". The passkey survived, which is the half that could
      not be proven from here.

## Everything else, compared line by line

Kenny asked whether the rebuild was identical on every OTHER level. Read out
of the vzdump's own `pct.conf` rather than from memory:

| | old | new |
|---|---|---|
| arch · ostype | amd64 · debian | same |
| cores · memory · swap | 2 · 2048 · 512 | same |
| rootfs | 8 G on local-lvm | same |
| features | `nesting=1,keyctl=1` | same |
| unprivileged | 1 | same |
| onboot | 1 | same |
| vmid · IP | 111 · 10.10.10.11 | same |
| **startup order** | 99 | **70** |
| **hostname** | lxc-productivity-stack | **111-app-productivity** |
| **tags** | none | **homelab** |

So: **not identical, and every difference is deliberate.** Five of them, none
accidental — the boot order moved into the fleet's scheme, the hostname now
follows the convention the safety guard checks, the tag marks it managed,
Vikunja is gone by Kenny's decision, and the postgres container is called
`supersync-db` instead of `supersync-postgres` so its directory, its restic
repository and its container all carry one name (O7/D25). The empty
`/app/data` mount was dropped for the reason given in the stack file.

~~Vikunja~~ — **not rebuilt** (Kenny, at the go: it is not used any more).
Its 557 KB database was not migrated and went with the old container; the
vzdump below holds it. `tasks.kp-soft.dev` now answers 404 rather than
routing anywhere, and its tile is gone from the front page.

**Measured outage: 217 s** (20:31:46 → 20:35:23), against 653 s for the home
drill. The difference is entirely the image pull: nothing stalled this time.

**Vzdump:** `vzdump-lxc-111-2026_08_31-20_32_29.tar.zst`, 1 170 826 632 bytes
on `hdd4tb-backup`. The whole old container, Vikunja included.

## Rollback

The old container is not destroyed until the list above is ticked. Until
then, rolling back is `docker compose up -d` in it. After it, the vzdump
taken in the same window is the net.
