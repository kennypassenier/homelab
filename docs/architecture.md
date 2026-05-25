# Global Architecture & Infrastructure Decisions (Rust Refactor)

This document outlines the overarching architectural principles, network designs, and storage layouts that apply to the entire GitOps homelab. These are the global rules that govern the 3-Tier Rust applications (CLIENT, HOST, and LXC).

## 1. Core GitOps & Infrastructure Philosophy
*   **Git is the Absolute Single Source of Truth:** No manual `docker run` commands, and no manual file edits on the server. If it is not in the Git repository, it does not exist [1].
*   **Idempotency & Fail-Closed Safety:** Every script, hook, and application must be able to run multiple times without causing duplicate entries or corruption [2]. If a critical dependency fails (e.g., a missing secret, a failed mount), the system must Fail-Closed (abort and crash) rather than boot in a vulnerable state.
*   **The Sync Loop (Push + Cron Fallback):** The primary deployment mechanism is the new HTTP Push API (triggered by the CLIENT for immediate deployments). However, a 30-minute fallback cron job guarantees that the system eventually reaches the desired Git state, even if a commit was made from another device [1].

## 2. Storage Layout & Persistence Strategy
*   **Stateless LXCs & Centralized Host Data:** Application data is never permanently stored inside the LXC container filesystem. All persistent configuration data lives on the Proxmox Host's fast NVMe SSD under `/opt/appdata/<STACK_NAME>` [3, 4].
*   **Unprivileged Bind Mounts:** The host directory is shared with the unprivileged LXC at `/appdata` via bind mounts [3]. Docker Compose files reference this path as `/appdata/<STACK>/<APP>/config` [5].
*   **Automatic Directory Creation:** To prevent containers from booting without bind-mounts (and thus losing data), the pre-deploy scaffolding dynamically creates all necessary directories in `/opt/appdata` on the Proxmox host before containers start [4, 6].
*   **Separation of Media Storage:** Large, replaceable media files are kept off the NVMe backup path. They are stored on separate spinning-disk arrays and mounted directly into the LXC at `/mnt/data/18TB` and `/mnt/data/12TB` [5].

## 3. Networking & DHCP Routing
*   **MAC-Based Static IPs:** IP addresses are never hardcoded inside the LXC OS. Instead, the CLIENT generates a safe MAC address, and the router (OPNsense) uses DHCP reservations to assign stable static IPs based on that MAC [3, 7].
*   **SSH Alias Management:** Access is managed via `~/.ssh/config` aliases (e.g., `ssh media`, `ssh gateway`), preventing the need to memorize IP addresses [8].
*   **Isolated Docker Bridge Networks:** Apps that need to communicate within the same stack (e.g., Paperless webserver and PostgreSQL) are placed on a shared, isolated Docker bridge network (e.g., `paperless_network`). These networks are created idempotently by the `hooks/setup.sh` pre-deploy scripts before Docker Compose runs [9].
*   **VPN Kill-Switch (Network Namespaces):** Containers that require VPN protection (like qBittorrent) do not have their own network stack. They use `network_mode: service:gluetun`, forcing all traffic through the WireGuard VPN container. If the VPN drops, the app loses all internet access instantly [10-12].

## 4. Secret Management Paradigm
*   **No Encrypted Commits (Goodbye SOPS/Age):** Storing encrypted secrets in Git (via SOPS) has been fully deprecated in favor of dynamic runtime injection [3, 13].
*   **Ephemeral Secrets Container:** Replacing Infisical, the LXC daemon now spins up a temporary Docker container prior to deployment. This container fetches secrets from a secure vault, writes them to a local `.env` file (which is `chmod 600` and `.gitignore`d), and immediately exits [2]. If this fails, the deployment halts.

## 5. Centralized Observability & Telemetry
*   **Universal Log Aggregation:** Every stack includes a `Promtail` container that ships all Docker logs to a centralized `Loki` instance on the monitoring LXC [14].
*   **Structured `logfmt` Telemetry:** The LXC GitOps Engine emits structured logs (`ts=... level=... stack=... app=... msg=...`) [15]. Promtail parses these to extract `level`, `stack`, and `app` as queryable Loki labels [16, 17].
*   **Zero-Maintenance Grafana:** Grafana is auto-provisioned directly from Git [18]. The "Homelab Logs" dashboard dynamically discovers newly deployed stacks and apps based on the Loki labels, requiring zero manual dashboard edits when adding a new app [19-21].

## 6. System-Wide Security Integrations
*   **Traefik + CrowdSec (L7 Bouncer):** Replacing the legacy Nginx Proxy Manager, Traefik serves as the GitOps-driven reverse proxy. It integrates the official CrowdSec Bouncer middleware to analyze access logs and block malicious IPs across the entire homelab [22, 23].
*   **Automated OS Security Patching:** During LXC provisioning, the HOST daemon automatically installs `unattended-upgrades`. This guarantees that the underlying Debian/Ubuntu OS in every container receives critical security patches without manual intervention [6, 24].

## 7. CI/CD & Monorepo Workflows
*   **Monorepo Path Filtering:** The entire 3-Tier architecture lives in a single Git repository. GitHub Actions uses path filters (`paths: 'client-app/**'`) so that changes to one tier do not trigger builds for another.
*   **Automated Release & Self-Healing:**
    *   The **LXC Daemon** is built as a Docker image, pushed to GHCR, and automatically updated across all containers via Watchtower.
    *   The **HOST Daemon** compiles natively via GitHub Actions, publishes to GitHub Releases, and pulls its own updates automatically.
