# Scope — The Homelab Deployment Project

Phase 0 output. **Approved via the Phase 0 gate form on 2026-08-30** — every
statement below reflects Kenny's actual answer, not the draft. Frozen except
through a mini-round (`FORM_PROTOCOL.md` §5).

Feature IDs are assigned in Phase 2; the G/N/C/S markers here are scope
statements, not features.

## Why this project exists

Homelab Rust (the orchestrator) is built, released, and live on the Proxmox
host — but it manages six containers out of thirteen, and the seven it does
not manage are the ones that matter most. Meanwhile several pieces of Kenny's
own software finished their own procedures and are waiting for a home. And a
monitoring stack that works was built directly on the running machines, where
the next deploy from this repo would erase it.

This project is where all of that converges: one inventory, one desired end
state, one proven backup, then container-by-container integration until the
whole fleet is described in this repo and managed by this orchestrator.

## Goals (G)

- **G1 · One complete inventory first.** Every LXC and VM, every service
  running inside it, every configuration that was hand-tuned and would hurt
  to lose. Written down before anything is decided.
- **G2 · A desired end state, grouped by function.** Services that belong
  together share a container (arr-suite + Jellyfin; gluetun + qBittorrent;
  kyu + kyu-runner); services that must not take each other down are
  separated. The grouping is decided before anything moves.
- **G3 · The homeless services get a home.** kyu-runner and HTTPSwitchboard
  are released and deployed nowhere; both have a handover note in the vault
  that explicitly waits on this project. Almanac (the Google Calendar
  gateway) is added here by Kenny — it already runs on CT 112 as an adopted
  native service, so its work is placement within the end state, not a first
  deployment.
- **G4 · Native Rust binaries as a full deployment path.** C7 today only
  *adopts* a container someone built by hand. The end state needs the
  orchestrator to create a container and install a Rust service into it, the
  same way it does for a docker stack.
  ↳ *C7 = native-service adoption: the orchestrator supervises a systemd
  service on a container it did not build, and never creates or destroys it.*
- **G5 · One custom golden LXC image.** Docker, unattended-upgrades,
  node_exporter, cadvisor and promtail baked in, so a new container starts
  from a known shape instead of a bootstrap script. Keyring support for latch
  is a Phase 2 feature question, not assumed here.
- **G6 · Everything that runs is in this repo.** The live drift closes:
  Alertmanager, cadvisor on six hosts, the Grafana datasource and three
  dashboards, the node/cadvisor/almanac scrape jobs and the SMART collector
  all exist on the machines and in no repository.
- **G7 · A proven backup before integration starts.** Everything to the HDDs
  attached to the server, plus a completed Google Drive run, with a restore
  actually exercised. No container is integrated before this is green.
- **G8 · The alerting loop closed and the dashboards fed.** Alertmanager
  fires today into a `none` receiver; with HTTPSwitchboard deployed an alert
  reaches Home Assistant as a notification. Grafana's dashboards come from
  the repo, not from a database row.

## Non-goals (N)

- **N1 · VM 100, CT 102 and CT 103 are never touched.** OPNsense (the
  router), the Omada controller and the fileserver. No deploy, no adoption,
  no exec, not read-write, under no circumstance. This outranks every other
  statement in this document.
- **N2 · No rebuilding of what already works elsewhere.** The Home Assistant
  notification dispatcher, the pipeline-v2 chain, kyu's own retry machinery:
  this project connects to them, it does not reimplement them.
- **N3 · newsflash and latch get no container.** newsflash is a desktop
  client on Kenny's own PC; latch is a CLI the orchestrator itself calls.
  They stay in scope only as consumer and dependency.
- **N4 · Media content is out of backup scope.** Films and series on the
  12 TB and 18 TB pools are not backed up. Their *configuration* is.
- **N5 · No orchestrator features beyond what the rollout needs.** Ideas
  that surface during the work are queued as mini-rounds, not built on the
  way past.

## Hard constraints (C)

