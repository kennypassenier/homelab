# Feature registry

The permanent, authoritative feature list for homelab v2. **Feature IDs (A1…H6)
are stable identifiers** — use them in commits, code comments, docs and
conversations. Ratings come from Kenny's review round 1 (2026-08-10):
**Must** (onmisbaar) · **Should** (gewenst) · **Could** (later) · **Won't**
(bewust niet) · **Pending** (round 2 in progress).

Every feature lists test scenarios: how to prove it works, and where possible
how to test it automatically. `MockExecutor` = the command-execution trait test
double (no real pct/docker calls); `pilot` = the live syncthing stack.

---

## A · Security

### A1 · Whitelist-only management + no-touch list — **Must**
Only manifests are ever acted on; a hardcoded NO_TOUCH list (100-107, 111,
201-203) refuses the pre-existing guests outright. `pct list` is never
enumerated for management.
- **Auto**: unit test — deploy manifest with vmid 101 → `SAFETY ABORT`, and
  MockExecutor proves zero commands ran.
- **Auto**: property test — every vmid on the no-touch list is rejected in
  every operation type.
- **Manual**: dry-run against the live host lists SKIP for every unmanaged guest.

### A2 · Hostname guard before destructive actions — **Must**
Live hostname must equal `{vmid}-app-{stack}` before an existing container is
reused, changed or (C2) destroyed.
- **Auto**: unit test — MockExecutor returns `hostname: something-else` for
  `pct config` → operation aborts before any mutating command.
- **Manual**: rename a scrap container, attempt deploy, verify abort message.

### A3 · Fail-closed behavior — **Must**
Any failure lands on the safe side: failed provision → `enabled=false` until
re-enabled; missing `.env` → abort before compose.
- **Auto**: unit test — inject step failure at each pipeline stage; assert the
  stack ends disabled and no later stage ran.
- **Auto**: deploy spec with app requiring env but no vault entry → abort.

### A4 · TLS with pinned certificate on the client-host line — **Must** *(upgraded by Kenny, round 2)*
rustls + install-time self-signed cert; CLIENT pins the fingerprint on first
connect (TOFU, like SSH). Decisive argument: without TLS the bearer token
itself is sniffable on the LAN → full API control for any eavesdropper.
- **Auto**: integration test — connection with wrong/changed cert fingerprint
  is refused by the client; plain-HTTP connection attempt is refused by HOST.
- **Auto**: packet-capture test in CI harness — no plaintext token or env
  content on the wire during a deploy against a local HOST instance.

### A5 · Secrets vault on HOST — **Must**
`.env` content lives only in `/var/lib/homelab/secrets/` (0600) and inside the
target container; never in git, mirrors or logs.
- **Auto**: unit test — after a deploy with env, the HOST git repo contains no
  env content (grep the committed tree); vault file exists with mode 0600.
- **Auto**: log-scrub test — transcripts of an env push never contain values.
- **Manual**: redeploy a wiped container and confirm `.env` reappears from vault.

### A6 · Remote exec endpoint (off by default) — **Should**
Token-gated command execution in managed LXCs via HOST; disabled unless
explicitly enabled in host config; every invocation audit-logged.
- **Auto**: integration test — endpoint returns 403/404 when disabled (default).
- **Auto**: with the flag on, command runs and an audit line is written.
- **Auto**: exec against a no-touch vmid is refused regardless of config.

### A7 · Unattended security updates in every LXC — **Must**
Security-only, no auto-reboot, unused deps removed. Applied idempotently by the
bootstrap (already implemented).
- **Auto**: unit test — guard step emits the exact 50unattended-upgrades content.
- **Manual (pilot)**: `unattended-upgrade --dry-run` inside CT 110 reports the
  security origin as allowed.

### A8 · CrowdSec + bouncer on the gateway — **Should**
Migrates with the platform stack; the currently-broken bouncer wiring (vault
"Open Issues") gets fixed and *proven*.
- **Manual (acceptance)**: `cscli decisions add --ip <test-ip>` → request from
  that IP is blocked at traefik; removal unblocks. This becomes the standing
  smoke test after migration.
