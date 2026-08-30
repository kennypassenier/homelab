# Tech choices — The Homelab Deployment Project

Phase 3 draft. **Not approved.** Each item below is a gate-form item.

## What is inherited, and therefore not re-decided

This project extends an existing system rather than starting one, so almost
every Phase-3 question was answered by the orchestrator's own Phase 3 and is
not reopened here: Rust 2021 with MSRV 1.87, the core/proto/host/client
workspace split, serde + tokio + axum + rustls, docker compose inside LXCs,
restic over rclone for backups, Prometheus + Loki + Grafana for observability,
Traefik behind a Cloudflare tunnel at the edge. Reopening any of them is a
mini-round on `docs/ARCHITECTURE_DECISIONS.md`, not a decision here.

What follows is only what this project genuinely adds.

## T1 · How Prometheus learns about a new target

Today eleven node addresses and six cadvisor addresses are hardcoded in
`prometheus.yml`. O3 requires a new stack to be scraped without anyone
editing that list.
↳ *O3 = "a new stack registers itself with the observability services",
rated Onmisbaar.*

- **A · file-based discovery.** The orchestrator writes
  `/appdata/observability/targets/<stack>.json` on deploy and deletes it on
  destroy; Prometheus watches the directory with `file_sd_configs` and picks
  changes up without a reload.
- **B · keep a static list**, regenerated from the orchestrator's state each
  deploy and pushed as a whole file. Simpler to read, but every deploy
  rewrites the config of a service it does not own.
- **C · HTTP discovery**, with the host daemon serving a targets endpoint.
  One more moving part, and Prometheus then depends on the orchestrator being
  up to know what to scrape.

## T2 · How a stack gets a Grafana dashboard

- **A · generated provisioning files.** One dashboard JSON per stack,
  rendered from a template at deploy time into Grafana's provisioning
  directory, which Grafana reloads every ten seconds. Fits the existing
  provisioning setup and keeps dashboards in the repo, not in a database.
- **B · one fleet dashboard with a stack variable.** Nothing to generate; a
  new stack simply appears in a dropdown. Loses per-stack panels that differ
  (Jellyfin's transcode counters are not qBittorrent's).
- **C · hand-written per stack.** Honest, and exactly what left the current
  dashboards out of every repository.

## T3 · Where update behaviour is declared

O9 (clean shutdown before update) and O10 (never during a Jellyfin stream)
need a per-app place to say so.

- **A · docker labels**, matching the existing `com.homelab.backup.pause` and
  `com.homelab.update.policy`. One vocabulary, visible in `docker inspect`,
  and it travels with the compose file.
- **B · fields in `lxc-compose.yml`.** Typed and validated at deploy time, but
  a second place to look and invisible from the container itself.

## T4 · Where the fleet check runs

- **A · both.** The check is a pure function in `core`, invoked by a client
  command for on-demand use and by the host's nightly scheduler for the
  unattended pass, reporting through the same notification path as a backup.
- **B · client only.** Nothing runs unless Kenny runs it — which is precisely
  how the current drift accumulated unseen.
- **C · host only.** Unattended, but no way to ask "is it right now?"

## T5 · A native stack with more than one service

`StackState.native` is a single `Option<NativeServiceManifest>`: a native
stack holds exactly one service. The target layout puts three native services
on one container (kyu, kyu-runner, http-switchboard), so this has to give.

- **A · make `native` a list.** One stack, several units, one backup repo, one
  lifecycle. Matches how the docker side already treats `apps`.
- **B · one stack per service, several stacks per container.** No schema
  change, but the whole model assumes one stack owns its container — its
  hostname, its safety check, its backup repo are all per stack.

## T6 · Toolchain and versions

No change: `rust-toolchain.toml` pins the version CI asks for, MSRV stays
1.87, and the workspace version continues to move as one number.
