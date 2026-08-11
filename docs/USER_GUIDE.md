# User guide — every feature, keyed to its ID

What each feature does and how you use it, in the order you'll meet them.
IDs refer to [FEATURES.md](FEATURES.md); test steps live in
[TEST_PLAN.md](TEST_PLAN.md). Commands assume the repo root as working
directory with `HOMELAB_HOST`/`HOMELAB_TOKEN` in `.env`.

## Daily driver

```bash
homelab tui            # the control deck (all six tabs)
homelab tui --offline  # same TUI against a fake host — safe to explore
```

| Tab | Key (AZERTY) | What lives there |
|---|---|---|
| DASHBOARD | `1` (`&`) | host mesh, fleet, capacity (C6), transfers (G6) |
| STACKS | `2` (`é`) | per-stack detail, deploy `SHIFT+D`, plan `P`, new `N` |
| LOG_STREAM | `3` (`"`) | live logs (F2): `LEFT/RIGHT` source, `SPACE` follow |
| DOCTOR | `4` (`'`) | self-checks (F6), `R` re-runs |
| SETTINGS | `5` (`(`) | backup hour, retention tiers, webhook (G8) |
| SHELL | `6` (`§`) | remote REPL via audited exec (G4) |

`CTRL+K` opens the command palette (G3) everywhere; `F2` cycles effects;
`H` help; `Q` quits.

## Creating things

- **G2 · New-stack wizard** — `N` on the dashboard: preset → name →
  resources (RAM ladder, typed disk/swap, vmid pre-checked as free) →
  review → scaffold. Writes a real `stacks/<name>/` you could also have
  written by hand.
- **D7/G2 · Presets** — the wizard's catalog is the `presets/` directory:
  data, not code. Add an app = two files; see [PRESET_GUIDE.md](PRESET_GUIDE.md)
  and, for converting a vendor compose with an LLM,
  [LLM_COMPOSE_CONVERSION.md](LLM_COMPOSE_CONVERSION.md).
  `homelab presets` lists the catalog.
- **D8 · Core apps** — every new stack gets promtail from `presets/_core/`
  automatically; delete its dir from the stack to opt out.
- **A5 · Secrets** — put `stacks/<name>/<app>/.env` next to the compose;
  deploy ships it over the TLS line into the host vault
  (`/var/lib/homelab/secrets/`). Never in git (gitignored), never in
  bundles, never in presets.

## Deploying and changing

- **D1/C1 · Deploy** — `homelab deploy stacks/<name>` (or `SHIFT+D`).
  Pipeline: validate → safety gates → storage → provision → bootstrap →
  guards → commit intent → push files → compose up → verify → route →
  state. Fully idempotent (B1): run it twice, the second run changes
  nothing.
- **D6 · Plan preview** — `P` in the TUI or `homelab plan stacks/<name>`
  (local validation only, no network, no token needed).
- **B8 · Golden template** — new stacks provision in seconds via
  `template: "clone:999"` (the default). Rebuild the template after big
  Debian updates: destroy CT 999, then `homelab template-build 999 <v+1>`.
  `homelab templates` (C5) lists what's available.
- **B4 · Drift** — the dashboard flags `[UPD]` when your local stack files
  differ from what the host last applied. Redeploy to converge.
- **C4 · Hot-resize** — raise `memory_mb`/`cores`/`disk_gb` in the manifest,
  then `homelab resize stacks/<name>`: applied live, no restart. Shrinks
  are refused while running (RAM/cores allowed stopped; disk never).
- **H4 · Hardware** — `gpu: true` (VAAPI, /dev/dri with correct gids) or
  `vpn: true` (/dev/net/tun) under `lxc:` in the manifest; presets may set
  them (jellyfin does).
- **D3 · App add/remove** — add/remove an app dir + manifest entry,
  redeploy; removed apps are composed down, config dirs are kept.
- **D11 · Share a stack** — `homelab export stacks/<name>` → one YAML,
  secrets excluded; `homelab import <bundle> <newname> <vmid>` re-derives
  the whole identity.