- **Auto (later)**: scripted version of the above in the platform verify gate.

### A9 · Multi-user / RBAC — **Won't**
Single-admin system. Kept out deliberately; revisit only if that assumption
changes.

## B · Idempotency and self-healing

### B1 · Idempotent bootstrap and deploy — **Must**
Every operation converges; re-runs are no-ops when nothing changed.
- **Auto**: "second run is quiet" test — run deploy twice against MockExecutor;
  run 2 must contain zero mutating commands (only checks).
- **Manual (pilot)**: redeploy syncthing; transcript shows skips, no restarts.

### B2 · Runaway guards — **Must**
Docker log caps, journald limits, syslog rotation, weekly docker prune, apt
autoclean — applied on every deploy (already implemented).
- **Auto**: unit test — guard step produces daemon.json/journald/logrotate/
  timer content; conditional-restart logic only fires on content change.
- **Manual (pilot)**: `docker inspect syncthing` shows the 10m×3 LogConfig;
  `systemctl list-timers` shows docker-prune; `journalctl --disk-usage` ≤ 100M.

### B3 · Verify gates after every deploy — **Must**
`compose ps` running-check, restart counters, auto-collected diagnostics on
failure; "Sync complete" only after all gates pass.
- **Auto**: unit test — MockExecutor reports one app not running → deploy fails
  and the failure payload contains the diagnostic block.
- **Manual (pilot)**: break an image tag on purpose; watch the gate fail with
  logs in the focus feed, and the stack end disabled (A3).

### B4 · Drift detection — **Should**
HOST hashes applied intent per stack; TUI shows [UPD] when local files differ.
- **Auto**: unit test — hash stored on apply; modified file → drift true;
  redeploy → drift false.

### B5 · Transaction journal for destructive operations — **Should**
Provision, destroy, restore and deploy write phase-by-phase journal entries;
requirement: it covers *all* dangerous paths or it ships disabled.
- **Auto**: unit test — every destructive RPC writes begin/phase/end records;
  kill the pipeline mid-way in a test and assert the journal shows the exact
  incomplete phase.

### B6 · Rollback guard — **Should** *(round 2)*
Per-app digest history (previous/current + timestamps); rollback = compose
pinned to the previous digest. Pairs with D9 (pre-update snapshot) so "really
back" = old image + old data; image rollback alone does not revert DB
migrations — documented limitation.
- **Auto**: unit test — update records digest pair; rollback renders compose
  override with the exact previous digest.
- **Manual (pilot)**: update syncthing, roll back, verify the old digest runs.

### B7 · Systemd watchdog integration — **Should** *(renamed from heartbeat failsafe, round 2)*
The old client-heartbeat protocol is retired. Instead: `WatchdogSec=60` in the
unit + `sd_notify` pings from the daemon's main loop — a hung-but-alive HOST is
hard-restarted by systemd. ~20 lines instead of a protocol.
- **Auto**: unit test — main loop emits watchdog pings at the required cadence
  under load.
- **Manual**: SIGSTOP the daemon; systemd restarts it within the window;
  F3 notification fires.

### B8 · Golden template as bootstrap cache — **Should**
`template build` RPC produces `debian-12-homelab-vN` with docker + guards baked
in; bootstrap remains the source of truth and always runs over it.
- **Auto**: unit test — template-builder pipeline uses only its own temp vmid;
  destroy path refuses any other vmid.
- **Manual**: create a stack from the template; deploy transcript shows
  bootstrap skipping everything; create-time drops to seconds.

## C · Provisioning and lifecycle

### C1 · Declarative LXC provisioning via manifest — **Must**
`lxc-compose.yml` v2 (intent only) drives create/update of the container.
- **Auto**: unit tests — manifest → exact expected `pct create/set` argument
  vectors (golden tests); invalid manifests rejected with clear errors.
