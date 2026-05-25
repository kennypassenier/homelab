

# Tier 3: LXC (GitOps Engine in Docker)

The LXC daemon is a Rust application packaged as a Docker container, running alongside your apps inside every Proxmox container. It replaces `node-sync.sh` and `container.sh` ([1, 5, 6]).

## 1. Premium UI/UX (Ratatui)
- [ ] **Hyper-Modern Interface:** Just like the Client and Host applications, the Ratatui interface for this LXC Daemon MUST be highly polished and visually stunning when accessed via SSH.
- [ ] **Styling & Feedback:** Implements a centralized styling module with dynamic colors (Cyan/Magenta accents). Active tabs must be highlighted, background sync states must use animated spinners, and error logs must stand out in high-contrast Red. Modals must render as floating, centered pop-ups with a shadow effect.

## 2. GitOps Engine & Fallback
- [ ] **Sparse Checkouts:** Autonomously fetches only the configuration for its specific stack using Git Sparse-Checkout, discarding any manual local changes ([7]).
- [ ] **Axum API & File-Locks:** Runs an Axum web server to receive HTTP Push triggers from the CLIENT. It also exposes `/api/backup/pause` and `/api/backup/resume` endpoints for the HOST backup orchestrator. To prevent race conditions with the fallback 30-minute cron job, it uses `fs2` file locks.

## 3. Container Orchestration & Security
- [ ] **Bollard API Integration:** Communicates directly with `/var/run/docker.sock` to pull images, stop, and start containers.
- [ ] **Pre-Deploy Hooks (`setup.sh`):** Replaces `pre-sync.sh` ([8]). Before touching Docker, it checks for `hooks/setup.sh` (used for creating shared Traefik networks). It executes this hook via `tokio::process::Command` and aborts deployment if it fails. Legacy one-time migration scripts are strictly forbidden here.
- [ ] **Ephemeral Secrets Container:** Replaces Infisical ([9]). To fetch secrets, it spins up a short-lived Docker container that pulls secrets, writes the `.env` file, and exits. If this container crashes, the deployment halts (Fail-Closed) to ensure apps never boot without secrets.
- [ ] **Atomic Mount Validation:** To prevent data loss if a Proxmox bind-mount fails, it compares the Linux `st_dev` (device ID) of the `/docker` directory and the persistent `/config` directory. If they match, the mount failed, and the daemon refuses to start the application.

## 4. Resilience, Telemetry & Garbage Collection
- [ ] **Fail-Safe Rollbacks:** After starting a container, it monitors the Docker API. If the container crashes within 10 seconds, it automatically rolls back to the previous known-good Image IDs.
- [ ] **Webhook Alerts:** If a deployment fails or a rollback occurs during a cron cycle, it sends an HTTP POST alert to Ntfy or Discord using `reqwest`, specifying the exact stack that failed.
- [ ] **Structured Logging (logfmt):** Emits structured `logfmt` lines (`ts=... level=... stack=... app=... msg=...`) so Promtail can ship them to Loki, ensuring the universal Grafana logs dashboard continues to work without maintenance.
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
| container.sh           | Main TUI entrypoint, manual sync trigger | [ ]    |
| node-sync.sh           | GitOps engine, fallback cron, logfmt     | [ ]    |
| pre-sync.sh            | Pre-deploy hooks (`setup.sh`)            | [ ]    |

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
