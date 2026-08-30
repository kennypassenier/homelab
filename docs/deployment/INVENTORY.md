# Inventory — the homelab as it actually runs

Phase 1 output of the deployment project. Swept live from the Proxmox host
on **2026-08-30**; every line below was read off a running machine, not
copied from a repository or a memory. Where something could not be reached,
it says so rather than guessing.

Raw sweep material is not kept: re-run the commands in §9 to refresh.

## 1 · Guests

| VMID | Name | Kind | IP | RAM | Disk | Role | Managed by |
|---|---|---|---|---|---|---|---|
| 100 | 100-infra-opnsense | VM | 10.10.5.1 | 4 G | 32 G | router / firewall | **untouchable** |
| 101 | vm-home-assistant | VM | 10.10.10.2 | 6 G | 32 G | Home Assistant + add-ons | **untouchable lifecycle** (config by API, per-change consent) |
| 102 | 102-infra-omadacontroller | LXC | — | — | — | network controller | **untouchable** |
| 103 | 103-infra-fileserver | LXC | 10.10.10.3 | — | — | fileserver, owns the ZFS subvols | **untouchable** |
| 104 | lxc-platform-stack | LXC | 10.10.10.4 | 5 G | 30 G | edge + observability | ansible-era, hand-tended |
| 105 | lxc-downloader-stack | LXC | 10.10.10.5 | 2 G | 10 G | gluetun + qBittorrent | ansible-era, hand-tended |
| 106 | lxc-media-stack | LXC | 10.10.10.6 | 8 G | 80 G | Jellyfin + arr-suite | ansible-era, hand-tended |
| 107 | lxc-mqtt-stack | LXC | 10.10.10.7 | 1 G | 8 G | **empty — nothing runs** | to be cleaned up |
| 108 | 108-app-synctest | LXC | 10.10.10.8 | 1 G | 4 G | syncthing (test stack) | **homelab** |
| 109 | 109-app-kyu | LXC | 10.10.10.9 | 256 M | 2 G | kyu message hub (native) | **homelab (state is stale — F7)** |
| 111 | lxc-productivity-stack | LXC | 10.10.10.11 | 2 G | 8 G | Vikunja + SuperSync | ansible-era; off no-touch since 2026-08-29 but never adopted |
| 112 | 112-app-almanac | LXC | 10.10.10.12 | 512 M | 4 G | almanac calendar gateway (native) | **homelab** |
| 113 | 113-app-metrics | LXC | 10.10.10.13 | 1 G | 16 G | Prometheus + Alertmanager | **homelab** |
| 190 | 190-scratch-mailbox | LXC | 10.10.10.14 | 256 M | 2 G | scratch, running | to be cleaned up |
| 191 | 191-scratch-kyu-runner | LXC | 10.10.10.15 | 512 M | 2 G | scratch, shared with pipeline-v2 | to be cleaned up after coordination |
| 999 | debian-12-homelab-v1 | LXC | — | — | — | golden template, stopped | replaced by G5 |
| 9000 | ubuntu-2404-tmpl | VM | — | — | — | VM template, stopped | out of scope |

Address convention: last octet = vmid − 100. CT 190/191 break it by holding
.14 and .15, which a future CT 114/115 would claim.

## 2 · Services per guest

**CT 104 · platform (8 containers).** traefik, cloudflared, crowdsec,
goaccess, grafana, loki, uptime-kuma, cadvisor. Ports 80, 1883, 3000, 3001,
3100, 7880, 8081, 8090. No promtail — this host's own container logs reach
Loki through nothing.

**CT 105 · downloader (4).** gluetun (Surfshark WireGuard, `ch-zur`),
qbittorrent through `network_mode: service:gluetun`, promtail, cadvisor.
Ports 8080 (qBittorrent WebUI, published by gluetun), 8081.

**CT 106 · media (9).** jellyfin, sonarr, radarr, bazarr, prowlarr, seerr,
flaresolverr, promtail, cadvisor. Ports 8096, 8989, 7878, 6767, 9696, 5055,
8191, 8081.

**CT 108 · synctest (3).** syncthing, promtail, cadvisor. Ports 8384, 22000,
21027, 8081.

