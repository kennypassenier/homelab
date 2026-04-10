# Homelab GitOps Architecture

Welcome to the Homelab GitOps repository. This project contains the infrastructure as code (IaC) and configuration for managing a homelab environment using Proxmox, LXC containers, Docker, and transparent Git encryption (SOPS + Age).

## 🚀 Quick Start: Central Managers

For ease of use, all operations have been bundled into three central interactive manager scripts located in the root of this repository. Instead of remembering individual script paths, simply run the manager for your current environment:

- **`./client.sh`**: Run this on your local workstation (e.g., Pop!_OS) to create/remove stacks and apps, add SSH aliases, or initialize encryption.
- **`./host.sh`**: Run this on the Proxmox server to bootstrap new LXC containers, manage backups, enable GPU passthrough, or sync the host configuration.
- **`./container.sh`**: Run this inside an LXC container (usually in `/opt/gitops`) to manually trigger the GitOps sync.

## 📚 Documentation (Wiki)

To keep the root of this repository clean and maintainable, all detailed documentation has been moved to the `docs/` directory. Please refer to the following files for in-depth information:

- **[Part 1: Architecture](docs/01-architecture.md)**: High-level overview of GitOps, LXC, Storage, Secrets, and the sync loop.
- **[Part 2: Ground Zero](docs/02-ground-zero.md)**: Initializing SOPS and Age encryption on your local workstation.
- **[Part 3: Bootstrapping & Networking](docs/03-bootstrapping-and-networking.md)**: Creating LXCs, static IPs, and SSH config.
- **[Part 4: GitOps & Lifecycle](docs/04-gitops-and-lifecycle.md)**: The 5-minute sync loop, app updates, and GC.
- **[Part 5: Backups & Host](docs/05-backups-and-host-management.md)**: Restic backups, GPU passthrough, and host sync.
- **[Part 6: User Guide & Responsibilities](docs/06-user-guide-and-responsibilities.md)**: Clear mapping of manual user actions vs. automated system processes.
- **[Philosophy & Guidelines](docs/PHILOSOPHY.md)**: Our core design principles, coding standards, and best practices (DRY, shared UI libraries, idempotency, safety, etc.). **Must read** before contributing.
- **[LLM Context](docs/LLM_CONTEXT.md)**: Essential context and rules for LLMs (like Claude, ChatGPT, Gemini) assisting with this project.

## 📂 Repository Structure

```text
homelab/
├── apps/                   # Individual application configurations (Docker Compose, .env)
├── docs/                   # Detailed documentation and guidelines (Wiki)
├── scripts/                # Individual script modules (client, host, container, shared)
├── secrets/                # Encrypted Age private key
├── client.sh               # Central manager for local workstation actions
├── host.sh                 # Central manager for Proxmox host actions
└── container.sh            # Central manager for container actions
```
