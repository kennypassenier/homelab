# Structured test plan

Test every feature step by step. Two parts:

- **Part A — Offline (now, no infra):** everything runnable today with
  `homelab tui --offline` or the headless CLI against a fake host. Proves the
  software without touching Proxmox.
- **Part B — Live pilot (after host install + explicit go):** the real deploy
  loop on a dedicated throwaway container (vmid 108). Only after the gridsim
  demo, once the HOST daemon is installed.

Each item lists its feature ID (see FEATURES.md), the steps, and what a pass
looks like. Work top to bottom; check items off as you go.

Build once:
```bash
cargo build --release --manifest-path ~/Projects/homelab/client/Cargo.toml
alias homelab='~/Projects/homelab/target/release/homelab'
```

---

## Part A — Offline (do these now)

Launch: `homelab tui --offline` (needs a real terminal, ≥100×30 recommended).

### A1 · Boot & chrome (G1)
1. Watch the splash: ASCII logo materializes through the decrypt reveal, boot
   log lands line by line, "READY :: press any key".
2. Press any key → dashboard.
- **Pass:** splash plays, lands on DASHBOARD with the connection dot green.

### A2 · Tabs, AZERTY (G1)
1. Press `1` `2` `3` `4` to switch tabs; then the unshifted AZERTY row `&` `é`
   `"` `'`; then `TAB` / `SHIFT+TAB`.
- **Pass:** every key switches tabs; the flicker/decrypt effect fires on switch.

### A3 · Effects toggle (G1)
1. Press `F2` repeatedly: FX cycles FULL → SUBTLE → OFF → FULL.
- **Pass:** glitch/pulse/scanline visibly change; OFF is calm.

### A4 · Dashboard panels + capacity (C6, G6)
1. On DASHBOARD, read HOST_MESH, LXC_MESH (3 demo stacks), CAPACITY,
   DATA_TRANSFERS.
2. In CAPACITY confirm: "RAM used" bar (actual), an "alloc … ×" overcommit
   line, and "load / cores".
- **Pass:** capacity leads with actual usage, overcommit shown as context.

### A5 · Fleet navigation
1. `UP`/`DOWN` moves the selection through the fleet table.
- **Pass:** the pulsing highlight + ▶ marker follow the selection.

### A6 · Log stream (F2)
1. Go to LOG_STREAM (`3`). Watch lines arrive.
2. `LEFT`/`RIGHT` cycles the source (ALL → HOST → each stack).
3. `UP`/`DOWN` scrolls back; the title shows "⏸ SCROLL -N"; `SPACE` toggles
   follow; `G` jumps to the tail.
- **Pass:** source filter narrows the stream; scrollback anchors; tail resumes.

### A7 · Doctor (F6)
1. Go to DOCTOR (`4`). It requests checks and renders them colored.
2. Press `R` to re-run.
- **Pass:** checks render with [Ok]/[Warn]/[Fail] and remedy lines.

### A8 · Command palette (G3)
1. Press `CTRL+K`. Type "doct". Arrow to a match. `ENTER`.
- **Pass:** fuzzy list filters; selection runs the action; palette closes.

### A9 · New-stack wizard (G2, D7)
1. On DASHBOARD/STACKS press `N`.
2. **Preset:** `UP`/`DOWN`, pick one, `ENTER`.
3. **Name:** type a name (lowercase); hostname preview updates; `ENTER`.
4. **Resources:** `UP`/`DOWN` between RAM/CPU/DISK/SWAP/VMID; `LEFT`/`RIGHT`
   adjusts; on DISK and SWAP type a number for a custom size; SWAP shows
   "(auto from RAM)" until you touch it, "off" at 0; `ENTER`.
5. **Review:** confirm derived hostname/ip/resources/swap; `ENTER`.
- **Pass:** a real `stacks/<name>/` dir is written (check on disk); status line
  says "scaffolded … press SHIFT+D to deploy". Delete the dir to undo.
6. Verify the scaffold: `cat stacks/<name>/lxc-compose.yml` — swap follows the
   formula, protection: true, no watchtower; `cat stacks/<name>/<app>/docker-compose.yml`
   has `com.homelab.update.policy=manual`, no watchtower label.

