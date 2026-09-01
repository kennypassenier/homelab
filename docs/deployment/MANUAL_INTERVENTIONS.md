# Manual interventions, and why each one should stop being possible

Kenny, 2026-09-01, on the correction form: the rule "after a manual repair,
deploy immediately" is right *for the development process* — but **once he is
using the software, there should be no manual interventions at all, and none
should be necessary.**

That is a sharper goal than the rule it amends, and it changes what the
measure is for. "Deploy after a manual repair" treats manual repair as normal
and merely makes it stick. What he asked for is that the count goes to zero.

So this file counts them. Every manual intervention on a managed container is
listed with what would have to exist for it to have been unnecessary. The
number at the top is the metric; it is meant to fall.

## Open: 3

| # | What was done by hand | What would make it unnecessary | State |
|---|---|---|---|
| 1 | Killed a hung `docker compose pull` and re-ran it | The F129 fallback does this now, but it has never fired in the field — the images were already local by the time it shipped | **guarded, unproven** |
| 9 | Remove the `go.kp-soft.dev` ingress and its Access policy from the Cloudflare dashboard | Nothing this orchestrator can do: the tunnel's ingress rules and every Access policy exist only in Cloudflare's own configuration (F12), and no credential for it lives here. Kenny has to click it. Until he does, the hostname is a public entry pointing at a route that no longer exists | **awaiting Kenny** |
| 8 | Read the running process uid by hand on three containers, to work out why the ownership check was wrong | Nothing: this was diagnosis rather than repair. Listed anyway, because the reason it was needed is that a check gave a confident wrong answer, and that is the thing that should not have happened | **diagnosis, not repair** |

## Closed: 6

| # | What was done by hand | What makes it unnecessary now |
|---|---|---|
| 2 | `chown` on Loki's data directory | The declared owner was corrected, and the ownership check (F137) refuses a deploy where the app cannot write to its own data. The hand-repair itself was undone by the next deploy, which is the whole point |
| 3 | Re-registered the CrowdSec bouncer after deleting the shared key | Nothing yet in code — but the fault is recorded (F140) and the tidy-up that caused it is done and will not recur: there is one row now, not three |
| 4 | Removed the stale `manual-observability.yml` route | The route file for a moved service now travels with its stack (`stacks/uptime/traefik-routes.yml`), so the stale one had a successor rather than a gap |
| 5 | Moved Traefik's access logs to a borrowed path | Declared as a `data_mount` in the stack file; the move is now what a deploy does |
| 6 | Renamed `/opt/uptime-kuma` so nothing could start it | The gateway rebuild removed it entirely; the directory no longer exists |
| 7 | Set `gateway_routes_dir` in `host.toml` | Host configuration rather than a repair — but it is the kind of value that should be derived from the gateway stack's own mount instead of typed. Queued as part of H6 |

## The honest gap

Numbers 1 and 3 are guarded by code that has never had to act, and 7 is a
typed value that could be derived. None of them is a *repair* any more, but
none is proven either.

The measurement Kenny asked for is therefore not "did I deploy after a manual
repair" but **"how many entries are in the open table, and is that number
falling"** — checked at the next milestone, and again in the retrospective.

**First reading, 2026-09-01:** two open. Every one of the thirteen stacks has
since been deployed cleanly through the full check set, which is the closest
thing to proof the closed entries have: whatever was repaired by hand survived
a deploy that would have overwritten it.