**CT 109 · kyu.** Native binary `/usr/local/bin/kyu` under `kyu.service` as
user `kyu`, `EnvironmentFile=/etc/kyu/kyu.env`, store in `/var/lib/kyu`.
Port 8080. Carries its own `kyu-backup` script writing `kyu.backup-*.db`
beside the live store.

**CT 111 · productivity (5).** vikunja (SQLite), supersync + postgres:16,
promtail, cadvisor. Ports 3456, 1900, 8081.

**CT 112 · almanac.** Native binary under `almanac.service`. Port 8080,
serves `/metrics` and `/healthz`.

**CT 113 · metrics (5).** prometheus (90 d retention), alertmanager,
pve-exporter, promtail, cadvisor. Ports 9090, 9093, 9221, 8081.

**PVE host.** `homelab-host.service` on :8443, `prometheus-node-exporter`,
and the SMART textfile collector (`smart-collector.timer`, every 15 min).
Cron: `vzdump`, `e2scrub_all`, `zfsutils-linux` — all distribution defaults;
the hand-written ZFS replication cron is gone, absorbed into the daemon.

## 3 · Two container layouts, both live

- **Ansible era** (104, 105, 106, 111): `/opt/<app>/docker-compose.yml` with
  config at `/opt/<app>-config/` — **inside the container**. Destroying the
  container destroys the configuration.
- **Homelab era** (108, 113): `/opt/<stack>/<app>/docker-compose.yml` with
  config bind-mounted from `/appdata/<stack>/<app>-config` **on the host**,
  which is what restic backs up and what survives a rebuild.

`/appdata` on the host today holds only `metrics/` and `synctest/`. Every
ansible-era stack's configuration lives nowhere else but inside its own
container. This is the single biggest structural gap in the fleet.

Layouts already mix: 105, 106 and 111 carry a homelab-style
`<stack>/promtail/` directory beside their ansible-style app directories.

## 4 · How the services are wired to each other

**Edge.** Cloudflare tunnel → cloudflared (CT 104) → traefik `:80` on
`platform_net` → per-service upstream. Traefik takes routes from two
providers: docker labels for containers on CT 104 itself, and files in
`/opt/traefik-config/routes/` for everything on another guest.

**Route files** (11, of which one is a `.bak` that Traefik's extension filter
ignores):

| File | Hostnames → upstream |
|---|---|
| `lxc-media-stack.yml` | fin/son/rad/baz/prowl/seerr.kp-soft.dev → 10.10.10.6 |
| `lxc-downloader-stack.yml` | qbit.kp-soft.dev → 10.10.10.5:8080 |
| `lxc-productivity-stack.yml` | tasks.kp-soft.dev → 10.10.10.11:3456 |
| `111-app-supersync.yml` | sp.kp-soft.dev → 10.10.10.11:1900 (needs a Cloudflare Access **bypass**: Bearer token + WebSocket cannot pass interactive login) |
| `108-app-synctest.yml` | sync.kp-soft.dev → 10.10.10.10:8384 |
| `112-app-almanac.yml` | almanac.kp-soft.dev → 10.10.10.12:8080, with `/metrics` deliberately rewritten to a 404 |
| `lxc-mqtt-stack.yml` | TCP `HostSNI(*)` on :1883 → 10.10.10.7:1883 — **points at the empty container** |
| `manual-homeassistant.yml` | ha.kp-soft.dev → 10.10.10.2:8123 |
| `manual-terminus.yml` | trmnl.kp-soft.dev → 10.10.10.2:2300 (Kobo dashboard; needs a service-token Access policy) |
| `manual-routes.yml` | opn.kp-soft.dev → **https**://10.10.5.1, prox.kp-soft.dev → https://10.10.5.250:8006, both `insecureSkipVerify` |

Traefik's own dashboard is routed at traefik.kp-soft.dev; grafana, goaccess
and uptime-kuma route by docker label.