- **C1 · The order is fixed and each step gates the next.** Inventory →
  grouping → capture the existing configuration → build the end state →
  full backup → integrate container by container, least important first.
  Inside the rollout: anything behind the edge needs cloudflared and Traefik
  healthy first, anything monitored needs Prometheus, anything alerting
  needs HTTPSwitchboard.
- **C2 · A fresh deploy must reproduce every coupling we made by hand.**
  Widened by Kenny at the gate, and this is the sharpest requirement in the
  document. It is not only Jellyfin's hardware transcoding and qBittorrent
  behind gluetun: Grafana dashboards are created automatically, promtail and
  Loki point at the right addresses, and every service-to-service link keeps
  working after a deploy as if nothing had changed. Adding a new stack in
  the future must integrate itself into those services — a new Grafana
  dashboard where one is needed, an addition to an existing dashboard where
  that fits. The target is to express all of it as config and compose files,
  not as remembered steps.
- **C3 · Inter-service configuration uses LXC IP addresses, not the public
  URLs.** Kenny's rule at the gate: services address each other at
  `10.10.10.x`, not at `something.kp-soft.dev`, so an internet outage or a
  Traefik problem cannot take the internal wiring down with it. The public
  URL is used only where nothing else works. This is independent of C4.
- **C4 · Most services stay reachable from the internet.** Traefik keeps
  doing what it does today; C3 changes internal wiring only, never external
  reachability.
- **C5 · A problem in Kenny's own software leaves this session.** It is
  written up in full and routed to that project's own conversation in the
  Projects group (created if absent); this project may mark itself BLOCKED BY
  that project and end its turn with a standing Unblock form.
- **C6 · The work must survive a context loss.** Every finding, task and
  decision gets a number and lives in `docs/deployment/REGISTER.md` in this
  repo, not in the conversation. Updating it is part of doing the work.
- **C7 · The safety list is now exactly four guests, and it is code.**
  `core/src/safety.rs::DEFAULT_NO_TOUCH` was narrowed on 2026-08-30 from
  `100-107, 111, 201-203` to `100, 101, 102, 103`, on Kenny's instruction at
  this gate ("wat ik nu zeg is heilig"). Every other LXC comes under
  orchestrator management as this project integrates it. A pinning test
  (`a1_no_touch_list_is_exactly_the_four_untouchable_guests`) fails if anyone
  widens or narrows the list silently.
  Removing a vmid from the list does **not** by itself make it deployable:
  A2 refuses any container whose hostname is not the canonical
  `<vmid>-app-<stack>`, and every legacy stack (`lxc-media-stack` and
  siblings) fails that check until it is deliberately renamed.
- **C8 · VM 101 (Home Assistant) is not managed like the others.** Its VM
  lifecycle stays untouchable — it remains on the no-touch list. What this
  project *will* do is change configuration inside Home Assistant itself,
  mostly automations, and mostly towards the end of the project when the
  notification system is rebuilt around newsflash and kyu. **Every such
  change needs Kenny's explicit permission, per change.**

## Decisions taken at the gate

- **CT 107** (`lxc-mqtt-stack`, 10.10.10.7) runs nothing but sshd and the
  metrics agent — no docker, no mosquitto. It gets cleaned up.
- **CT 111** (`lxc-productivity-stack`, 10.10.10.11) runs Vikunja and
  SuperSync with Postgres. It is kept and integrated.
- **CT 190 and CT 191**, the kyu/kyu-runner scratch containers holding
  10.10.10.14 and .15, are cleaned up after coordinating with the
  notification-pipeline-v2 project, which shares 191.
- **This project's documents** live in `docs/deployment/` in this repo, so
  the repo's own gates and commit hooks run over them.

## Success criteria (S)

- **S1** Every container except the untouchable ones is described in this
  repo, appears in `homelab state`, and its rebuild-from-zero has been
  drilled at least once.
- **S2** No service regressed: every endpoint in the vault's "Home Network
  Services" answers as before, and Jellyfin still transcodes in hardware.
- **S3** A deliberately triggered alert arrives as a Home Assistant
  notification, end to end.
- **S4** A restore drill is green on both backup targets.
- **S5** A deploy of any stack changes nothing that is already live — repo
  and running fleet are identical.
