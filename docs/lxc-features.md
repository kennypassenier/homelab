


# Tier 3: LXC (GitOps Engine in Docker)

The LXC daemon is a Rust application packaged as a Docker container, running alongside your apps inside every Proxmox container. **node-sync.sh and container.sh are fully deprecated and must not be used.** All sync, management, and deployment logic is now implemented in Rust, with the daemon running an Axum web server for HTTP Push APIs and a fallback cron loop for eventual consistency.

- [ ] **Hyper-Modern Interface:** Just like the Client and Host applications, the Ratatui interface for this LXC Daemon MUST be highly polished and visually stunning when accessed via SSH. **Gum is not used anywhere in the system.**
- [ ] **Styling & Feedback:** Implements a centralized styling module with dynamic colors (Cyan/Magenta accents). Active tabs must be highlighted, background sync states must use animated spinners, and error logs must stand out in high-contrast Red. Modals must render as floating, centered pop-ups with a shadow effect.

- [ ] **Sparse Checkouts:** Autonomously fetches only the configuration for its specific stack using Git Sparse-Checkout, discarding any manual local changes ([7]).
- [ ] **Axum API & File-Locks:** Runs an Axum web server to receive HTTP Push triggers from the CLIENT. It also exposes `/api/backup/pause` and `/api/backup/resume` endpoints for the HOST backup orchestrator. To prevent race conditions with the fallback 30-minute cron job, it uses `fs2` file locks.

- [ ] **Bollard API Integration:** Communicates directly with `/var/run/docker.sock` to pull images, stop, and start containers.
- [ ] **Pre-Deploy Hooks (`setup.sh`):** Replaces `pre-sync.sh`. `pre-sync.sh` is no longer used for directory creation or secrets. `hooks/setup.sh` is strictly limited to creating external Docker networks (e.g., media_network) before the Bollard Docker API brings up the compose project. Legacy one-time migration scripts are forbidden.
- [ ] **Ephemeral Secrets Container:** Infisical (and its CLI) is entirely removed. To fetch secrets, the LXC daemon spins up a short-lived Docker container that pulls secrets, writes the `.env` file, and exits. If this container crashes, the deployment halts (Fail-Closed) to ensure apps never boot without secrets.
- [ ] **Atomic Mount Validation:** The daemon robustly checks the Linux `st_dev` (device ID) of the `/docker` and persistent `/config` directories. If they match, the mount failed, and the daemon refuses to start the application, ensuring bind-mounted persistent storage is securely attached before containers boot.

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
