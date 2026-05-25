# Homelab Features — Expanded & Explained


## 1. Centralized Log Aggregation (Promtail + Loki + Universal Grafana Dashboard)
- **What:** All container logs are collected and shipped to a central Loki server using Promtail. Grafana provides a universal, auto-updating dashboard for all logs.
- **How:** Each stack includes a Promtail container configured to tail logs from all apps in that stack. Logs are labeled with `stack` and `app` and sent to Loki. A single provisioned Grafana dashboard (see `stacks/monitoring/grafana/provisioning/dashboards/homelab-logs.json`) provides instant access to all logs, with dropdowns to filter by stack and app. New stacks/apps appear automatically—no manual dashboard edits needed.
- **Where:** Used in every stack (downloader, media, monitoring, paperless, etc.). Logs are viewed in Grafana’s web UI.
- **Why:** Enables unified log search, troubleshooting, and alerting across the entire homelab, with zero manual dashboard maintenance.


## 2. Automated Container Updates (Watchtower)
- **What:** Containers are automatically updated to the latest image versions, but only if explicitly enabled.
- **How:** Watchtower runs as a service in each stack, monitoring for new image versions and restarting containers as needed. Only containers with the label `com.centurylinklabs.watchtower.enable=true` are updated. This allows you to exclude critical apps (like Jellyfin during streaming, or database-backed services) from auto-updates to prevent downtime or data loss. Watchtower is also labeled to update itself in every stack.
- **Advanced:** Watchtower supports lifecycle hooks (pre/post-update commands) to check for active streams or database locks before updating (not always used, but supported by the system). 
- **Where:** All stacks, with selective enablement per app in docker-compose.yml.
- **Why:** Ensures security patches and new features are applied without manual intervention, while protecting critical workloads from unwanted restarts.
## 2a. Watchtower Tag-Based Update Control
- **What:** Fine-grained control over which containers are updated automatically.
- **How:** By omitting or setting `com.centurylinklabs.watchtower.enable=false` on a service, it is excluded from Watchtower updates. This is used for apps like Jellyfin (to avoid updates during streaming) and for database containers.
- **Where:** All stacks, especially media and database-backed apps.
- **Why:** Prevents downtime or data corruption during critical operations.
## 2b. Watchtower Lifecycle Hooks (Supported)
- **What:** Ability to run scripts before/after updating a container (e.g., check for active streams, block updates if busy).
- **How:** By setting Watchtower lifecycle labels (e.g., `com.centurylinklabs.watchtower.lifecycle.pre-update-command`).
- **Where:** Supported in all stacks, can be enabled as needed.
- **Why:** Adds safety for stateful or user-facing services.
## 6a. Custom Healthchecks and Dependency Order
- **What:** Ensures containers only start when dependencies are healthy (e.g., VPN up before torrenting).
- **How:** Uses Docker Compose healthchecks and `depends_on` with `condition: service_healthy`.
- **Where:** downloader/qbittorrent (waits for Gluetun VPN), other stacks as needed.
- **Why:** Prevents leaks and ensures correct startup order.
## 6b. Custom Network Modes
- **What:** Some services share a network stack for security (e.g., VPN chaining).
- **How:** `network_mode: service:<name>` in docker-compose.yml.
- **Where:** downloader/qbittorrent uses Gluetun's network stack.
- **Why:** Ensures all traffic is routed through VPN.
## 6c. Entrypoint/Command Overrides
- **What:** Some containers override the default entrypoint or command for custom startup logic (e.g., Promtail, Cloudflared, Paperless AI Assistant).
- **How:** Set in docker-compose.yml via `entrypoint:` or `command:`.
- **Where:** Various stacks.
- **Why:** Enables advanced configuration and integration.
## 6d. User/Group/Capabilities
- **What:** Some containers run as root (`user: "0:0"`) or with extra capabilities (`cap_add`, `devices`) for hardware or network access.
- **How:** Set in docker-compose.yml.
- **Where:** monitoring/grafana, downloader/qbittorrent, etc.
- **Why:** Required for certain hardware or privileged operations.
## 7a. GPU Passthrough for Hardware Acceleration
- **What:** Hardware acceleration for containers (e.g., Jellyfin transcoding) is enabled via safe LXC GPU passthrough.
- **How:** The `enable-gpu.sh` script configures the LXC for Intel/AMD GPU passthrough by appending the correct cgroup and mount entries to the LXC config. Bind mounts `/dev/dri/card0` and `/dev/dri/renderD128` are used for device access.
- **Where:** Any stack/app needing hardware video acceleration (media stack, Jellyfin, etc.).
- **Why:** Enables efficient video transcoding and hardware-accelerated workloads.
## 7b. Bind Mounts for Hardware Devices
- **What:** Host GPU devices are bind-mounted into containers for direct access.
- **How:** LXC config and docker-compose volumes.
- **Where:** Media stack, or any app needing GPU access.
- **Why:** Required for hardware acceleration.
## 9a. .env-Driven Secrets and Dynamic Config
- **What:** All secrets and dynamic config are injected at runtime via `.env` files, never hardcoded.
- **How:** All scripts and compose files source `.env` files for secrets and config.
- **Where:** All stacks and apps.
- **Why:** Ensures security, flexibility, and easy rotation of secrets.

