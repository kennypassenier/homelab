# Architecture Overview

Welcome to the core architecture documentation for the Homelab GitOps repository. This document explains the foundational design principles, the technologies chosen, and *why* this specific architecture was built to manage the homelab environment.

By reading this document, developers and LLMs will understand the flow of data, how secrets are handled, how networking is structured, and how the GitOps reconciliation loop keeps everything in sync.

---

## 1. High-Level Concept: GitOps

The primary philosophy driving this homelab is **GitOps**. In this paradigm, this Git repository serves as the single source of truth for the entire infrastructure. 

*   **Declarative State:** You do not SSH into a server to manually run `docker run` or edit a `.env` file. Instead, you define the desired state in Git (e.g., creating a new stack folder with a `docker-compose.yml`).
*   **Automated Reconciliation:** The servers continuously pull this repository and apply the changes automatically. If a server dies, you can bootstrap a new one, and it will rebuild itself perfectly from Git within minutes.

## 2. Compute Layer: Proxmox VE & LXC Containers

Instead of running one massive virtual machine (VM) with dozens of Docker containers, or managing a complex Kubernetes cluster, this homelab uses **Proxmox Virtual Environment (VE)** running **unprivileged Linux Containers (LXC)**.

### Why LXC?
*   **Performance & Efficiency:** LXCs share the host's kernel, meaning they have almost zero overhead compared to full VMs. They boot instantly and consume minimal RAM/CPU.
*   **Isolation (Stacks):** Applications are logically grouped into "Stacks" (e.g., `media`, `gateway`, `paperless`). Each Stack gets its own isolated LXC container. If the `media` stack crashes or is compromised, the `gateway` stack remains unaffected.
*   **Docker Inside LXC:** Docker and Docker Compose run *inside* these unprivileged LXCs. This provides the standard containerized workflow developers love, combined with the hard isolation of Proxmox containers.

**Standard LXC Sizing:**
*   **CPU:** 2 Cores
*   **RAM:** 1024 MB (1 GB)
*   **Swap:** 512 MB
*   **OS Disk:** 8 GB (Base OS and Docker binaries only).

## 3. Storage Strategy: Host Bind Mounts

Data persistence is critical. Docker volumes inside an LXC can be difficult to back up and migrate. Therefore, we use **unprivileged bind-mounts** from the Proxmox host.

*   **Fast App Data:** The fast NVMe SSD storage on the Proxmox host (`/opt/appdata/<STACK_NAME>`) is directly bind-mounted into the LXC container at `/appdata`. 
*   **Why?** This allows the Proxmox host to centrally back up all application configurations (via Restic) without needing to SSH into every individual container. It also means if an LXC is destroyed, the actual configuration data remains safely on the host.
*   **Bulk Storage:** Heavy media or backup data is kept on separate NAS drives or secondary HDDs (e.g., `/HDD2TB`) and is mounted into the specific LXCs that need it (like the `media` stack).

## 4. Secret Management: SOPS + Age

You cannot commit plaintext passwords, API keys, or database credentials to a Git repository. To solve this, we use **Mozilla SOPS** combined with **Age**.

*   **Transparent Encryption:** Developers use a local setup (`client.sh -> Initialize Ground Zero`) which installs Git filters (smudge/clean). 
*   **How it works:** When you commit a `.env` file, Git automatically intercepts it and encrypts the contents using your public Age key. The file is stored in GitHub as ciphertext.
*   **Decryption:** When the LXC container pulls the repository, the GitOps script uses the private Age key (provided during bootstrap) to seamlessly decrypt the `.env` file back to plaintext before Docker Compose runs.
*   **Why Age?** Age is a simple, modern, and secure encryption tool. It is much lighter and easier to manage in a homelab than heavy enterprise solutions like HashiCorp Vault.

## 5. Networking & Routing

To keep the architecture simple and decoupled, networking is handled outside of Proxmox.

*   **OPNsense (DHCP):** We rely on an OPNsense router for network management. When a new LXC is bootstrapped, it generates a unique MAC address. We assign a static IP to this MAC address via OPNsense Kea DHCP Reservations.
*   **Local SSH Aliases:** To easily access the containers from the client workstation (Pop!_OS), we manage `~/.ssh/config` via the `client.sh` manager. This allows typing `ssh media` instead of remembering IP addresses.
*   **Reverse Proxy:** The `gateway` stack runs Nginx Proxy Manager and CrowdSec, handling all incoming web traffic, SSL termination, and L7 security (blocking malicious IPs).

## 6. The GitOps Reconciliation Loop

The heart of the automation is the `node-sync.sh` script, which runs inside every LXC container via a cronjob every **5 minutes**. 

Here is the step-by-step flow of the reconciliation loop:
1.  **Git Sparse Checkout:** The container only pulls the specific `apps/<STACK_NAME>` folder it needs, ignoring the rest of the repository to save space and time.
2.  **Pull Latest State:** `git pull origin main` retrieves the latest declarative state.
3.  **Decrypt Secrets:** SOPS automatically decrypts any updated `.env` files.
4.  **Pre-Sync Hooks:** If a `pre-sync.sh` script exists in the stack folder (e.g., for setting up external Docker networks or migrating data folders), it is executed. These scripts are designed to be idempotent.
5.  **Garbage Collection (GC):** The script checks the host data in `/opt/appdata/<STACK_NAME>`. If it finds a folder for an app that no longer exists in Git, it dynamically stops the container and permanently deletes the host data. This ensures no orphaned data is left behind.
6.  **Apply State:** Finally, it runs `docker compose pull -q` (to fetch new image updates) followed by `docker compose up -d --remove-orphans`.

## 7. Observability & Automated Updates

*   **Watchtower:** A centralized Watchtower container runs in every stack. It automatically checks for base image updates and gracefully restarts containers. It includes pre-checks (e.g., ensuring Jellyfin has no active streams before updating) to prevent downtime.
*   **Promtail & Loki:** Containers optionally run Promtail, which reads Docker logs and pushes them to a central Loki/Grafana instance in the `monitoring` stack. Environment variables for Promtail (like `LOKI_IP`) are dynamically injected via `.env` files and `-config.expand-env=true`, keeping configuration files free of hardcoded IP addresses.

---

**Next Steps:**
To understand how to prepare your local machine to work with this architecture, please proceed to **[Part 2: Ground Zero Setup](02-ground-zero.md)**.