### A10 · Change-plan preview (D6)
1. Select a stack, press `P`.
- **Pass:** CHANGE_PLAN modal lists CREATE/UPDATE/SYNC + payload + safety
  gates; `ESC` cancels, `ENTER` would deploy.

### A11 · Deploy focus window (G6, F2)
1. Select a stack, press `SHIFT+D` (or `ENTER` from the plan).
- **Pass:** near-fullscreen FOCUS window; the demo deploy's steps stream into
  the task feed; the transfer flow animates during "push files"; a progress
  bar fills; on completion the border/title flip to COMPLETE; `ENTER` closes,
  `ESC` backgrounds it (feed continues in LOG_STREAM).

### A12 · Ticker (attention-first)
1. Watch the bottom strip. With the demo fleet (media drifted, no env) it
   shows "⚠ UPD pending: media", "⚠ NOENV media", app-down warnings.
- **Pass:** only actionable items scroll; nothing needing attention → "● ALL
  SYSTEMS NOMINAL" + live telemetry.

### A13 · Small-terminal guard
1. Resize the terminal below 80×24.
- **Pass:** "TERMINAL TOO SMALL" message; restores when enlarged.

### A14 · CLI validation (D10)
```bash
homelab plan stacks/syncthing      # validates locally, no network
```
- **Pass:** "✓ valid — …" or a precise validation error. Corrupt a value in
  the manifest and re-run to see the error.

---

## Part B — Live pilot on vmid 108 (after host install + go)

**Prerequisites (one-time, Proxmox host — after the demo):**
1. HOST daemon installed as a systemd service with its TLS cert + token
   (see V2_PILOT_HANDOFF.md). Confirm: `curl -k https://10.10.5.250:8443/api/health`.
2. Old zombie `host-daemon.service` stopped/disabled.
3. `~/Projects/homelab/.env` has HOMELAB_HOST + HOMELAB_TOKEN.

vmid 108 is the dedicated automated-test container; nothing else is touched.

### B1 · Link + pinning (A4)
```bash
homelab ping
```
- **Pass:** first run prints "pinned host certificate SHA256:…" (verify it
  matches the fingerprint the host printed at boot); returns "pong". A second
  run is silent-pinned. Tamper with `~/.config/homelab/pin` → next run refuses
  with a fingerprint-mismatch error.

### B2 · Doctor against the real host (F6)
```bash
homelab doctor
```
- **Pass:** real checks (disk, state, mirror…) render; overall Ok/Warn.

### B3 · Provision + full deploy (C1, C3, B2, A7, D1, B3, H1, F1)
```bash
homelab deploy stacks/synctest-108     # created below
```
- **Pass, watch the streamed transcript:** safety gates pass → pct create 108
  (with --onboot, order, protection, tags, timezone) → docker bootstrap →
  runaway guards + unattended-upgrades → files pushed → compose up → verify
  gates green → "Sync complete". Then:
  - `pct config 108` shows protection 1, onboot 1, the homelab tag.
  - `pct exec 108 -- docker inspect syncthing | grep -A4 LogConfig` → 10m×3.
  - Syncthing GUI reachable at its IP:8384.

### B4 · Idempotency (B1)
```bash
homelab deploy stacks/synctest-108     # run it again
```
- **Pass:** transcript shows skips (docker present, configs unchanged); no
  container recreate, no restarts.

### B5 · Safety gate refusal (A1, A2)
1. Temporarily edit the manifest vmid to 101 (Home Assistant) and deploy.
- **Pass:** "SAFETY ABORT: vmid 101 is on the no-touch list"; zero commands
  ran. Revert the manifest.

### B6 · Fail-closed + incident bundle (A3, AR14, AR16)
1. Break the compose (e.g. a bad image tag) and deploy.
- **Pass:** verify gate fails; deploy reports failure with a remedy; an
  incident bundle is written under `/var/lib/homelab/incidents/` containing a
  replayable `commands.sh`. Fix and redeploy → green.

### B7 · Backup + auto-restore (E1, E3, E5)
```bash
homelab ...backup 108        # once the backup verb lands (M4)
```
- **Pass:** restic snapshot created; wipe the config dir; redeploy → E3
  auto-restore refills it; syncthing returns with its config.

