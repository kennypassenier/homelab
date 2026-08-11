# V2 Pilot Handoff — exact state & remaining steps

> Written 2026-08-10 on branch `v2-merge`. Live deployment was deliberately
> **paused before touching the Proxmox host** because the gridsim school demo
> (Aug 15–31) depends on it. Verified at pause time: host healthy, uptime 10d,
> all guests running, **nothing of v2 installed** (no `/etc/homelab`, no
> binary, no service, no `/appdata`).

## What is DONE and committed

| Piece | Where | State |
|---|---|---|
| Cyberpunk TUI mockup (simulated data) | `tui-preview/` | Working: splash, dashboard, stacks+wizard (form-style resources, custom disk entry), deploy & backup focus modes, logs with source selector/scroll, real-data ticker, command palette, FX toggle. `cargo run --release -p tui-preview` |
| Cargo workspace | root `Cargo.toml` | Members: proto, host, client, tui-preview. Legacy crates (client-app, host-daemon, lxc-daemon) excluded, untouched. |
| Protocol crate | `proto/` | WS JSON: RpcRequest{Ping,Status,DeployStack}, ServerMsg{Hello,Log,RpcDone}; StackManifest (lxc-compose v2, intent-only), FileBlob, env map (secrets channel), GatewayRoute. |
| HOST daemon (merged architecture) | `host/` | axum WS on :8443, required bearer token; deploy pipeline: safety gates → /appdata storage → pct create/start → wait systemd → docker bootstrap (get.docker.com) → local git commit (/var/lib/homelab/repo) → pct push files + .env (0600, vault copy in /var/lib/homelab/secrets) → compose pull/up → verify (compose ps + diagnostics) → gateway route push → state.json. **No destroy path exists.** NO_TOUCH list hardcoded: 100-107,111,201-203; QEMU-vmid check protects all VMs (incl. template 9000); existing CT reused only when hostname == `{vmid}-app-{stack}`. |
| CLIENT CLI | `client/` (binary `homelab`) | `homelab ping|status|deploy stacks/<name>`; reads manifest+files, `.env` files go over the secrets channel; streams HOST logs live; env `HOMELAB_HOST`/`HOMELAB_TOKEN` (auto-read from `.env`? no — export or `set -a; . ./.env; set +a`). |
| Syncthing stack definition | `stacks/syncthing/` | Per vault note §4: vmid 110, `110-app-syncthing`, 10.10.10.10, 512MB/1c/4G, onboot order=50, unprivileged+nesting; data on host `/appdata/syncthing/syncthing-config` (owner 101000); apps: syncthing + promtail (Loki 10.10.10.4); traefik fragment `sync.kp-soft.dev` → 8384 (GUI only, sync protocol stays LAN). |
| Debian-compatible binary | `target-debian/release/homelab-host` (gitignored) | Built in `rust:1-bookworm` Docker. Rebuild: `docker run --rm -v "$PWD":/w -w /w -v homelab-cargo-cache:/usr/local/cargo/registry -e CARGO_TARGET_DIR=/w/target-debian rust:1-bookworm cargo build --release -p homelab-host` |
| API token | local `.env` (gitignored, 0600) | Also staged at the session scratchpad as `host.env` for scp — regenerate if stale: `openssl rand -hex 32`. |

## REMAINING — run only AFTER the school demo

Every step below touches the Proxmox host. In order:

1. **Install HOST** (was blocked mid-flight; nothing landed):
   ```bash
   scp target-debian/release/homelab-host root@10.10.5.250:/usr/local/bin/homelab-host
   scp <env-file> root@10.10.5.250:/etc/homelab/host.env   # mkdir -p /etc/homelab first; chmod 600
   ```
   Unit file `/etc/systemd/system/homelab-host.service`:
   ```ini
   [Unit]
   Description=Homelab HOST daemon (v2)
   After=network-online.target pve-cluster.service
   Wants=network-online.target
   [Service]
   EnvironmentFile=/etc/homelab/host.env
   ExecStart=/usr/local/bin/homelab-host
   Restart=always
   RestartSec=5
   [Install]
   WantedBy=multi-user.target
   ```
   Then `systemctl daemon-reload && systemctl enable --now homelab-host` and
   check `curl http://127.0.0.1:8443/api/health` → `ok`.
2. **Smoke test from desktop**: `set -a; . ./.env; set +a; ./target/debug/homelab ping` (expect HOST hello + pong).
3. **Pilot deploy**: `homelab deploy stacks/syncthing` — watch the streamed
   transcript. Pre-verified on 2026-08-10: template `debian-12-standard_12.12-1`
   present, vmid 110 free on both pct and qm, 57G free on pve-root.
4. **Verify**:
   - `pct status 110` running; GUI at `http://10.10.10.10:8384`
   - traefik: `curl -H 'Host: sync.kp-soft.dev' http://10.10.10.4` (fragment is
     picked up by the file-provider watch — no traefik restart)
   - Loki: `curl 'http://10.10.10.4:3100/loki/api/v1/label/stack/values'`
     should now include `syncthing`
   - Reboot-safety: `pct config 110` shows `onboot: 1`, `startup: order=50`
5. **Syncthing app setup** (manual, one-time): set a GUI password, add the
   vault folder at `/var/syncthing/obsidian-vault`, enable **staggered file
   versioning**, pair desktop + phone device IDs. Do NOT expose 22000 publicly.
6. **Cloudflare**: if `sync.kp-soft.dev` doesn't resolve, add the DNS record to
   the tunnel (catch-all ingress already routes it to traefik). Kenny does this
   in the CF dashboard.

## Known deviations / notes for the next session

- **TLS is not yet on the CLIENT↔HOST line** (token-only). Acceptable for the
  syncthing pilot (its payload has no secrets); implement rustls + pinned
  self-signed cert (plan Phase 2) before any stack with a real `.env` ships.
- The old crash-looping `host-daemon.service` (June zombie) is still present on
  pve — harmless (never binds), clean up during Phase 4: `systemctl disable --now host-daemon`.
- TUI is not yet wired to the real protocol — next big step: give tui-preview a
  backend trait (SimBackend | RemoteBackend over `proto`), so the mockup
  becomes the real CLIENT.
- `homelab status` output is raw text; format it later.
- **Runaway guards — IMPLEMENTED in HOST** (`apply_runaway_guards`, runs on
  every deploy, idempotent; closes vault "Homelab Open Issues" §2 fleet-wide):
  1. Docker json-file log caps (10m × 3) via `/etc/docker/daemon.json`,
     applied before app containers start; 2. journald capped
     (SystemMaxUse=100M, 1 month retention); 3. logrotate policy for
     syslog/auth.log (daily, 7 rotations, maxsize 50M); 4. weekly
     `docker system prune --filter until=168h` systemd timer (catches
     watchtower's stale image layers); 5. apt periodic autoclean + clean.
  Post-pilot verification: `docker inspect syncthing | grep -A4 LogConfig`,
  `systemctl list-timers docker-prune.timer`, `journalctl --disk-usage`.
- **Syncthing's own growth**: during app setup (step 5) configure staggered
  versioning **with a max age** (e.g. 365 days) — staggered mode then cleans
  old versions automatically; the vault itself is ~10 MB so bounded.
- Old `stacks/` entries (cloudflared, gateway, todo) are legacy-format and
  ignored by the new code; migrate or archive them in Phase 4b.
- Full plan (feature verdicts, phases, no-touch list) lives in the planning doc
  from this session; the authoritative no-touch list is enforced in
  `host/src/main.rs` (`NO_TOUCH`).