**Observability.** promtail on 105/106/108/111/113 → Loki at
`http://10.10.10.4:3100`. Prometheus on CT 113 scrapes node_exporter on
eleven hosts (`:9100`), cadvisor on six (`:8081`), pve-exporter (`:9221`) and
almanac (`:8080`). Alertmanager holds four rules and delivers to a `none`
receiver. Grafana on CT 104 has both datasources and six provisioned
dashboards.

**Alerting and notification.** `homelab-host` posts operation results to
`http://10.10.10.2:8123/api/webhook/homelab-ops-c4d81f26`. Alertmanager has
no delivery leg — that is what HTTPSwitchboard is for.

**Kenny's own software.** kyu (CT 109) is the hub; almanac (CT 112) posts
into it; kyu-runner and newsflash consume from it; HTTPSwitchboard will
translate Alertmanager into the HA webhook shape. All addressing is by LXC
IP already, which matches scope constraint C3.

## 5 · Configuration that took real work and must not be lost

| Where | What | Why it is fragile |
|---|---|---|
| CT 106 jellyfin | `/dev/dri/renderD128` passthrough, `group_add: 993,44,104,105,106,107`, `user: 1000:1000`, `shm_size: 4gb`, `JELLYFIN_TRANSCODE_DIR=/dev/shm` | hardware transcoding stops silently if any of it is dropped |
| CT 105 gluetun | Surfshark WireGuard, `SERVER_HOSTNAMES=ch-zur.prod.surfshark.com`, `HEALTH_TARGET_ADDRESSES=1.1.1.1:443,8.8.8.8:443` | the health-target override exists to break a DNS startup race; the default marks a working VPN unhealthy |
| CT 105 qbittorrent | `network_mode: service:gluetun` + `depends_on: service_healthy` | this pair IS the kill switch |
| CT 105 qbittorrent | `Session\ExcludedFileNames` blocking `*.exe *.bat *.cmd *.scr *.lnk` in `qBittorrent.conf` | set by hand 2026-08-29; nothing else in the chain stops executables |
| CT 105 qbittorrent | `DOCKER_MODS=…vuetorrent-lsio-mod` + the manual "use alternative Web UI" setting | the mod installs the files; the setting that activates them is in the app's own config |
| CT 104 traefik | local CrowdSec plugin (`--experimental.localPlugins`), bouncer on the **entrypoint** not per route, `crowdseclapihost=crowdsec:8080` without a scheme | a remote plugin download failure once disabled every plugin; per-route attachment does not survive route re-rendering |
| CT 104 goaccess | the JSON `--log-format` string matching Traefik's access-log format | the default COMBINED format silently parsed zero lines for weeks |
| CT 104 traefik | `manual-routes.yml` uses **https** to OPNsense with `insecureSkipVerify` | http produced an endless 301 redirect loop |
| CT 111 supersync | postgres tuned down (`shared_buffers=128MB`, `mem_limit: 512m`) for a 2 G LXC | upstream defaults hand victim selection to the host OOM killer |
| CT 111 supersync | binds `1900` on all interfaces, not upstream's 127.0.0.1 | Traefik reaches it from another LXC |
| CT 104 grafana | six provisioned dashboards + two datasources under `/opt/grafana/provisioning/` | provisioning files, not database rows — but they exist in no repository |
| PVE host | `smart-textfile-collector.py` using `-d auto` | `smartctl --scan` mislabels these SATA disks as `scsi` and exits 4 with an empty table |

## 6 · Secrets in play (names only, never values)

| Guest | File | Variables |
|---|---|---|
| 104 | `/opt/cloudflared/.env` | `TUNNEL_TOKEN` |
| 104 | `/opt/traefik/.env` | `CROWDSEC_BOUNCER_API_KEY` |
| 104 | `/opt/grafana/.env` | `GF_ADMIN_USER`, `GF_ADMIN_PASSWORD` |
| 105 | `/opt/downloader/.env` | `WIREGUARD_PRIVATE_KEY` |
| 106 | `/opt/jellyfin/.env` | `JELLYFIN_API_KEY` |
| 111 | `/opt/supersync/.env` | `JWT_SECRET`, `POSTGRES_PASSWORD`, `WEBAUTHN_*`, `SMTP_*`, `ALLOWED_EMAILS`, … |
| 111 | `/opt/vikunja/.env` | `VIKUNJA_SERVICE_PUBLICURL`, `VIKUNJA_SERVICE_SECRET` |
| 113 | `/opt/metrics/pve-exporter/.env` | `PVE_USER`, `PVE_TOKEN_NAME`, `PVE_TOKEN_VALUE` |
| 109 | `/etc/kyu/kyu.env` | hub tokens |
| 112 | `/etc/almanac/…` | latch key + Google credentials |