### B8 · Traefik route (H1) + Loki (F1)
- **Pass:** `curl -H 'Host: <route>' http://10.10.10.4` reaches syncthing;
  Loki label query includes the test stack within minutes.

### B9 · Gated destroy (C2, when it lands in M4)
```bash
homelab destroy stacks/synctest-108    # typed-name confirmation
```
- **Pass:** requires typing the stack name; lifts protection then pct destroy;
  refuses any vmid ≠ 108's hostname.

### B10 · Reboot safety (C3)
1. (Optional, off-peak) reboot the host.
- **Pass:** 108 comes back on boot in the right order; doctor green.

---

## Standing rule during automated testing
Everything in Part B is confined to **vmid 108** until Kenny takes over. The
no-touch list (100, 101, 102, 103) is enforced in code and blocks anything
else regardless of manifest content.

### B11 · Managed update + rollback (D9/B6)
```bash
homelab update stacks/synctest-108 syncthing
```
- **Pass:** steps policy → capture → pull+up → verify stream by; "update
  complete". Break it deliberately (edit the compose image to a bad tag,
  deploy, then update): verify fails → the op re-tags the captured image,
  force-recreates, reports "ROLLED BACK … now healthy" — the container is
  running the previous image. (Live-proven happy path 2026-08-11.)

### B12 · DR runbook (E7)
```bash
homelab runbook
```
- **Pass:** `docs/DR_RUNBOOK.md` regenerates: layer 0-3 recovery (incl. the
  12/18TB disks living on the host, shared via CT 103/samba), one section per
  v2 stack, legacy stacks marked LEGACY.

### B13 · Scheduler (E4) + notifications (F3) — after config
1. Add to `/etc/homelab/host.toml`: `backup_hour = 4` and
   `notify_webhook = "http://<ha>/api/webhook/homelab_ops"`; restart the
   daemon. (Requires the restic/rclone token for backups to succeed, and an
   HA webhook automation to receive F3.)
- **Pass:** journal shows "scheduler armed"; next morning state.json
  `last_backup` is fresh; HA receives a JSON payload per operation.

### B14 · Host self-update + rollback drill (H5/B7) — LIVE-PROVEN 2026-08-11
```bash
homelab self-update target-debian/release/homelab-host
```
- **Pass (happy path):** selfcheck → backup → install → arm marker →
  restart; ~8s later `/api/version` reports the new version and the journal
  says "self-update accepted". (Proven: 2.1.0 → 2.1.1 over the TLS line.)
- **Pass (garbage binary):** shipping a non-executable is refused at the
  selfcheck gate with "Exec format error"; nothing touched. (Proven.)
- **Pass (crash-on-start release):** a binary that passes selfcheck but dies
  on start crash-loops; systemd StartLimit → OnFailure → rollback script
  restores the previous binary and restarts. Daemon returns on the old
  version, marker cleared. (Proven — full automatic recovery.)
- B7: `systemctl show homelab-host -p Type,WatchdogUSec` → notify / 30s;
  a hung (not crashed) daemon is killed and restarted by systemd.

### B15 · Backup→restore round-trip (E1/E2) — LIVE-PROVEN 2026-08-11
```bash
homelab backup stacks/synctest-108
```
```bash
homelab restore stacks/synctest-108
```
- **Pass:** backup: init → quiesce → "snapshot <id> saved" → resume →
  retention; the repo appears in Drive under `homelab-backups/<stack>-config`.
  Restore: validate → quiesce → restore → resume → verify health, all green.
  (Proven: snapshot 36a7361d, full drill.) NOTE: currently on the legacy
  shared rclone client + a TEMP restic password — swap both before real
  migrations (see host `/var/lib/homelab/secrets/restic.pw`).

### B16 · Fleet patch (H6) — LIVE-PROVEN 2026-08-11
```bash
homelab patch
```
- **Pass:** one "patch <stack>" step per managed stack, sequential,
  fail-closed; no-touch vmids never appear.

