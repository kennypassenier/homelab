# homelab-admin — scope (Phase 0, DRAFT)

Status: Phase 0 in progress (2026-09-28). Goals and non-goals answered (part 1 + follow-up); success criteria and constraints (part 2) open. Route: **full**
(Kenny, 2026-09-28, "Beheerdashboard richting": route = Volledig). Every
statement below is one form item; nothing here is agreed until the form says so.

Analysis this scope rests on: artifact "Homelab-beheerdashboard"
(https://claude.ai/artifact/MFafAzdKMHDFM1jWM8h3wU), measured 2026-09-28.

Decisions already taken (2026-09-28, two forms):
- runs as its own container (stack), the host daemon stays on pve;
- stack edits land in a working copy in that container and are pushed to GitHub;
- base: chassis-rs, extended with two new optional features `webapp` and `live`
  (built in chassis-rs, off by default, not compiled into services that do not
  ask for them);
- frontend: plain HTML + JavaScript (ES modules, no build step) with kp-themes'
  framework-free CSS and JS;
- reachable from the home network only;
- full procedure route.

## Goals

- **G1 · One place to manage the fleet.** A browser dashboard that shows
  everything the CLI and TUI can read (fleet, per-stack detail, findings,
  doctor, backups and second copies, restore drills, manual checks,
  incidents, images and pins, patch state, host settings) and can start every
  non-destructive operation the TUI can. The TUI stays for running things.
- **G2 · Settings and firewall editable in the browser.** Per-stack settings
  that live in stack files (firewall rules first, then resources, mounts,
  routes, image versions) are edited in forms, validated with the same
  `homelab-core` code the deploy uses, written as a commit and deployed.
- **G3 · Its own stack.** `stacks/admin` on a free vmid (120), deployed,
  backed up and rebuilt by homelab like every other stack. Bootstrap: install
  the host as today, then `homelab deploy stacks/admin`.
- **G4 · The host learns to tell.** Structured (JSON) replies for fleet check,
  doctor, incidents, manual checks, backups/snapshots; real status in
  `GetState` (running, restarts, cpu, ram instead of fixed values); request
  ids on log lines; all host.toml settings readable and editable with the
  start-up validation. The CLI and TUI use the same replies.
- **G5 · Scoped tokens.** The host accepts one token per machine with a scope
  (read / operate / all) and refuses what lies outside it. The dashboard gets
  **all** (Kenny, 2026-09-28, scope follow-up: "Dashboard krijgt alles"):
  destructive actions from the browser ask for the typed name, as the CLI does.
- **G6 · Secrets shown and edited in the browser** (Kenny, 2026-09-28, scope
  follow-up: "Tonen en bewerken"; replaces the draft non-goal N1). A value is
  shown on click and only for a while, like the kyu dashboard; an edit is
  written to latch from the dashboard container (its own latch credential in
  CT 120) and deployed. Every reveal and edit lands in the host's audit log.
- **G7 · The TUI moves into the dashboard** (Kenny, 2026-09-28, scope
  follow-up, own answer). What the dashboard can do is taken out of the TUI;
  the TUI is not developed further. Last step of the project: an emergency
  path that can only install the dashboard on a fresh Proxmox (bootstrap),
  for when CT 120 does not exist yet.

## Non-goals

- **N2** No access from outside the home network in this project (no
  Cloudflare route); that waits for the Cloudflare Access split.
- **N3** The CLI is not replaced; it keeps working when the dashboard is down.
- **N4** No users and roles beyond token scopes; one operator (Kenny).
- **N5** No metrics or log storage of its own: container logs and graphs come
  from Loki/Prometheus on CT 113 (queried or linked).
- **N6** The dashboard never touches pve itself: every operation goes through
  the host daemon, which stays the only actor on the hypervisor.

## Recorded for a later round (not in this scope)

- **Renumbering the containers by criticality** (Kenny, 2026-09-28): once the
  dashboard is done, reconsider the vmid layout so the most critical
  infrastructure (the dashboard, then the gateway) gets the lowest numbers and
  less critical containers higher ones. Note for that round: VM 100 (OPNsense)
  and VM 101 (Home Assistant) hold 100 and 101 and are on the no-touch list.

## Success criteria

- **S1** Every read the CLI/TUI offer today is visible in the dashboard
  (checked against the 48-verb table).
- **S2** A firewall rule added in the browser ends as a commit on GitHub, a
  deploy, and a `homelab check` without drift — measured once end to end.
- **S3** kyu-runner's release binary is the same size after the chassis-rs
  release that adds `webapp`/`live` (features off = not compiled).
- **S4** With the admin container stopped, the CLI and the nightly round work
  unchanged (measured by stopping it).

## Constraints

- **C1** Code in the homelab repository as a new crate `admin` (reuses
  `homelab-core`), released with the homelab release; the chassis-rs features
  are built and released in chassis-rs first.
- **C2** Every rollout to a container goes through Homelab Rust.
- **C3** Code, comments and the dashboard's own UI text in English (Kenny's
  rule: dashboards English, Dutch only for the parents' dashboards).
