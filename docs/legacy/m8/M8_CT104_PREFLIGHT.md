# M8 · CT 104 (gateway) — pre-flight

Read-only sweep of the last container in M8, taken 2026-09-01 while it was
running normally. Every number here is what the acceptance test compares
against afterwards. Nothing was changed to produce it.

## Why this one is different

The other three containers could fail and cost their own service. This one
carries everything else's front door. It runs the Cloudflare tunnel, so while
it is down **nothing at all is reachable from outside the house** — not the
*arr suite, not Jellyfin, not Home Assistant's remote access. It also runs
Loki, so logs shipped by every other container have nowhere to go for the
duration, and Uptime-Kuma, so the thing that would normally tell us something
is down is itself down.

There is no partial rollout: the tunnel is one process and the routes are one
file tree.

## What is on it

| | |
|---|---|
| hostname | `lxc-platform-stack` (v1 name — A2 refuses it, so a rebuild is the only route) |
| resources | 4 cores, 5120 MB, 1024 MB swap, 30 G rootfs (5.4 G used) |
| network | 10.10.10.4/24, vlan 10, `onboot: 1`, `startup: order=5` |
| privilege | unprivileged |
| mounts | **none** — no bind mounts at all, everything lives on the rootfs |

Nine containers: traefik, cloudflared, crowdsec, grafana, loki, uptime-kuma,
goaccess, promtail, cadvisor.

## Configuration directories

| directory | size | files | what it really is |
|---|---|---|---|
| `/opt/traefik-config` | 142 M | 132 | routes + plugins + **access logs** |
| `/opt/loki-config` | 156 M | 3775 | Loki's **log database**, not config |
| `/opt/grafana-config` | 114 M | 573 | grafana.db + provisioning |
| `/opt/uptime-kuma-config` | 15 M | 3 | kuma.db (+ `-wal`, `-shm`) |
| `/opt/crowdsec-config` | 10 M | 132 | the bouncer registry and hub |
| `/opt/goaccess-config` | 1.9 M | 1 | its report database |
| `/opt/cloudflared-config` | 4 K | **0** | empty — the tunnel is a token, not a file |

Three of these are live SQLite databases with write-ahead logs on disk
(`kuma.db-wal` is present right now). They must be copied with the apps
stopped, exactly as the media stack was, or the copy is a torn database that
looks fine until it is opened.

## The 26 hostnames

Twenty-two come from `/opt/traefik-config/routes/`, four from docker labels on
this container's own compose files (`go`, `grafana`, `traefik`, `uptime`).
Every one of them answered on 2026-09-01 — 25 with `302` (Cloudflare Access
redirect) and `sp` with `200`:

```
alerts almanac baz budget docs fin go grafana ha home kuma kyu opn pdf
prom prowl prox qbit rad seerr son sp sync traefik trmnl uptime
```
all under `.kp-soft.dev`.

Traefik listens on **:80 only** — there is no `:443` and no API port. TLS
terminates at Cloudflare and the tunnel reaches traefik over plain HTTP inside
the container. That is why every route says `entryPoints: [web]`.

## Application state to preserve

| | recorded |
|---|---|
| Grafana dashboards | 19 |
| Grafana datasources | 2 |
| Uptime-Kuma monitors | 36 |
| CrowdSec bouncers | 3 registrations, all `traefik-bouncer` |
| CrowdSec active decisions | 0 |
| Loki labels | `container_name, filename, host, job, service_name, stack, stream` |

The three CrowdSec bouncer rows are the same bouncer re-registering as traefik
got a new container IP (172.18.0.2, .3, .7). Harmless, and worth not
reproducing.

## Secrets that must travel

| variable | app | consequence if lost |
|---|---|---|
| `TUNNEL_TOKEN` | cloudflared | **total loss of external access** — this is the tunnel |
| `CROWDSEC_BOUNCER_API_KEY` | traefik | traefik cannot ask CrowdSec; the plugin fails closed |
| `GF_ADMIN_USER` / `GF_ADMIN_PASSWORD` | grafana | no admin login |

`crowdsec/.env` exists but every line in it is commented out.

## Backups

Three vzdumps of CT 104, the newest from **2026-09-01 02:30 (2.4 G)**. The
gateway is **not** in `state.json`, so the orchestrator takes no restic
backups of it — the vzdump is currently its only net, and a fresh one has to
be taken immediately before the destroy.

## What this pre-flight does NOT settle

- Whether Loki's 156 MB of log history is worth carrying. It is a database of
  logs, not configuration; losing it loses history, not function.
- Whether traefik's access logs (part of that 142 MB) should move to the
  container's own disk instead of a directory that gets backed up nightly.
- Whether the three duplicate CrowdSec bouncer registrations should be
  cleaned up before or after.

Those are decisions, and they go to Kenny in a form.
