💻 Tier 1: CLIENT (The Desktop TUI)
[ ] Ratatui Interface: Set up a modern terminal UI with floating modals, 3D shadow effects, dynamic colors, and spinners instead of plain text.
[ ] Tab Navigation: Build a state machine for tabs (Dashboard, Scaffolding, HostManagement).
[ ] App Scaffolding: Dynamically generate docker-compose.yml templates using Askama/Tera. These templates must include Traefik labels, Watchtower, and Promtail configurations.
[ ] MAC Address Generator: Generate safe, random Locally Administered MAC addresses to prevent DHCP conflicts.
[ ] Pre-flight Linting: Parse and validate YAML code via serde_yaml before allowing a Git commit.
[ ] Blast Radius Security: Display a stark red, floating modal when deleting an app/stack. The action is only executed if the user types the exact app/stack name.
[ ] Idempotent SSH Management: Locally read and write the ~/.ssh/config file to add aliases without creating duplicate or corrupted entries.
[ ] GitOps Integration: Automatically stage, commit (with generated commit messages), and push changes to the main branch.
[ ] HTTP Push API Trigger: Send an HTTP POST request with a Bearer token to the LXC Daemon to force deployments.
[ ] Live SSE Telemetry: Establish an asynchronous connection that streams deployment logs from the LXC live at the bottom of the desktop UI.
🖥️ Tier 2: HOST (The Proxmox Daemon)
[ ] Native Daemon Setup: Initialize the Ratatui project that runs natively on the bare-metal Proxmox server.
[ ] Proxmox API Client: Build a reqwest client to create (clone) new LXC containers, including the injection of static network MAC addresses.
[ ] Post-Provisioning Exec Hook: Implement an atomic Proxmox Exec API call that immediately runs apt-get update && upgrade, and installs Docker + unattended-upgrades right after LXC creation.
[ ] Atomic Hardware Parser (GPU/TUN): Read config files (/etc/pve/lxc/<vmid>.conf) and append lxc.cgroup2.devices.allow rules for hardware acceleration or VPN tunnels via a safe, atomic "rename" operation.
[ ] Backup Orchestrator: Send HTTP POST calls (/api/backup/pause) to specific LXC containers to freeze applications before initiating the Restic backup.
[ ] Rust Drop Guards: Guarantee that the resume signal (/api/backup/resume) is always sent after a backup, even if Restic crashes or panics.
[ ] CI/CD Self-Updater: Automatically query the GitHub Releases API, download the latest Rust binary to /tmp/, atomically overwrite itself, and restart via systemd.
📦 Tier 3: LXC (The GitOps Engine in Docker)
[ ] Bollard Docker Integration: Connect to /var/run/docker.sock to locally pull images, and stop/start/remove containers.
[ ] Sparse Git Checkouts: Autonomously fetch the code for only the active stack (discarding local changes) using Git Sparse-Checkout to replace the old shell scripts.
[ ] Atomic Mount Validation: Implement a built-in check comparing the Linux st_dev ID of /docker with /config to detect bind-mount failures and prevent data loss.
[ ] Ephemeral Secrets Container: Fire up a temporary Docker container to securely fetch variables and write the .env configuration. Abort the deployment (Fail-Closed) if this fails.
[ ] Fail-Safe Rollbacks: Check if a newly started container crashes within 10 seconds; if so, automatically attempt to restart the previously running image IDs.
[ ] Garbage Collection: Detect deleted Git folders and remove orphaned containers and images via the Docker API.
[ ] Force-Delete Security: Ensure actual file data on the host mount is only deleted if the trigger API call contains the force_deletion=true token.
[ ] Pre-Deploy Hooks: Check for the existence of a hooks/setup.sh file (e.g., for creating external Docker networks) and execute it before deploying.
[ ] Axum API with File-Locks: Run the web server for the Push API while simultaneously acquiring an fs2 lock to prevent overlapping deploys from the fallback cronjob.
[ ] Webhook Notifications: Automatically send an HTTP POST request to Ntfy/Discord if an automatic deployment fails and triggers a rollback.
🌐 Homelab Wide / Existing Integrations (To be migrated)
[ ] Traefik Reverse Proxy: Configure Traefik with Docker labels to replace Nginx Proxy Manager.
[ ] CrowdSec L7 Bouncer: Integrate the CrowdSec bouncer mechanism as a middleware within Traefik.
[ ] Watchtower Auto-Updates: Retain Watchtower configurations in the stacks and ensure the Tier 3 LXC daemon image is automatically updated via the GitHub Container Registry (GHCR).
[ ] Promtail/Loki Logs: Maintain the Promtail structure so new and existing apps send the correct logfmt labels (ts, level, stack, app) to Loki and the auto-provisioned Grafana dashboard.
[ ] GitHub Actions Monorepo Filters: Configure the CI/CD pipelines with paths filters so only modified tiers (directories) trigger a build.
[ ] Rust Unit Tests: Apply Test-Driven Development (e.g., test the MAC validator, the idempotency of the passthrough scripts, and mock the mount ID checks).