Only CT 113's and CT 108's secrets travel through latch today (D12). Every
ansible-era `.env` is a plaintext file on a container disk with no other copy.

## 7 · Findings

Numbered into `REGISTER.md`; the sharp ones:

- **F7 · kyu has never been backed up and is excluded from the nightly run.**
  Homelab's state still describes the stack as `mailbox`: hostname
  `109-app-mailbox`, unit `mailbox`, binary `/usr/local/bin/mailbox`, data
  dirs `/var/lib/mailbox` + `/etc/mailbox` — none of which exist since the
  rename. `enabled: false`, `last_backup: NEVER`, while almanac, metrics,
  synctest, host-meta and the ZFS jobs all ran at 04:04 this morning. The
  most likely chain is the H8 auto-disable after the first failing nightly
  run following the rename. The hub carries its own `kyu-backup` snapshots
  locally, so nothing is lost — but nothing is off the machine either.
- **F8 · the live no-touch list overrides the code.** `/etc/homelab/host.toml`
  sets `no_touch = [100,101,102,103,104,105,106,107,201,202,203]`, and that
  key *replaces* the compiled default rather than adding to it. Today's
  narrowing of `DEFAULT_NO_TOUCH` therefore changed nothing on the running
  host. The file must be brought in line deliberately, per container.
- **F9 · Uptime Kuma monitors exactly one thing** — `kyu`'s `/healthz`.
  Jellyfin, Traefik, Home Assistant, almanac, the arr-suite and the whole
  edge are unwatched, after four weeks of uptime.
- **F10 · CT 107 is empty but still routed.** `lxc-mqtt-stack.yml` sends TCP
  :1883 to 10.10.10.7, where nothing listens. Meanwhile CT 104 publishes 1883
  itself. Where MQTT actually terminates needs settling before 107 is removed.
- **F11 · ansible-era configuration lives only inside its container** (§3).
- **F12 · the Cloudflare tunnel ingress exists only in the Cloudflare Zero
  Trust dashboard.** `/opt/cloudflared-config/` is empty and the tunnel is
  token-based, so the hostname→service mapping and every Access policy are
  in no repository and cannot be restored from one. Not reachable from here
  without credentials.
- **F13 · CT 104 ships no container logs to Loki** — every other docker host
  runs promtail; the one hosting Loki does not.
- **F14 · kyu's SQLite store is 98 KB of `.db` behind a 4.1 MB `.db-wal`.**
  Any backup or move must take `.db`, `.db-wal` and `.db-shm` together, or
  use the hub's own `kyu-backup`, or it silently captures an old state
  (standing rule 15a).
- **F15 · CT 111 was taken off the live no-touch list on 2026-08-29 so
  homelab could adopt it — the adoption never happened.** It has been
  unprotected and unmanaged at the same time since.

## 8 · Not yet inventoried, deliberately

- Cloudflare Zero Trust: tunnel ingress rules and Access applications
  (needs credentials — F12).
- Home Assistant's own automations, scripts and helpers that this project
  will eventually rework (scope C8; consent per change).
- The `~/Projects/ansible` repo as a reference for what the ansible-era
  stacks were *supposed* to look like; useful when reconstructing intent.
- Per-app internal settings held in each app's own config database (Sonarr's
  indexers, Jellyfin's libraries, Grafana's non-provisioned objects).

## 9 · How to refresh this document

Everything here came from `ssh pve` plus `pct exec <id> -- …`: `pct list`,
`qm list`, `pvesm status`, `zfs list`, per-container `docker ps`,
`systemctl list-units`, `ss -tlnp`, `find /opt -name docker-compose.yml`,
`cat /etc/homelab/host.toml`, and `/var/lib/homelab/state.json`.
