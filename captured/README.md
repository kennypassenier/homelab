# captured/ — live configuration that is not yet a deployable stack

Configuration read off the running machines and committed here so a deploy
cannot silently revert it, but **not** placed under `stacks/` because there is
no stack to deploy it with yet.

The distinction matters. A directory under `stacks/` with an
`lxc-compose.yml` is something the orchestrator will act on, and three such
files were deleted on 2026-08-30 precisely because they claimed vmids that
live containers were using. Nothing here has a manifest and nothing here is
deployable; these are records.

Each subdirectory moves into `stacks/` when that stack is actually built, in
milestone M8 of `docs/deployment/REALIZATION_PLAN.md`.

| Directory | What | Moves to |
|---|---|---|
| `gateway/` | CT 104's Grafana provisioning: two datasources and seven dashboards | `stacks/gateway/` |
| `gateway/cloudflare-*.json` | R5 (2026-09-20): the Cloudflare tunnel's ingress rules and the three Access applications with their four policies, read through the read-only API token; e-mail addresses redacted. What the edge does before Traefik sees a request — captured so a lost Cloudflare account can be rebuilt from a record instead of memory | stays a record: Cloudflare is configured through its own API, never by a deploy |
| `fleet/` | The cadvisor compose file that runs identically on every docker host | baked into the golden template (O2) |
| `pve-host/` | The SMART textfile collector and its systemd timer, which run on the Proxmox host itself — never inside a container, because SMART is unreadable from an unprivileged LXC | stays here; the host is not a stack |
| `pve-host/firewall/` | The Proxmox firewall files live on pve since 2026-09-20 (F183): the datacenter switch, the host's own firewall kept OFF, and CT 116's per-veth ruleset that closes neighbour traffic OPNsense cannot see | stays here; applied by hand with `pct set … firewall=1`, rollback is the same flag at 0 |

## Why the SMART collector is a script and not an exporter

`smartctl_exporter` has no Debian package, and installing an unmanaged binary
on the hypervisor was the worse trade. The collector writes a `.prom` file
that node_exporter picks up. Its one non-obvious detail is in the code:
`smartctl --scan` labels the SATA disks behind this controller as `scsi`,
which makes `smartctl` exit 4 with an empty attribute table — `-d auto`
negotiates the translation, and the collector tries that first.
