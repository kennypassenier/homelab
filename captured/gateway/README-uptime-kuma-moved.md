# Uptime Kuma left this container on 2026-09-01

It ran here until D68 moved it to its own container (CT 107, stack `uptime`),
because a watchman on the machine he is watching goes down with it — which is
exactly what happened on 2026-08-31.

Its compose file used to sit beside the others in this directory. It is gone
rather than kept, because a captured file for a service that is no longer here
is the kind of record that gets copied back by mistake. The live definition is
`stacks/uptime/uptime-kuma/docker-compose.yml`.

On the gateway itself, `/opt/uptime-kuma` is renamed to
`/opt/uptime-kuma.moved-to-107` so nothing can start it by accident, and
`/opt/uptime-kuma-config` is deliberately left in place as a fallback copy
until the move has proven itself. Both disappear with the gateway rebuild.