## 3. GitOps-Driven Configuration & Deployment
- **What:** All stack and app configurations are managed in Git and automatically applied to running containers.
- **How:** node-sync.sh runs every 5 minutes in each LXC, pulling the latest changes from Git, running pre-sync scripts, and applying docker-compose changes.
- **Where:** All stacks, via node-sync.sh and pre-sync.sh.
- **Why:** Guarantees reproducibility, easy rollback, and auditability of all changes.

## 4. .env-Based Secrets Management
- **What:** All secrets (API keys, passwords, tokens) are stored in .env files, not in code or Git.
- **How:** .env files are injected into containers at runtime and sourced by scripts.
- **Where:** All stacks and apps.
- **Why:** Keeps secrets out of version control and allows for easy rotation.

## 5. Pre-Sync Script Automation
- **What:** Each stack can have a pre-sync.sh script that runs before docker-compose up.
- **How:** Used for tasks like exporting secrets, seeding config files, or preparing directories.
- **Where:** downloader/pre-sync.sh (exports Infisical secrets, seeds qBittorrent config), media/pre-sync.sh, etc.
- **Why:** Automates one-off or per-deploy setup steps, ensuring containers always start with the right config.

## 6. Healthchecks and Dependency Management
- **What:** Containers can depend on the health of other containers before starting.
- **How:** For example, qBittorrent uses network_mode: service:gluetun and only starts when Gluetun’s VPN tunnel is healthy (checked via wget to 127.0.0.1:9999).
- **Where:** downloader stack (qBittorrent + Gluetun).
- **Why:** Prevents leaks (e.g., no torrenting without VPN) and ensures correct startup order.

## 7. TUN Device Passthrough for VPN Containers
- **What:** VPN containers (like Gluetun) require access to the TUN device for WireGuard/OpenVPN.
- **How:** bootstrap-lxc.sh and host.sh scripts auto-detect if a stack needs TUN and configure the LXC accordingly.
- **Where:** downloader stack, any stack using VPNs.
- **Why:** Ensures VPN containers work out-of-the-box without manual LXC tweaks.

## 8. Garbage Collection of Orphaned App Data
- **What:** If an app is removed from Git, its containers are stopped and its data is deleted automatically.
- **How:** node-sync.sh detects missing app folders and triggers cleanup.
- **Where:** All stacks.
- **Why:** Prevents orphaned data from filling up disks and keeps the system tidy.

## 9. Centralized Management Scripts (client.sh, host.sh, container.sh)
- **What:** Interactive CLI menus for all major management tasks (create, remove, reset, sync, enable TUN, etc.).
- **How:** Scripts in the repo root provide a unified interface for both host and container operations.
- **Where:** Used on both the Proxmox host and inside LXCs.
- **Why:** Simplifies management and reduces the risk of manual errors.

## 10. Structured Logging for Sync Operations
- **What:** node-sync.sh emits logs in logfmt format, with fields for timestamp, level, stack, app, and message.
- **How:** Promtail parses these logs and sends them to Loki with appropriate labels.
- **Where:** All stacks.
- **Why:** Enables powerful filtering and alerting in Grafana.

## 11. Auto-Provisioning of Grafana Datasources
- **What:** Grafana is configured to automatically add Loki as a datasource on startup.
- **How:** Provisioning files in /appdata/monitoring/grafana.
- **Where:** monitoring stack.
- **Why:** Ensures logs are always available in Grafana without manual setup.