- **Manual (pilot)**: raise memory_mb, redeploy, `pct config` shows new value.

### C2 · Gated destroy — **Must** *(upgraded by Kenny from Should)*
Typed stack-name confirmation in the TUI + A2 hostname guard + B5 journal
entry; refuses no-touch vmids unconditionally.
- **Auto**: unit test — destroy with wrong typed name → refused; with hostname
  mismatch → refused; MockExecutor confirms `pct destroy` only after all gates.
- **Manual (acceptance)**: destroy a scrap test stack end-to-end; then verify
  107/111 decommissioning uses this path.

### C3 · Boot policy fleet-wide — **Must**
`onboot=1` + startup order from the manifest, enforced and verified (power-loss
rule: platform first, apps later).
- **Auto**: verify-gate test — manifest order not reflected in `pct config` →
  gate fails.
- **Manual (post-migration)**: controlled host reboot; observe ordered start
  and all services healthy without intervention.

### C4 · Hot-apply resources — **Could**
Live RAM/cores increase, shrink refused while running.

### C5 · Template selection from the live Proxmox API — **Could**
Replaces the hardcoded template string with discovery.

## D · Deployment and gitops

### D1 · Push-sync over one secured line — **Must**
CLIENT sends complete files + env over the line; HOST pushes into the LXC and
runs compose. No git or agents inside containers.
- **Auto**: integration test (host binary against MockExecutor): DeployStack
  spec → expected sequence mkdir/push/pull/up per app.
- **Auto**: path-traversal tests — file paths with `..` or absolute paths are
  refused.

### D2 · Activation gating — **Must**
New stacks are born disabled; nothing runs until explicitly enabled; A3 flips
it back off on failure.
- **Auto**: unit test — deploy of a disabled stack performs provisioning steps
  but refuses app start (or is refused entirely — design decision recorded in
  the architecture doc).

### D3 · App add/remove + garbage collection — **Must**
Removing an app from intent stops and removes its containers on next sync;
data dirs are kept until explicitly deleted.
- **Auto**: unit test — spec v2 without app X → GC issues compose down + rm for
  X only, touches nothing else, keeps `-config` dirs.
- **Manual (pilot)**: add a scrap app to syncthing stack, remove it, verify
  cleanup.

### D4 · Git history on HOST + rollback via revert — **Should**
Every deploy commits intent locally; revert + redeploy = config rollback.
- **Auto**: unit test — two deploys → two commits; secrets never in tree (A5
  test shares this); revert produces previous file content.

### D5 · GitHub mirror — **Should** *(round 2)*
Background push of the HOST intent repo (never secrets) after each commit,
with retry queue; never blocks a deploy.
- **Auto**: unit test — push failure lands in the retry queue and the deploy
  RPC still succeeds; queue drains on next success.
- **Manual**: clone the mirror on another machine; verify full history, zero
  secret content.

### D6 · Change-plan / diff preview before apply — **Should**
Terraform-style: changed lines, affected containers (UPDATE/SKIP), active
safety gates — shown before anything runs.
- **Auto**: unit test — plan output for a known state/spec pair matches golden
  file; "no changes" spec produces an all-SKIP plan.

### D7 · Preset catalog + template library — **Should** *(extended per Kenny)*
Built-in app presets AND a directory of canonical docker-compose templates
(promtail, watchtower-replacement, arr apps…) configured **once**; on install
only the per-VM values (stack name, host, IPs, paths) are substituted.
- **Auto**: golden tests — each template + a sample context renders to a valid
  compose file (`docker compose config` parses it in CI).
- **Auto**: substitution test — unresolved `{{placeholder}}` left in output →
  hard error.

### D8 · Auto-injection of core apps (promtail) — **Should**
Every new stack gets promtail from the D7 template automatically; visible and
removable.
- **Auto**: scaffold test — new stack spec contains promtail app with correct
  loki endpoint and labels.

