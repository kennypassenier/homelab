# homelab-admin — inventory (Phase 1 input, DRAFT)

Measured 2026-09-28 by three read-only sweeps (protocol and data, bases,
hosting). Facts only; every line names where it was read. The Phase 2 feature
list is built from this.

## 1. The protocol today (proto/src/lib.rs)

- 46 `Command` variants (lib.rs:29-310), 7 `ServerMsg` kinds: Hello, Log, Ask,
  Transfer, State, Config, RpcDone (lib.rs:434-476). One WSS line on
  `/api/ws`, bearer token, pinned self-signed TLS (host/src/main.rs:2873-2879).
- Mutating commands take the global op lock (AR12, main.rs:4870), stream `Log`
  lines, end with one `RpcDone`. `Log`/`Ask`/`Transfer`/`State`/`Config` are
  broadcast to every session; log lines carry **no request id**.
- Structured replies: only `Today`, `GetApplied`, `State`, `Config`. Fleet
  check, doctor, incidents, incident show, manual checks, templates and the
  wipe plan return **preformatted text** in `RpcDone.message`.
- `GetState` hard-codes `running=true`, `restarts=0`, `online=true`,
  `env_sealed=true`, `cpu_pct=0`, `ram_pct=0` (main.rs:6440-6480).
- Only the deploy's "service checks" step asks the operator (deploy.rs:3434);
  the CLI cannot answer (client/src/main.rs:1801-1810), the TUI can (`a`/`s`).
- `HostConfigView` exposes 3 of ~45 host.toml keys (backup_hour,
  notify_webhook, retention; lib.rs:314-322). The rest: ssh only.
- No snapshot list, no container log tail, no metric series over the line.

## 2. Data and where it lives

| Data | Source of truth | Edited by |
|---|---|---|
| Stack definition (manifest, compose, routes, firewall, checks, labels) | git repo `stacks/` (15 dirs, 157 files, 36 checks.yml, 32 compose, 6 service.yml) | files + deploy/apply |
| Applied intent | host `/var/lib/homelab/repo` + `applied_hash` | deploy |
| Per-stack record, drills, second copies, integrity, pins, upstream releases, manual checks, retired, parked | host `state.json` (core/src/state.rs:16-312) | nightly, commands |
| Host settings | `/etc/homelab/host.toml` (FileConfig, main.rs:70-199) | 3 keys via SetConfig, rest ssh |
| Secrets | latch → sealed on host `<state_dir>/secrets/<stack>/<app>.env` | latch + deploy |
| Incidents | `/var/lib/homelab/incidents/` (≤200, 90 days) | written on failure |
| Logs / metrics | Loki, Prometheus on CT 113 | — |

## 3. TUI against CLI

6 tabs (DASHBOARD, STACKS, LOG_STREAM, DOCTOR, SETTINGS, SHELL;
client/src/tui/model.rs:18). 29 of 48 CLI verbs have no TUI equivalent,
among them: checks (list/answer), incidents show, apply, resize, patch,
zfs-replicate, backup-host-meta, backup-devices, templates, template-build,
release-update-native, rollback-native, destroy, forget, wipe, prune-orphans,
export, import, runbook; restore only "latest", update only whole stacks.
Key map: client/src/tui/keys.rs:74-304.

## 4. Bases

- **chassis-rs 2.2.1**: axum 0.8, features core / assets / dashboard /
  passkeys / self-update / notify / testing. Dashboard = minijinja templates,
  token + cookie login, passkeys, kp-themes vendored at `/static/*`. CSP
  `script-src 'self'`; no hook for a project's own static files
  (docs/DASHBOARD.md:432); no SSE/WebSocket. kyu works around it with its own
  `/assets` route. kyu-runner enables only `core` + `self-update`.
- **kp-themes 7.2.0**: 22 themes; framework-free CSS + JS modules (`js/auto.js`)
  and 21 React components. Dashboard pieces: kp-datatable, kp-dialog,
  kp-tabs, kp-toast, kp-log, kp-form/field, **kp-wizard** (js/wizard.js),
  kp-sidenav, kp-health, kp-timeline, kp-diff. No chart component.
- **JobTracker** (nearest web precedent): Node 26 + Express 5 + React 19/Vite,
  kp-themes via git tag, scrypt password + passkeys, JSON + git storage,
  docker image on ghcr, CT 116.

## 5. Hosting facts (pve, measured read-only 2026-09-28)

- The daemon runs every command locally as root: pct ×52, zfs, restic on host
  paths, rclone, writes in `/etc/pve`; never the Proxmox API; root@pam has no
  API token.
- pve: 12 cores, 31.8 GB RAM (14.7 GB available); daemon 12.9 MB.
- One wildcard edge `*.kp-soft.dev` → Traefik :80 on CT 104 behind Cloudflare
  Access; **no LAN-only hostname pattern exists yet** (only precedent: D78,
  GoAccess on a bare IP:port).
- Open panel findings that bear on the dashboard: traefik-lan-host-header-bypass,
  pve-management-on-container-vlan, api-token-is-root (fix-120),
  small-sharp-edges ("make host settings declarative before any web UI").
