# Migration inventory — CT 104/105/106 → v2 stacks

*Compiled 2026-08-11 from read-only inspection of the live containers
(`pct config`, `docker ps`, `docker inspect`, `du`). Nothing was modified.*

**The completeness rule:** every container and every mount below is classified
`MIGRATE` (copied + verified) or `RECREATE` (regenerated, with the reason).
A migration may only proceed when this table has no unclassified rows, and it
is only *done* when the checksum verification (§ Procedure, step 5) passes.
If a future inspection finds a mount not listed here, the migration plan is
stale — regenerate this document first.

## Where config lives today (this surprised us)

Old layout: configs sit **inside each container** at `/opt/<app>-config` on
the container's own rootfs. The v2 convention moves them to the **Proxmox
host** at `/appdata/<stack>/<app>-config`, bind-mounted in — so containers
become disposable and restic backs up host paths. Migration therefore means
copying data out of the old containers onto the host.

Secrets: `.env` files live next to the compose files (`/opt/<app>/.env`).
In v2 these go to the host vault (`/var/lib/homelab/secrets/`), pushed over
the TLS line — never into git.

## CT 104 — platform stack (→ v2 stack `platform`)

| App | Mount | Size | Verdict |
|---|---|---|---|
| traefik | `/opt/traefik-config` → `/etc/traefik` (incl. `logs/`) | 61M | **MIGRATE** (certs! `acme.json`) |
| crowdsec | `/opt/crowdsec-config` → `/etc/crowdsec` + `data/` | 10M | **MIGRATE** (decisions DB) |
| crowdsec | `/opt/crowdsec/whitelists.yaml` (single file) | — | **MIGRATE** (part of stack files) |
| grafana | `/opt/grafana-config` → `/var/lib/grafana` | 113M | **MIGRATE** (dashboards, users) |
| grafana | `/opt/grafana/provisioning` → `/etc/grafana/provisioning` | — | **MIGRATE** (stack files) |
| loki | `/opt/loki-config/data` → `/loki` | 36M | **MIGRATE** (log history) — or accept loss if we choose a clean log start |
| loki | `/opt/loki/loki-config.yaml` (single file) | — | **MIGRATE** (stack files) |
| uptime-kuma | `/opt/uptime-kuma-config` → `/app/data` | 284K | **MIGRATE** (monitors, history) |
| cloudflared | `/opt/cloudflared-config` → `/etc/cloudflared` | 4K | **MIGRATE** (tunnel creds) |
| goaccess | `/opt/goaccess-config` → `/srv/report` | 1.9M | **MIGRATE** (small; cheap to keep) |
| goaccess | named volume → `/var/www/goaccess` | — | **RECREATE** (generated HTML report) |
| traefik/crowdsec/goaccess | shared `/opt/traefik-config/logs` | — | covered by traefik-config above |
| traefik | `/var/run/docker.sock` | — | **RECREATE** (runtime socket, comes with the new container) |

`.env` files to vault: `traefik`, `crowdsec`, `grafana`, `cloudflared`.

## CT 105 — downloader stack (→ v2 stack `downloader`)

| App | Mount | Size | Verdict |
|---|---|---|---|
| qbittorrent | `/opt/downloader-config/qbittorrent` → `/config` | 8.5M | **MIGRATE** (settings, categories, torrents state) |
| qbittorrent | `/mnt/data/18TB/downloads` → `/downloads` | huge | **RECREATE the mount** — data stays on the host disks; the v2 lxc gets the same `mp0/mp1` bind mounts (`/HDD18TB/subvol-103-disk-0`, `/HDD12TB/subvol-103-disk-0`). Nothing to copy. |
| gluetun | *(no mounts)* | — | config is 100% env — **the `.env` is the config**. VPN credentials (WIREGUARD_PRIVATE_KEY, OPENVPN_USER/PASSWORD, …) → vault. Losing this file = VPN dead. |
| promtail | `/opt/downloader/promtail-config.yml` + positions | — | **RECREATE** (v2 scaffolds its own promtail; positions file is disposable) |