### D9 · Managed updates with per-app policy — **Should** *(round 2)*
HOST periodically checks registries for newer digests; TUI shows update
badges; update flow = E1 snapshot → pull → restart → B3 gates → B6 digest
recorded. Per-app policy field: `manual` (default for stateful apps), `auto`,
or `auto-after-N-days`. Watchtower is not part of the new system.
- **Auto**: unit test — policy engine: auto app updates on detection, manual
  app only flags; N-days app updates only after threshold.
- **Auto**: update pipeline ordering test — snapshot strictly precedes pull.
- **Manual (pilot)**: update-badge → button → verified update → rollback.

## E · Backup and recovery

### E1 · Restic backups per stack with retention — **Must**
Per-stack encrypted repos, 7d/4w/3m retention + prune, dedup.
- **Auto**: unit test — backup pipeline emits init/backup/forget with correct
  repo path per stack.
- **Manual (pilot)**: two backup cycles; `restic snapshots` shows both; prune
  respects policy.

### E2 · Full restore flow — **Must**
Choose snapshot → quiesce → restore → restart → verify gates, as a first-class
TUI operation with focus feed.
- **Acceptance (standing)**: quarterly "restore drill" on the pilot stack:
  wipe config dir, restore snapshot, verify app healthy with old data. The
  drill itself is a scripted `homelab restore --drill` so it stays easy.

### E3 · Auto-restore on empty config during deploy — **Must** *(upgraded by Kenny)*
Fresh/empty config dirs are filled from the latest snapshot before apps start;
backup-target failure degrades to a loud warning, never a blocked deploy.
- **Auto**: unit test — empty-dir detection triggers restore step; non-empty
  skips it; restic failure → warning path, deploy continues.
- **Manual (pilot)**: destroy + recreate CT 110; syncthing returns with its
  device/folder config intact.

### E4 · Interval scheduler + quiesce labels — **Should**
Interval-based cycles with a single-cycle lock; `backup.pause=true` containers
stopped during their snapshot; per-stack serial (never fleet-wide downtime).
- **Auto**: unit test — overlapping cycle request while one runs → refused;
  labeled containers get stop/start around snapshot, unlabeled don't.

### E5 · Offsite to Google Drive — **Must** *(upgraded by Kenny)*
Restic repos synced via rclone with a fresh OAuth token; layered above local
backups, never the only copy.
- **Auto**: config check — backup pipeline warns loudly when the offsite
  remote is unconfigured/expired (this is also the F3 notification trigger).
- **Manual**: restore one file from the Drive copy on a machine that never saw
  the local repo (proves the offsite chain end-to-end).

### E6 · PBS / vzdump whole-container safety net — **Could**
Separate infrastructure project (vault §1); tracked there, not in this codebase.

## F · Observability

### F1 · Promtail → Loki fleet-wide — **Must**
Existing pipeline; every stack ships docker + system logs labeled per stack.
- **Manual (pilot)**: Loki label query shows `stack=syncthing` within minutes
  of first deploy.
- **Auto (post-migration)**: verify gate queries Loki for recent lines from the
  deployed stack.

### F2 · Live log streaming in the TUI — **Must**
WS stream with source filter, scrollback, and per-task focus feeds.
- **Auto**: protocol test — log broadcast reaches a connected client within N
  ms in an integration test with a fake HOST.
- **Manual**: mockup behaviors (filter, anchor-scroll, focus feeds) reproduced
  against the real stream.

### F3 · Notifications via the HA dispatcher — **Should**
Webhook to Kenny's existing HA notification system (ack flow included) for:
backup failed, deploy failed, crash-loop, offsite-token expired.
- **Auto**: unit test — each trigger produces exactly one webhook payload
  (golden JSON), with dedup/backoff on repeats.
- **Manual**: force a failing backup; phone notification arrives via HA.

### F4 · Metrics stack (prometheus/cadvisor/pve-exporter) — **Could**
Becomes a regular stack via D7 after migration (vault §2).

### F5 · Status/health API on HOST — **Should**
`/api/health`, version, host metrics for TUI panels and uptime-kuma.
- **Auto**: integration test — endpoints respond without auth for health, with
  auth for metrics; uptime-kuma monitor added after pilot (manual).

