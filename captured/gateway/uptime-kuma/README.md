# Uptime Kuma — the monitor set

Until 2026-08-31 this instance had exactly one monitor: the kyu healthcheck
Kenny added by hand. Everything else in the house was unwatched, and nothing
said so — an empty monitoring tool looks identical to a healthy one.

`seed-monitors.py` creates the set: an HTTP check per service, a ping per
container, and two external hostnames that prove the Cloudflare tunnel and
Traefik are passing traffic. It is idempotent — a monitor whose name already
exists is left untouched, including any edits Kenny has made to it — so it
can be re-run whenever a service is added.

Every endpoint in the list was probed before being added. A monitor that is
red from birth trains you to ignore the dashboard, which is worse than not
having it.

## Running it

Kuma has no API token: it authenticates with the same login as the web
interface, which lives in latch as `platform/uptime-kuma/.env`.

```sh
cd stacks && latch pull --env prod
PW=$(grep '^KUMA_PASSWORD=' platform/uptime-kuma/.env | cut -d= -f2-)
rm -f platform/uptime-kuma/.env          # latch keeps it; disk should not
python seed-monitors.py "$PW"
```

Needs `uptime-kuma-api` (pip). Kuma speaks socket.io, not REST, so plain
curl is not an option.

## Where this is going

This is a script because CT 104 is not managed by the orchestrator yet. The
orchestrator already renders a Prometheus target file (T1) and a Grafana
dashboard (T2) per stack at deploy time; a Kuma monitor set is the same shape
and belongs there too, so that adding a stack cannot leave it unwatched.