`.env` files to vault: `downloader` (the gluetun credentials).

## CT 106 — media stack (→ v2 stack `media`)

| App | Mount | Size | Verdict |
|---|---|---|---|
| jellyfin | `/opt/jellyfin-config` → `/config` | **16G** | **MIGRATE** (users, watch state, metadata). Biggest single item — see § Jellyfin note |
| jellyfin | named volume → `/cache` | — | **RECREATE** (transcode/image cache, regenerates) |
| jellyfin | `/mnt/data/{18TB,12TB}` | — | **RECREATE the mount** (same as 105 — host bind mounts, no copy) |
| radarr | `/opt/radarr-config` → `/config` | 1.7G | **MIGRATE** |
| sonarr | `/opt/sonarr-config` → `/config` | 494M | **MIGRATE** |
| prowlarr | `/opt/prowlarr-config` → `/config` | 52M | **MIGRATE** (indexers) |
| bazarr | `/opt/bazarr-config` → `/config` | 47M | **MIGRATE** |
| seerr | `/opt/seerr-config` → `/app/config` | 8.7M | **MIGRATE** |
| flaresolverr | named volume → `/config` | 4K | **RECREATE** (stateless; empty volume) |
| \*arr apps | `/mnt/data/{18TB,12TB}` | — | **RECREATE the mount** |
| promtail | config + positions | — | **RECREATE** (v2 scaffold) |

`.env` files to vault: `jellyfin`.

**Jellyfin note (16G):** mostly `metadata/` + `transcodes/`. Two options at
migration time: (a) copy everything — slow but zero re-scanning; (b) copy
everything except `transcodes/` and `cache/` subdirs — much smaller, Jellyfin
regenerates them. Decide at migration; default (b). **Never** skip `data/`
(watch state DB) or `config/`.

**Media paths caveat:** old containers mount data at `/mnt/data/*` but the
compose files map them to `/data/*` in-container. The v2 compose must keep
the **in-container** paths identical (`/data/18TB`, `/data/12TB`, `/config`,
`/downloads`) or every library/root-folder path in the *arr databases breaks.
In-container paths are part of the config — treat them as frozen.

## Procedure (per stack, later, with explicit go — one stack at a time)

1. **Freeze**: `pct exec <old> -- docker compose down` per app (stop writes).
2. **Copy out** (host side, container → host):
   `pct exec <old> -- tar -C /opt -cf - <app>-config | tar -C /appdata/<stack> -xf -`
   then rename to the v2 layout and `chown -R 101000:101000` (unprivileged map).
3. **Verify completeness** (the guarantee): generate a manifest on both sides
   and diff — `find <dir> -type f -printf '%P %s\n' | sort | sha256sum` inside
   the old container vs on the host copy. Byte counts + file lists must match
   exactly. A mismatch aborts the migration.
4. **Deploy** the v2 stack (new vmid) with the copied `/appdata` and the
   `.env`s pushed to the vault. Old container stays **stopped but intact**.
5. **Acceptance**: services healthy, logins/watch-state/torrents/indexers
   verified by Kenny, then a first restic backup to
   `gdrive:homelab-backups/<stack>-config` must succeed.
6. **Only then** destroy the old container (C2 gated). Until acceptance, the
   rollback is `docker compose up -d` in the old container.

## Google Drive cleanup (Kenny's note, 2026-08-11)

The loose `<app>-config` folders in the Drive **root** are the *old* Ansible
rclone copies. The v2 system writes restic repos under **`homelab-backups/`**
only (folder exists). After each stack's migration + first successful v2
backup + a verified test-restore, the corresponding loose root folders become
dead weight — Kenny archives/deletes them manually (they are the last-resort
copy until then, so not before).
