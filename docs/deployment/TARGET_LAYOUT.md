# Target layout — which service runs where

Phase 4 draft. **Not approved, and not yet attacked by the critic.**

Kenny's grouping rules, from the Phase 0 brief: bundle by function so related
services sit together (arr-suite with Jellyfin, gluetun with qBittorrent, kyu
with kyu-runner), and separate what must not take each other down. Plus the
priority rule: anything behind the edge needs cloudflared and Traefik healthy
first.

## The proposal

| vmid | stack | services | why together |
|---|---|---|---|
| 104 | **edge** | traefik, cloudflared, crowdsec, goaccess | one failure domain: the way in. goaccess reads Traefik's access log off the same disk, crowdsec parses the same file |
| 105 | **downloader** | gluetun, qbittorrent | qBittorrent uses `network_mode: service:gluetun` — they cannot be separated, that pair IS the kill switch |
| 106 | **media** | jellyfin, sonarr, radarr, bazarr, prowlarr, seerr, flaresolverr, recyclarr | Kenny's explicit wish: the arr-suite and Jellyfin interact constantly. recyclarr configures sonarr/radarr, so it belongs beside them |
| 108 | **syncthing** | syncthing | already its own container; the Obsidian vault peer |
| 109 | **messaging** | kyu, kyu-runner, http-switchboard | everything that moves a message from one shape or place to another. All three are Rust binaries under systemd |
| 111 | **productivity** | vikunja, supersync, postgres | Kenny's own task and sync data; postgres exists only for supersync |
| 112 | **almanac** | almanac | self-updating, self-reverting; deliberately left alone |
| 113 | **observability** | prometheus, alertmanager, pve-exporter, grafana, loki, uptime-kuma | everything that measures, in one place. Grafana, Loki and Uptime Kuma move off the edge |
| — | removed | 107 (empty), 190 and 191 (scratch) | |

Every container additionally carries node_exporter, cadvisor and promtail,
baked into the golden template (O2) rather than installed per stack.

## The two moves this proposes, and why

**1 · The edge stops hosting observability.** CT 104 currently runs the way
in (traefik, cloudflared, crowdsec) *and* the way you find out something is
wrong (grafana, loki, uptime-kuma). Those are different failure domains
sharing 5 GB of RAM and one disk. A Loki that fills its volume, or a Grafana
that leaks, takes the household's entire internet-facing surface with it.
Moving them to 113 leaves the edge small, boring, and rarely deployed.

Cost, stated plainly: every promtail in the fleet points at
`http://10.10.10.4:3100` and must be repointed to 113. That is exactly the
kind of coupling O3 exists to make automatic, so it is work this project
wants to do anyway rather than extra work.

**2 · The messaging services share a container.** kyu already runs on CT 109
as a native binary. kyu-runner and http-switchboard both publish static
binaries with checksums, so all three can run as systemd units side by side —
no docker, no mixed stack. They form one story: a message arrives, is
translated, is delivered.

## Open problems this draft does not solve

- **Uptime Kuma's independence.** Its value is watching from outside what it
  watches. On 113 it sits beside Prometheus and Alertmanager, so a single
  container failure blinds every automated watcher at once. The remaining
  external observer would be Home Assistant plus the alerting chain — which
  itself runs partly on 109 and 113. No option here is clean: on the edge it
  watches its own host, on its own container it costs a container, on 113 it
  shares a fate.
- **A native stack holds one service** (`StackState.native` is a single
  option), so CT 109 with three services needs T5 settled first.
- **CT 109 is sized for one small binary**: 256 MB RAM, 1 core, 2 GB disk.
  Three services need more, and growing an LXC's disk is not free.
- **The vmid-to-last-octet convention** survives this layout without a new
  container, which is convenient but accidental — nothing enforces it.
- **CT 108's purpose.** It is named `synctest` and was the pilot's test
  container, but it runs the real syncthing peer. Either it is a test stack
  that should be recreated as `110-app-syncthing` per the existing preset, or
  it is production and should be renamed. It cannot honestly stay both.
