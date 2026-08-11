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