## G · UX and TUI

### G1 · Cyberpunk TUI with effects and focus modes — **Must**
Ground-up rebuild on the real protocol; splash, fx engine with intensity
levels, dashboard, focus windows (mockup approved as design base).
- **Manual (design)**: mockup parity checklist per tab at 80×24 and full size.
- **Auto**: `cargo test` snapshot tests on render output (ratatui TestBackend)
  for each screen with a fixed world state and fx off.

### G2 · Wizards for stack and app management — **Must**
Form-style flows (arrow-key fields, free disk entry), review step with derived
values; wizard writes the same files a human could write by hand.
- **Auto**: wizard state-machine unit tests — every path produces a valid
  manifest (validated by the same validator the host uses).

### G3 · Command palette — **Should**
Fuzzy access to every action.
- **Auto**: registry test — every TUI action is reachable via palette (no
  orphaned actions).

### G4 · Remote shell tab (PTY via HOST) — **Could**
Depends on A6; spec exists in the old project.

### G5 · Maintenance window mode — **Won't**
Hidden global state; individual pause switches already cover the need.

### G6 · Visual data transfers — **Should** *(confirmed round 2)*
File/byte movement rendered as animated cyberpunk transfer streams: pct push,
backup/restore bytes, image pulls — progress with particle/flow effects in the
focus windows and a transfers panel. Real numbers drive the animation (no fake
progress).
- **Auto**: transfer events carry byte counts in the protocol; renderer
  snapshot test with a scripted transfer.
- **Manual**: pilot deploy shows live transfer viz matching actual sizes.

## H · Network and hardware

### H1 · Traefik route fragments via file-provider watch — **Must**
One YAML fragment per stack, pushed to the gateway's watched dir; no restarts.
- **Auto**: unit test — fragment filename/destination constrained to the
  routes dir on vmid 104 only (path-escape attempts refused).
- **Manual (pilot)**: `curl -H 'Host: sync.kp-soft.dev' http://10.10.10.4`
  routes to syncthing within seconds of deploy.

### H2 · OPNsense Kea DHCP reservations — **Could**
Static IPs in manifests suffice; old project has reference code.

### H3 · Cloudflare DNS automation — **Won't** *(round 2)*
Resolved without software: the wildcard record `*.kp-soft.dev → tunnel` is
**already configured** (confirmed by Kenny) — every future hostname exists the
moment its traefik fragment (H1) lands. No API token, no code, no maintenance.

### H4 · Hardware passthrough at migration (GPU, TUN, NAS mounts) — **Must**
Manifest flags (`gpu: true`, `vpn: true`, NAS mounts) drive udev/cgroup/mount
config — with the ansible chmod-0777-recurse bug fixed (targeted, non-recursive
permissions).
- **Auto**: unit test — flags → exact expected pct raw-config/udev content.
- **Manual (migration acceptance)**: jellyfin VAAPI transcode works; gluetun
  gets its TUN; NAS paths writable — each a scripted check in the migration
  runbook.

### H5 · HOST self-update with rollback watchdog — **Must** *(upgraded by Kenny)*
GitHub-Release driven: preflight (`--version` + link check), keep previous
binary, swap, watchdog reverts within 35s if the new one isn't healthy.
- **Auto**: unit tests on the preflight/swap/rollback state machine with a fake
  release + fake health signal (this code exists in the old project with the
  same seams).
- **Manual**: publish a deliberately broken release to a test tag; watch the
  watchdog roll back and report via F3.

### H6 · Fleet OS patching on command — **Should**
One TUI action: serial `apt full-upgrade` across managed LXCs with per-CT
result + reboot indicator.
- **Auto**: unit test — patch pipeline iterates exactly the managed set,
  serial, aborts the sequence on first failure (fail-closed).
- **Manual**: run against pilot + one scrap stack; verify serial order.

## I · Round-3 additions (proposed by Claude, rated by Kenny 2026-08-10)