### B17 · Definitive backup credentials — LIVE-PROVEN 2026-08-11
Host rclone remote `gdrive` now uses **Kenny's own OAuth client** (fresh
token via `rclone authorize`), and `/var/lib/homelab/secrets/restic.pw` holds
the **real** restic password. Proven: fresh snapshot f9bc71f8 + green restore
drill via the new credentials; the OLD pre-v2 restic repos in the Drive root
(e.g. `downloader-config`, 2 snapshots) open with the same password — they
remain the last-resort recovery layer during migration. Scheduler armed:
`backup_hour = 4` in host.toml ("scheduler armed" in the journal).

### A15 · SETTINGS tab (G8) — offline
1. `homelab tui --offline` → press `5` (AZERTY: `(`).
- **Pass:** HOST_SETTINGS renders: NIGHTLY RUN hour (◂ 04:00 ▸), retention
  tiers ("every Xd for Y days / forever"), WEBHOOK row, sync indicator.
  `UP/DOWN` moves fields, `LEFT/RIGHT` edits values, `A`/`D` adds/removes a
  tier, `ENTER` on WEBHOOK opens a text editor (keys are swallowed while
  editing), `SHIFT+S` saves (demo acks).

### B18 · Settings round-trip (G8) — live
```bash
homelab config
```
- **Pass:** prints nightly run / webhook / retention tiers as stored on the
  host. In the TUI: edit a value, SHIFT+S → "settings saved and applied";
  `ssh root@10.10.5.250 cat /etc/homelab/host.toml` shows the change +
  `[[retention]]` tables; the scheduler uses the new hour without a restart.

### B19 · Failure-path webhooks (F3) — LIVE-PROVEN 2026-08-11
Three events beyond per-op notifications, all captured live against a local
webhook catcher:
- `host-online` — 3s after daemon start: `{op, ok, version}`; `ok:false` +
  an `interrupted:` error text when AR13 found mid-flight operations. This
  is the power-cut answer: HA hears the homelab came back.
