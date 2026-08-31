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

## Acceptance, after the rebuild

Each line below is the same measurement taken again. A number that moved is a
failure, not a curiosity.

- [ ] `vikunja.db` is 557 056 bytes and the directory 6 602 968.
- [ ] postgres reports the same seven tables with the same row counts.
- [ ] Both hostnames answer 200 through Traefik, from the same address.
- [ ] Kenny's Super Productivity desktop and phone still sync — the passkey
      still works, which is the one thing a changed `WEBAUTHN_RP_ID` would
      silently break (the value was verified byte-identical beforehand).
- [ ] Vikunja does not ask him to log in again — the session secret is
      carried across, not regenerated.
- [ ] A first restic backup of both new repositories succeeds and is not
      empty.

## Rollback

The old container is not destroyed until the list above is ticked. Until
then, rolling back is `docker compose up -d` in it. After it, the vzdump
taken in the same window is the net.