### C6 · Capacity overview — **Should**
Committed vs available host resources computed from manifests + live host
data; wizard warns when a new stack would overcommit (the "9 GB headroom"
concern from the vault, live).
- **Auto**: unit test — commitment sum over a manifest set matches golden
  value; wizard warning triggers at the threshold.

### D10 · Pre-flight validation — **Must** *(upgraded by Kenny)*
Formal schema validation of manifests and compose files, client-side (instant
wizard feedback) AND host-side (never trust the client), with
`docker compose config` as the compose gatekeeper before anything runs.
- **Auto**: fixture suite of invalid manifests/compose files → each rejected
  with the expected, human-readable error (golden messages).
- **Auto**: client and host share the same validator crate — a divergence test
  ensures one implementation, two call sites.

### D11 · Stack export/import bundles — **Could**
Share a stack (manifest + compose + presets, never secrets) as a single file;
import runs through the wizard with per-VM substitution (D7 mechanics).
- **Auto**: round-trip test — export → import produces an equivalent stack;
  secret-exclusion test on the bundle content.

### E7 · DR-runbook generator — **Must** *(upgraded by Kenny)*
Generated, always-current total-loss rebuild document: hardware, step order,
backup/mirror locations, required secrets and where they come from. Refreshed
automatically after every change; cannot go stale because it renders from real
state.
- **Auto**: generator snapshot test against a fixture state; CI fails if a new
  feature adds state the runbook template doesn't cover (coverage check).
- **Manual (standing)**: yearly tabletop drill — follow the generated runbook
  on paper and log every gap found.

### F6 · Doctor / self-diagnosis — **Should**
`homelab doctor` + TUI panel: link/TLS/token health, daemon version, host disk,
state-vs-reality consistency, orphaned containers, backup freshness per stack,
offsite token validity, mirror lag — all green or an actionable hint.
- **Auto**: each check is a pure function over injected probes → unit-test
  matrix of healthy/broken permutations.
- **Manual**: break one thing on purpose (expire token) and verify doctor
  pinpoints it.

### G7 · Demo mode — **Won't**
No user-facing `--demo` flag. The simulator lives on only as an internal test
fixture for G1's TUI snapshot tests — implementation detail, not a feature.

### C7 · Native Rust services under systemd — **Should** *(added 2026-08-28, Kenny's route "C+")*
Kenny's own Rust services (mailbox, almanac, …) run as bare binaries under
systemd in their own LXC — no docker layer — with the homelab as the safety
net their self-update cannot be: before the app runs `<tool> update`, the
previous binary is preserved and a rollback is armed (the H5 pattern); a new
version that fails to come up healthy is rolled back from OUTSIDE the app.
Includes **adoption**: taking over an existing hand-built container (CT 109,
`109-app-mailbox`, is the first) — verify unit/binary/EnvironmentFile/data
layout, record it in state, enable backups and update supervision, without
restarting the service. Ships with `docs/LLM_SERVICE_ADOPTION.md` (sister to
LLM_COMPOSE_CONVERSION.md) so a future LLM session can run an adoption
end-to-end.
- **Auto**: install/adopt/update ops against the MockExecutor incl. the
  refusal paths (wrong unit, missing EnvironmentFile, data outside /appdata);
  rollback-armed update flow.
- **Manual**: adopt CT 109 live; then a deliberately broken mailbox release
  that self-updates and is rolled back by the homelab — the same drill H5
  passed.

### D12 · Secrets from latch instead of plaintext files — **Should** *(added 2026-08-28)*
The client no longer requires `stacks/<stack>/<app>/.env` as a plaintext file
on the workstation. The manifest names each app's secret source; at deploy
time the client obtains the decrypted content per app file by invoking
`latch cat` (a latch-side machine interface, requested from the latch
project) and composes the .env payload in memory — wire and host-vault
behaviour unchanged (0600 vault on the host stays, D11 bundles still never
carry secrets). Missing latch/file/key = a hard preflight error (D10) naming
the remedy; the host vault still holds the previous values as fallback.
Per-file sourcing sidesteps latch run's cross-file merge semantics entirely.
- **Auto**: composition + mapping unit tests; plaintext-scan asserts no
  secret content is written to the workstation disk on any path; preflight
  refusal tests.
