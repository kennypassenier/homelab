


# Tier 3: LXC (GitOps Engine in Docker)

The LXC daemon is a Rust application packaged as a Docker container, running alongside your apps inside every Proxmox container. **node-sync.sh and container.sh are fully deprecated and must not be used.** All sync, management, and deployment logic is now implemented in Rust, with the daemon running an Axum web server for HTTP Push APIs and a fallback 30-minute tokio interval loop for eventual consistency.

**Communication model:** The LXC daemon only responds to requests from the CLIENT. It does **not** call the HOST daemon directly. When an LXC operation depends on a prior HOST step (e.g., appdata directories being created before first sync), the CLIENT orchestrates the sequence: it waits for the HOST to confirm completion, then issues the follow-up call to the LXC.

## 1. Plain Output (No Ratatui)
- [ ] **Rudimentary Logging Only:** The LXC daemon emits plain `stdout`/`stderr` log lines in logfmt format. There is no Ratatui TUI. All interactive management and visual feedback lives exclusively in the CLIENT.
- [ ] **Structured Logfmt:** Every event follows `ts=<ISO8601> level=<info|warn|error> component=lxc stack=<name> app=<name> msg="..."`. Log lines are the sole output surface.
- [x] **SSE Stream:** All log events are broadcast via `GET /api/logs/stream` (SSE) so the CLIENT can render them live in the per-stack deployment modal.

- [x] **Sparse Checkouts:** Autonomously fetches only the configuration for its specific stack using Git Sparse-Checkout, discarding any manual local changes ([7]). Initialised on first boot via `GITOPS_REPO_URL` env var; falls back gracefully if already cloned.
- [x] **Axum API & File-Locks:** Axum web server on `:8080` handling `POST /api/sync`, `POST /api/backup/pause`, `POST /api/backup/resume`, and `GET /api/logs/stream` (SSE). Runs concurrently with TUI via tokio. `/tmp/gitops.lock` prevents concurrent syncs.

- [x] **Bollard API Integration:** Polls `/var/run/docker.sock` every 5s via `bollard` crate. Lists all containers with name, image, state, ports, uptime. Displayed live in Containers and Dashboard tabs.
- [x] **Pre-Deploy Hooks (`setup.sh`):** Replaces `pre-sync.sh`. `pre-sync.sh` is no longer used for directory creation or secrets. `stacks/<stack>/setup.sh` runs before `docker compose up` if present. Legacy one-time migration scripts are forbidden.
- [ ] **Ephemeral Secrets Container:** Infisical (and its CLI) is entirely removed. To fetch secrets, the LXC daemon spins up a short-lived Docker container that pulls secrets, writes the `.env` file, and exits. If this container crashes, the deployment halts (Fail-Closed) to ensure apps never boot without secrets.
- [x] **Atomic Mount Validation:** Compares `st_dev` of `/docker` and `/config` against root `/` every 10s via `std::os::unix::fs::MetadataExt`. Mismatched device IDs indicate a real bind mount; matching IDs trigger a WARN log and show red status in the Secrets tab.

## 4. Resilience, Telemetry & Garbage Collection
- [ ] **Fail-Safe Rollbacks:** After starting a container, it monitors the Docker API. If the container crashes within 10 seconds, it automatically rolls back to the previous known-good Image IDs.
- [ ] **Webhook Alerts:** If a deployment fails or a rollback occurs during a cron cycle, it sends an HTTP POST alert to Ntfy or Discord using `reqwest`, specifying the exact stack that failed.
- [x] **Structured Logging (logfmt):** Emits structured `logfmt` lines (`ts=... level=... stack=... app=... msg=...`) so Promtail can ship them to Loki, ensuring the universal Grafana logs dashboard continues to work without maintenance. All log lines are also broadcast over the SSE endpoint.
- [ ] **Garbage Collection & Force-Deletion:** If an app folder is removed from Git, it deletes orphaned containers and images ([10]). However, actual persistent data on the host mount (`/opt/appdata`) ([11]) is only deleted if the API trigger specifically contains the `force_deletion=true` token.

## 5. Updates & Maintenance
- [ ] When changes are pushed to the `lxc-daemon/` folder, GitHub Actions builds the Docker image and pushes it to the GitHub Container Registry (GHCR).
- [ ] Existing **Watchtower** instances inside the stacks ([12], [13]) detect the new GHCR image and automatically perform a self-update of the LXC daemon without manual intervention.
- [ ] **Traefik + CrowdSec:** Seamlessly deploys Traefik as the reverse proxy (via GitOps labels) and integrates the official Traefik CrowdSec Bouncer middleware for L7 intrusion prevention.

---

**Legend:**
- [x] = Complete
- [ ] = Not yet implemented or not fully integrated

---

## Implementation Details & Mapping

| Old Bash Script         | LXC Rust Feature/Module                  | Status |
|------------------------|------------------------------------------|--------|
| container.sh           | Main TUI entrypoint, manual sync trigger | [x]    |
| node-sync.sh           | GitOps engine, fallback cron, logfmt     | [x]    |
| pre-sync.sh            | Pre-deploy hooks (`setup.sh`)            | [x]    |

---

## References

27. node-sync.sh (legacy entrypoint)
28. container.sh
29. scripts/container/
30. Git Sparse-Checkout
31. scripts/container/pre-sync.sh
32. Infisical (legacy secrets)
33. Orphaned container/image GC
34. /opt/appdata persistent data
35. Watchtower auto-update
36. Traefik + CrowdSec integration

See `refactor/refactor-features.md` for full requirements.