- `self-update-rollback` — sent by the OnFailure script when a bad release
  is automatically rolled back (the daemon can't report its own death).
- `daemon-failed` — sent when the daemon crash-loops and systemd gives up
  (proven with a 6×SIGKILL drill; daemon recovered with reset-failed+start).
To arm for real: set the webhook URL in the SETTINGS tab (or host.toml) to
an HA webhook automation.

### B21 · Release-driven host update (H7) — LIVE-PROVEN 2026-08-12
First official release v3.0.0: `make release VERSION=3.0.0` → CI gate →
GitHub Release with SHA256SUMS → `homelab release-update` downloaded,
checksum-verified and shipped the binary over the line; host went
2.6.0 → 3.0.0 through the H5 pipeline (backup, armed rollback, restart,
marker cleared healthy). The TUI badge + U key run the identical code path
and will surface at the next release.

### B22 · Enabled flag, light variant (H8) — LIVE-PROVEN 2026-08-12
`homelab disable synctest` → `onboot: 0` in 108.conf + `"enabled": false`
in state.json, container stayed running (the flag never touches run state).
`homelab enable synctest` → `onboot: 1` + `"enabled": true`. Scheduler-skip
and auto-disable-on-failed-nightly are unit-tested; the next failed nightly
run is the live proof of the auto-park path.

### B23 · Host-meta backup + restore drill (H10) — LIVE-PROVEN 2026-08-27
The gap that started this: `homelab-backups/host-meta` did not exist —
`backup_host_meta` was never called by any code path, so the vault, state
and TLS material had never been backed up. After wiring it into the nightly
plan (v3.0.1): snapshot `ef93763b` written to
`gdrive:homelab-backups/host-meta-config` (53 files, 35 KiB), then restored
to a scratch dir on the host — `restic.pw` and `tls-key.pem` byte-identical,
intent repo incl. `.git` present. `state.json` differs by design: the
scheduler stamps `last_host_meta` AFTER the snapshot completes.

### B24 · ZFS snapshots + replication (E8) — LIVE-PROVEN 2026-08-27
Seeded `HDD2TB → HDD18TB/replica/HDD2TB` (10.5 MiB) and
`HDD4TB → HDD18TB/replica/HDD4TB` (45.3 GiB); the second run went
incremental (`zfs send -RI`) as intended. Two refusals proven on real data
before that: a full seed into a target holding foreign snapshots (caught by
ZFS, then by us — fix in v3.1.1), and an incremental into the legacy
REPLICA_* subtree whose parent/child snapshots no longer line up. The old
history (53 snapshots) is untouched; the retired cron script is archived at
docs/legacy/full_zfs_backup.sh.

### B25 · Secrets from latch, end to end (D12) — LIVE-PROVEN 2026-08-29
Against the real latch 2.2.0 (signed release): `latch init` on stacks/, a
test variable committed+pushed to env `prod`, plaintext deleted, then
`homelab deploy stacks/synctest-108` with `latch_secrets: [syncthing]` and
`HOMELAB_LATCH_ENV=prod`. The variable arrived in the host vault
(`secrets/synctest/syncthing.env`) AND in the container
(`/opt/synctest/syncthing/.env`) while the workstation held zero plaintext
.env files throughout (checked). Failure path proven separately against the
real binary: latch's remedy passes through verbatim, empty stdout, exit 1.
Cleanup: ciphertext removed from latch, stack file reverted; the stacks/
latch project link remains for real use.

### B26 · Metrics stack live (F4) — LIVE-PROVEN 2026-08-29
CT 113 (`113-app-metrics`) deployed from the stack definition: Prometheus
(90d retention) + pve-exporter + promtail. All targets up: kyu
(10.10.10.9:8080, `kyu_sweeper_age_ms` scraping), pve (25 `pve_up`
series — the whole park), prometheus itself. The pve-exporter credentials
travelled via latch (D12's first production use): PVEAuditor token created
on the host, staged on tmpfs, committed to latch env prod, never a readable
file on the workstation. Found and fixed B1 protection-ordering bug on the
way (protection now last, after drive changes). Grafana datasource +
dashboard pending Kenny's service-account token (MB5).

### B27 · Native-service adoption + backup (C7) — LIVE-PROVEN 2026-08-29
CT 109 (kyu) and CT 112 (almanac, ExecStart wrapped in `latch run`)
adopted via `homelab adopt stacks/<name>`; both services stayed `active`
throughout — adoption touched nothing. First native backups written
(kyu-config 660 KiB, almanac-config 13 MiB) and the kyu snapshot
restore-drilled: the tar lists the real database files. Update supervision
(preserve → restart-if-changed → health → rollback) is mock-proven in 9
tests; the live broken-release drill is pending coordination with the
kyu project.

### B20 · HA webhook receiver (F3, HA side) — LIVE-PROVEN 2026-08-11
Host `notify_webhook` points at `automation.homelab_ops_webhook`
(`/api/webhook/homelab-ops-c4d81f26`, local-only, POST). Every event is
appended to `/media/homelab_events.log` on HA (Media browser → local);
**no notifications by default**. Failures (`ok:false`) additionally route
through `script.notification_dispatch` as a warning ONLY when
`input_boolean.homelab_event_notifications` is on (default off).
- **Pass (proven):** daemon restart → `host-online` line in the log;
  `homelab patch` → `patch-fleet` line. Notify entity timestamp matches each
  webhook trigger.
- **Kenny's test:** flip the toggle on, break something deliberately (e.g.
  deploy with a bad image), expect a warning push with ACK; flip back off.

### A16 · Data-driven presets (G2) — offline
```bash
homelab presets
```
- **Pass:** lists the catalog from `presets/` (6 entries, custom last), no
  "(built-in fallback)" markers.
1. Add a throwaway preset: `mkdir -p presets/test-x/hello`, write a
   `preset.yml` (description + ram_mb) and a `hello/docker-compose.yml`
   using `__STACK__` placeholders (copy from an existing preset).
2. `homelab tui --offline` → `N` → "test-x" appears in the wizard; finish
   the wizard with a test name.
3. Inspect `stacks/<name>/`: placeholders substituted, promtail injected,
   manifest apps list = app dir names. `homelab plan stacks/<name>` → valid.
4. Delete the stack dir + `presets/test-x`.
- Full recipe: docs/PRESET_GUIDE.md.

### B21 · Golden template + clone provisioning (B8) — LIVE-PROVEN 2026-08-11
```bash
homelab template-build
```
- **Pass (proven):** 7-step build on temp vmid 999 (claim → create → bake
  docker → bake guards → generalize → convert); `pct config 999` shows
  `template: 1`, named debian-12-homelab-v1.
- **Clone deploy (proven):** with `template: "clone:999"` in the manifest,
  destroy + full redeploy of synctest-108 took **52 seconds** end to end;
  transcript shows `pct clone 999 108` and "bootstrap docker :: ok
  (no change)" — nothing installed, everything was baked in. StackDefaults
  now defaults new stacks to clone:999.
- **Rebuild:** bump the version (`homelab template-build 999 2`) after
  destroying the old template (999 is refused while it exists — by design).

### B22 · Hot-apply resources (C4) — LIVE-PROVEN 2026-08-11
```bash
homelab resize stacks/<name>
```
- **Pass (proven):** raise memory_mb/cores/disk_gb in the manifest → applied
  live to the running container (108: 512→1024 MiB, 1→2 cores, no restart).
  A shrink attempt while running is refused with a clear remedy; RAM/cores
  shrink is allowed stopped; disk shrink never.

### B23 · Template discovery (C5) — LIVE-PROVEN 2026-08-11
```bash
homelab templates
```
- **Pass (proven):** lists clonable golden templates (`clone:999
  debian-12-homelab-v1`) and the OS tarballs from pveam.

### A17 · Stack bundles (D11) — offline, LIVE-PROVEN 2026-08-11
```bash
homelab export stacks/<name>
```
```bash
homelab import <bundle.yml> <new-name> <vmid>
```
- **Pass (proven):** export writes a single YAML (secrets excluded even when
  a .env exists); import substitutes the full identity (name, vmid, derived
  ip/hostname, /appdata paths, network) and validates with the deploy
  validator.

### A18 · Remote shell tab (G4) — offline + live
1. `homelab tui` → press `6` (AZERTY: `§`). Typing goes to the prompt
   (digits don't switch tabs); `LEFT/RIGHT` with an empty input picks the
   target stack; `ENTER` runs; `UP` recalls the last command.
- **Pass (live):** with `exec_enabled = true` in host.toml a command like
  `uptime -p` returns inline with a colored exit status; with it off, the
  SAFETY ABORT explanation appears in the pane; a no-touch target is always
  refused. Every command lands in /var/lib/homelab/audit.log.

### B24 · Kea DHCP reservation (H2) — needs OPNsense API creds
1. Create an API key on OPNsense (System → Access → Users → API keys) and
   put `key:secret` in /var/lib/homelab/secrets/opnsense (0600); add to
   host.toml: `opnsense_url = "https://10.10.10.1"` and
   `opnsense_cred_file = "/var/lib/homelab/secrets/opnsense"`; restart.
2. Deploy a NEW container (e.g. rebuild 108).
- **Pass:** transcript shows "[kea] reserved <ip> for <mac>"; the
  reservation appears in OPNsense → Services → Kea DHCP → Reservations.
  With OPNsense unreachable the deploy still succeeds with a loud warning.

### A19 · Metrics preset (F4)
1. `homelab presets` shows `metrics` (prometheus + cadvisor + pve-exporter).
2. Scaffold + deploy it to a test vmid; create
   `stacks/<name>/pve-exporter/.env` with PVE_USER/PVE_TOKEN_NAME/
   PVE_TOKEN_VALUE (PVEAuditor token) before deploying.
- **Pass:** Prometheus on :9090 shows all three targets UP; add it as a
  Grafana datasource on the platform stack.

## Not covered, by decision (phase-7 form, 2026-08-11)

- **H9 (Later):** op-lock queue visibility + cancel. Mutating operations
  serialize silently; a waiting command shows no queue position and cannot
  be cancelled short of restarting the daemon. Chosen Later by Kenny; the
  light variant (a "waiting behind <op>" event + reads outside the lock)
  is the intended first step.
- **H22 (Accepted):** (a) the API bearer token is effectively a host-root
  credential — self-update installs arbitrary binaries by design (single
  token + unsigned releases were explicit AR decisions); guard the token
  accordingly. (b) A failing app's last 20 log lines are captured into
  incident bundles; apps that log secrets to stdout put them there
  (bundles are root-only). Both recorded as known properties, not bugs.
- **D9 "auto-after-N-days" (registry text)**: only `manual`/`auto` exist in
  code. Flagged for a FEATURES.md amendment or a future implementation —
  pending Kenny's call in the completion report.
