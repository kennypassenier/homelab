# Homelab GitOps Architecture

Welcome to the Homelab GitOps repository. This project contains the infrastructure as code (IaC) and configuration for managing a homelab environment using Proxmox, LXC containers, and Docker.

## Release Automation

- HOST daemon updates are release-driven (GitHub Releases) rather than push-triggered.
- LXC daemon image is built and published to GHCR via change-aware GitHub Actions.
- Local `make build-host` now builds the daemon inside a Debian 12 Rust container so release assets remain compatible with Proxmox host glibc.
- HOST self-update now preflights downloaded binaries (`--version` + dynamic-link check), keeps a local backup, and arms a rollback watchdog after restart.
- Local `make build-lxc` now builds the daemon inside a Debian 12 Rust container so the resulting binary stays compatible with older glibc versions inside deployed LXCs.
- Makefile build/release targets now auto-run `latch commit` + `latch push` on desktop before Rust builds (best-effort by default; configurable via env).
- HOST auto-provisioning is now opt-in via `HOST_AUTO_PROVISION_ENABLED=1`; default behavior is provision only on explicit CLIENT/API trigger.
- Deployment order, required tokens, and env templates are documented in `docs/deployment.md`.
- Copy/paste diagnostics and manual recovery commands are documented in `docs/debug.md`.
- HOST runs headless-only in deployed mode.
- CLIENT keeps persistent websocket connections to HOST and active LXC stacks with automatic reconnect behavior.
- CLIENT/HOST/LXC websocket links now use active keepalive traffic to prevent stale idle disconnects.
- Upgrade visibility: HOST and LXC emit `daemon_version=...` lifecycle logs; CLIENT highlights version changes and reconnect transitions in the Logs tab.
- HOST API quick checks (for Postman/curl): `GET /api/health`, `GET /api/version`, `GET /api/metrics` on `http://<host-ip>:8080`.
- Update triggers: `POST /api/update` on HOST and LXC starts immediate update checks outside the periodic windows.
- Provisioning fail-close: if HOST provisioning fails for a stack, that stack is automatically set to `deploy.enabled=false` in its `stacks/<stack>/lxc-compose.yml` until you explicitly re-enable it.
- LXC bootstrap strategy: HOST pushes a Debian-12-compatible prebuilt latch binary (`latch-linux-x86_64-lxc.tar.gz` release path or local `make build-lxc` output), then runs the lightweight wrapper installer inside the LXC.

## 🚀 Quick Start: Central Managers

For ease of use, all operations have been bundled into three central interactive manager scripts located in the root of this repository. Instead of remembering individual script paths, simply run the manager for your current environment:

- **`./client.sh`**: Run this on your local workstation (e.g., Linux desktop) to create/remove stacks and stacks, add SSH aliases, or initialize encryption.
- **`./host.sh`**: Run this on the Proxmox server to bootstrap new LXC containers, manage backups, enable GPU passthrough, or sync the host configuration.
- **`./container.sh`**: Run this inside an LXC container (usually in `/opt/gitops`) to manually trigger the GitOps sync.

## 📚 Documentation (Wiki)

To keep the root of this repository clean and maintainable, all detailed documentation has been moved to the `docs/` directory. Please refer to the following files for in-depth information:

- **[Part 1: Architecture](docs/01-architecture.md)**: High-level overview of GitOps, LXC, Storage, Secrets, and the sync loop.
- **[Part 2: Ground Zero](docs/02-ground-zero.md)**: (Deprecated) Initializing secrets management on your local workstation.
- **[Part 3: Bootstrapping & Networking](docs/03-bootstrapping-and-networking.md)**: Creating LXCs, static IPs, and SSH config.
- **[Part 4: GitOps & Lifecycle](docs/04-gitops-and-lifecycle.md)**: The 5-minute sync loop, app updates, and GC.
- **[Part 5: Backups & Host](docs/05-backups-and-host-management.md)**: Restic backups, GPU passthrough, and host sync.
- **[Part 6: User Guide & Responsibilities](docs/06-user-guide-and-responsibilities.md)**: Clear mapping of manual user actions vs. automated system processes.
- **[Part 7: Troubleshooting & Debugging](docs/07-troubleshooting-and-debugging.md)**: Common sync errors and permission fixes.
- **[Part 8: Centralized Monitoring](docs/08-centralized-monitoring.md)**: Viewing container logs via Loki, Promtail, and Grafana.
- **[Contributing Guidelines](docs/CONTRIBUTING.md)**: Core design principles, coding standards, and best practices (DRY, shared UI libraries, idempotency, safety, etc.). **Must read** before contributing.
- **[LLM Context](docs/LLM_CONTEXT.md)**: Essential context and rules for LLMs (like Claude, ChatGPT, Gemini) assisting with this project.

## 📂 Repository Structure & Storage Layout

```text
homelab/
├── stacks/                   # GitOps-managed: docker-compose.yml, scripts, etc. (per app)
│   ├── media/
│   │   ├── jellyfin/         # GitOps: docker-compose.yml, scripts, etc.
│   │   └── ...
│   └── ...
├── docs/                     # Detailed documentation and guidelines (Wiki)
├── scripts/                  # Individual script modules (client, host, container, linux, shared)
├── secrets/                  # (Legacy) Encrypted secrets (no longer used)
├── client.sh                 # Central manager for local workstation actions
├── host.sh                   # Central manager for Proxmox host actions
└── container.sh              # Central manager for container actions
```

## Linux Requirements Bootstrap

To install build/release dependencies on a Linux workstation (including Docker, Rust, GH CLI, and cross-target tooling), use:

- `./scripts/linux/install-requirements.sh` (auto-detect distro family)
- `./scripts/linux/install-requirements-arch.sh` (Arch/Garuda/EndeavourOS)
- `./scripts/linux/install-requirements-debian.sh` (Debian/Ubuntu)

### New Storage & Config Layout (Inside Container)

For each app, the directory structure inside the container is:

```text
/$stackname/
	$appname/           # GitOps-managed: docker-compose.yml, scripts, etc.
	$appname-config/    # Persistent config/data (bind mount from Proxmox host)
```

**Example:**

```text
/media/
	jellyfin/           # GitOps: docker-compose.yml, scripts, etc.
	jellyfin-config/    # Persistent config/data
	sonarr/
	sonarr-config/
	...
```

**Rationale:**
- All persistent config/data is now in `$stackname/$appname-config` (bind mount from host)
- All GitOps-managed files (compose, scripts) are in `$stackname/$appname`
- This makes navigation and management much more logical and user-friendly