## Updating

- **D9/B6 · Managed updates** — `homelab update stacks/<name> [app]`:
  capture running image → pull → up → verify; on a failed verify it
  re-tags the previous image and force-recreates (automatic rollback).
  Per-app label `com.homelab.update.policy`: `manual` (default) or `auto`
  (nightly run updates it unattended).
- **H6 · Fleet patching** — `homelab patch`: serial apt dist-upgrade across
  every managed stack, fail-closed on the first error.
- **H5 · Host self-update** — `homelab self-update <new-binary>`: selfcheck
  gate → backup → install → armed rollback marker → restart. A release that
  crashes on start is rolled back automatically by systemd (proven with a
  deliberate broken release). Build the Debian binary with
  `docker run … rust:1-bookworm cargo build --release -p homelab-host`
  (CARGO_TARGET_DIR=target-debian).

## Backups and recovery

- **E1 · Backup** — `homelab backup stacks/<name>`: restic snapshot of the
  stack's `/appdata` dirs to `gdrive:homelab-backups/<stack>-config`.
  Containers labeled `com.homelab.backup.pause=true` are stopped during
  the snapshot (E4).
- **G8 · Retention** — tiered, editable in SETTINGS: e.g. daily for 7d →
  every 14d for 60d → every 60d forever. Computed by our engine, not
  restic's fixed buckets; the newest snapshot is never deleted.
- **E4 · Scheduler** — nightly at the SETTINGS hour: backup + auto-policy
  updates per stack, using the manifests stored in host state (no client
  needed).
- **E2 · Restore** — `homelab restore stacks/<name> [snapshot]` (default
  latest): validate → quiesce → restore → resume → verify.
- **E7 · DR runbook** — `homelab runbook` regenerates
  [DR_RUNBOOK.md](DR_RUNBOOK.md), the document for when everything is down.
- **C2 · Destroy** — `homelab destroy stacks/<name>`: typed-name confirm,
  no-touch check, hostname guard, lifts Proxmox protection deliberately.
  `/appdata` data survives for a later redeploy.

## Observability & control

- **F1/F2 · Logs** — promtail ships to Loki fleet-wide; the LOG_STREAM tab
  shows the live operation feed.
- **F3 · Events → Home Assistant** — one webhook POST per finished
  operation (`{op, ok, error}`), plus `host-online` at boot,
  `self-update-rollback` and `daemon-failed` from systemd. All land in
  `/media/homelab_events.log` on HA; flip
  `input_boolean.homelab_event_notifications` on to get warnings for
  failures.
- **F6 · Doctor** — `homelab doctor` or tab 4.
- **A6/G4 · Remote exec + SHELL tab** — off by default; enable with
  `exec_enabled = true` in host.toml. Every command is appended to
  `/var/lib/homelab/audit.log`; no-touch vmids are refused regardless.
- **H2 · Kea reservations** — with `opnsense_url` + `opnsense_cred_file`
  configured, every new container's IP+MAC is registered in Kea
  automatically (fail-open).
- **D4/D5 · History + mirror** — every deploy commits intent to
  `/var/lib/homelab/repo`; set `mirror_remote` in host.toml for an
  automatic offsite push (never blocks a deploy).

## Safety model (always on)

- **A1** hardcoded no-touch list (HA, OPNsense, k3s, omada, fileserver, and
  the un-migrated stacks) — every mutating operation checks it.
- **A2** hostname guard: a vmid must carry the expected `<vmid>-app-<name>`
  hostname before anything touches it.
- **A3** fail-closed: any failed step aborts the operation and writes an
  incident bundle (see [DEBUGGING_GUIDE.md](DEBUGGING_GUIDE.md)).
- **A4** TLS with a pinned self-signed cert + bearer token on the one
  CLIENT↔HOST line; **A7** unattended security updates in every container;
  **B2** runaway guards (log caps, restart limits); **B7** systemd watchdog
  on the daemon itself.
