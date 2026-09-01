# What a stack registers, and what it must unregister

T66. Kenny, 2026-09-01: adding a stack must register it with every service
that should know; removing one must clean up everywhere; and **that
completeness has to be expressed in the system rather than remembered.**

This table is the expression. It is the list a change has to be checked
against, and the reason the missing half was found at all — laying the deploy
steps beside the destroy steps made one gap obvious in a minute.

| What | Registered by | Unregistered by | Enforced |
|---|---|---|---|
| Host state record | `deploy` · record state | `destroy` · update state | test `s2_a_failed_deploy_still_records_the_stack` |
| Traefik route | `deploy` · gateway route | `destroy` · remove gateway route | filename equality check (client) |
| Prometheus scrape target | `deploy` · discovery | `destroy` · remove metrics discovery | — |
| Grafana dashboard | `deploy` · grafana dashboard | `destroy` · remove grafana dashboard | test `t66_destroy_removes_the_dashboard_the_deploy_wrote` |
| Log shipping (promtail) | golden template + `_core` preset | dies with the container | — |
| Container metrics (cadvisor) | `guards` · every docker host | dies with the container | tests `h4_cadvisor_*` |
| restic repository | first `backup` run | **deliberately never** — the backup outlives the stack, which is the point | — |
| `/appdata` configuration | `deploy` · host storage | **deliberately never** — C4 is built on the config surviving its container | — |
| **Uptime Kuma monitor** | **nothing** — a one-off script in `captured/gateway/uptime-kuma/` | **nothing** | **gap** |

## The one gap that is still open

A stack added today is watched by nothing. The 36 monitors were created once
by a script and no part of the system knows a stack should have one, so a new
stack is silently unmonitored and a removed one leaves a monitor that fails
forever — which is worse than no monitor, because it teaches you to ignore a
red light.

Closing it means the orchestrator talking to Uptime Kuma's API on deploy and
destroy. That is a real piece of work rather than a repair, and it belongs
with T64 (who measures what, and who notifies through which channel), because
the answer to "should this be an Uptime Kuma monitor at all" may well be no.

## Two entries that say "deliberately never"

They are in the table on purpose. A blank there would read as an oversight,
and both have been questioned before:

- **The restic repository outlives the stack.** That is the whole point of a
  backup — a stack destroyed by accident is exactly when you want it. F106
  came from the opposite assumption.
- **`/appdata` survives the container.** The C4 replacement procedure is built
  on it: same vmid, same address, configuration untouched, container
  disposable. Removing it on destroy would make every rebuild a restore.