## 12. Remote Path Mappings for Media Automation
- **What:** Sonarr, Radarr, and other media managers use remote path mappings to translate download client paths to their own filesystem.
- **How:** Configured in the Arr suite’s Web UI.
- **Where:** media stack.
- **Why:** Allows seamless import of downloads, even if the download client and media manager see different paths.

## 13. Centralized Reverse Proxy (Nginx Proxy Manager)
- **What:** All web UIs are routed through a single, user-friendly reverse proxy.
- **How:** Nginx Proxy Manager container, with CrowdSec integration for security.
- **Where:** gateway stack.
- **Why:** Simplifies access, enables SSL, and provides a single point for access control and monitoring.

## 14. Security Integrations (CrowdSec)
- **What:** CrowdSec protects the reverse proxy and other services from abuse and attacks.
- **How:** Integrated with Nginx Proxy Manager.
- **Where:** gateway stack.
- **Why:** Adds an active security layer to your homelab.

## 15. Automated Healthchecks and Service Restarts
- **What:** Containers are monitored for health, and unhealthy containers are restarted automatically.
- **How:** Docker Compose healthcheck directives and Watchtower.
- **Where:** All stacks, especially downloader (Gluetun healthcheck).
- **Why:** Ensures high availability and self-healing.

## 16. Bind-Mounted Persistent Storage
- **What:** All important data is stored on the host (or fileserver) and bind-mounted into containers.
- **How:** /appdata/<stack>/<app> and /mnt/downloads are mounted into containers.
- **Where:** All stacks.
- **Why:** Ensures data survives container or LXC recreation and is easy to back up.

## 17. Automated Backups (Restic)
- **What:** The host runs Restic to back up all /appdata folders, pausing containers as needed to prevent data corruption.
- **How:** Host-side backup scripts with container labels for pause/resume.
- **Where:** Proxmox host.
- **Why:** Provides reliable, consistent backups of all critical data.

## 18. Gum TUI Integration for Scripts
- **What:** Management scripts use Gum for rich, interactive terminal UIs when available.
- **How:** lib-ui.sh auto-detects Gum and switches to TUI mode.
- **Where:** All management scripts.
- **Why:** Improves usability and reduces errors in interactive sessions.

## 19. Automatic TUN Passthrough Detection
- **What:** Scripts detect if a stack needs TUN and configure the LXC automatically.
- **How:** bootstrap-lxc.sh and host.sh.
- **Where:** Any stack using VPNs.
- **Why:** Removes manual steps and ensures VPN containers always work.


## 20. Cron-Driven GitOps Sync
- **What:** node-sync.sh is run every 5 minutes via cron in each LXC.
- **How:** Configured by bootstrap-lxc.sh.
- **Where:** All stacks.
- **Why:** Ensures all changes in Git are applied quickly and automatically.

## 21. Automated OS Security Updates (unattended-upgrades)
- **What:** All Debian/Ubuntu-based containers receive automatic security updates for the OS.
- **How:** The bootstrap-lxc.sh script installs and configures unattended-upgrades in every LXC, ensuring critical security patches are applied without manual intervention.
- **Where:** All LXCs (hosted containers).
- **Why:** Reduces attack surface and keeps the base system secure with minimal effort.

## 22. Automatic Directory Creation for Bind-Mounts
- **What:** All required bind-mount directories for persistent data/config are auto-created on de Proxmox host vóór containers starten.
- **How:** pre-sync.sh en de create-new-stack/app scripts parsen docker-compose.yml en maken automatisch alle benodigde directories aan in /opt/appdata/<stack>/<app>.
- **Where:** Alle stacks en apps, bij elke (her)deploy of nieuwe stack/app creatie.
- **Why:** Voorkomt dat containers zonder bind-mounts starten (en data verliezen), maakt recovery/migratie eenvoudiger, en garandeert dat data altijd op de juiste plek op de host staat.

## 23. Dynamic Secrets Provisioning (Infisical)
- **What:** Secrets en gevoelige config worden automatisch geëxporteerd vanuit Infisical naar .env-bestanden per stack/app.
- **How:** pre-sync.sh scripts roepen Infisical CLI aan om secrets te exporteren vóór containers starten, zodat alle apps hun secrets als environment variables krijgen.
- **Where:** Alle stacks/apps die secrets nodig hebben.
- **Why:** Houdt secrets veilig buiten Git, maakt rotatie en beheer eenvoudig, en zorgt dat containers altijd up-to-date secrets hebben.
