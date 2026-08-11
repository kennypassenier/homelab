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
no-touch list (100-107, 111, 201-203) is enforced in code and blocks anything
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