- **Manual**: one deploy of the mailbox stack end-to-end with latch as the
  only secret source.

### E8 · ZFS snapshots + replication — **Should** *(added 2026-08-27)*
Absorbs `/root/full_zfs_backup.sh` into the orchestrator (Kenny: "alles vanuit
1 systeem ipv een systeem dat ik ga vergeten"). Declared jobs in host.toml
(`[[zfs_jobs]]` source/target), recursive snapshots named `homelab-<stamp>`,
incremental `send -RI` when a common base exists, retention through the same
tiered engine as restic (G8), runs in the nightly plan, reports over the
existing webhook/incident chain. CLI: `homelab zfs-replicate`.
Hard refusal where the old script powered through: no common snapshot + a
non-empty target = stop and ask, never destroy-and-reseed. An empty job list
is an error, not a silent success — that was the old script's actual failure
mode (it iterated over dataset names that no longer existed).
- **Auto**: job validation, common-base selection, snapshot-name parsing and
  date maths are pure functions; mock-executor tests assert that the refusal
  path issues neither `zfs destroy` nor `zfs receive`.
- **Manual**: live run against the real pools, verify snapshots on both sides
  and that a broken chain refuses instead of re-seeding.

### G9 · Own Rust services via GHCR images — **Should** *(added 2026-08-12)*
Pattern + tooling, zero orchestrator code: `templates/rust-service/`
(Dockerfile + release-image.yml) makes any Rust repo publish a GHCR image
next to each GitHub release; `presets/rust-service/` is a copyable example
stack (own service + RabbitMQ). The service then rides every existing
mechanism: deploy, nightly auto-update with rollback, backups, parking.
- **Auto**: preset loads via the existing scan_presets path (data, not code).
- **Manual**: first real service (e.g. RabbitDispatcher) end-to-end: tag →
  image → wizard → deploy → auto-update.

### H7 · Release-driven host updates — **Should** *(added 2026-08-12, evaluation form B6)*
The client checks GitHub for the newest release at TUI startup; a ticker badge
announces a newer host version, `U` opens a progress window that downloads the
release via authenticated `gh`, verifies the SHA256SUMS entry, and feeds the
binary into the existing H5 self-update pipeline (selfcheck, armed rollback,
watchdog). CLI: `homelab release-update [tag]`. Publishing a release never
touches the host — rollout is always a deliberate client action.
- **Auto**: version-compare + checksum-listing unit tests (fail-safe: malformed
  versions are never "newer"); badge + U-key snapshot test.
- **Manual**: publish a release, watch the badge appear, update, watch the
  failsafes.

### H8 · Per-stack enabled flag, light variant — **Should** *(added 2026-08-12, evaluation form B8)*
`homelab enable|disable <stack>` (TUI: `E`, `[OFF]` badge). Disabled = the
nightly scheduler skips the stack AND onboot is cleared (parking survives a
host reboot — the one thing a manual `pct stop` can't give). The flag NEVER
starts or stops containers: manual Proxmox actions are always respected, and
the two mechanisms are deliberately independent. A failed nightly run
auto-disables the stack — one loud message instead of a failure every night;
auto-disable is state-only (onboot untouched) so a transient failure can never
keep a stack from surviving a reboot. The flag persists across redeploys.
- **Auto**: op tests (disable clears onboot, enable restores it, no-touch
  refused), old-state compatibility test (missing flag = enabled).
- **Manual**: disable, verify the nightly skip line + onboot 0, re-enable.

---

**Feature phase closed 2026-08-10.** Final tally: 27 Must · 23 Should ·
7 Could · 5 Won't. Any future feature gets the next free ID in its domain and
a registry entry here before implementation.
