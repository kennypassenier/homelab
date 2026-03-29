# Homelab GitOps Architecture

This repository contains the infrastructure as code (IaC) and GitOps configuration for managing a homelab environment using Proxmox, LXC containers, Docker, and transparent Git encryption (SOPS + Age).

## Architecture Overview

The goal of this setup is to manage all self-hosted applications centrally via this Git repository.

- **Security:** Secrets (`.env` files) are encrypted in the repository using Mozilla SOPS and Age. They are decrypted seamlessly on the local machine and inside the containers.
- **Isolation:** Every application runs in its own unprivileged Proxmox LXC container.
- **Efficiency:** Containers use Git Sparse Checkouts to only download the files they need (their specific app directory and shared scripts).
- **Storage:** Fast-access app configurations (SSD) are stored locally in the container's Gitops directory (`./config`), while heavy media or backup data is bind-mounted from a centralized ZFS host drive (e.g. `/HDD2TB`).
- **Networking:** IP assignments are handled centrally via OPNsense (Kea DHCP Reservations) using the container's MAC address.

---

## Phase 1: Ground Zero (Local Setup)

This phase initializes the encryption keys and configures your local machine (e.g., Pop!\_OS) to automatically encrypt `.env` files when committing to GitHub.

### 1. Run the Initialization Script

Execute the script from the root of the repository to generate your Age keypair and set up the Git filters.

```bash
bash scripts/client/init-ground-zero.sh
```

_Note: You will be prompted to enter a strong passphrase (`PLACEHOLDER_PASSPHRASE`) to symmetrically encrypt your private Age key for secure storage in the repository._

### 2. Commit the Base Infrastructure

Once the script finishes, it generates `.sops.yaml`, `.gitattributes`, and `secrets/age.key.enc`. Push these to your repository:

```bash
git add .sops.yaml .gitattributes secrets/age.key.enc scripts/
git commit -m "chore: Initialize Ground Zero encryption"
git push -u origin main
```

---

## Phase 2: Bootstrapping a New Stack (Proxmox Host)

When you want to deploy a new stack of applications, you create a new LXC container in Proxmox and run the bootstrap script.

### 1. Generate the Stack Template Locally

From the root of your local repository, run the stack generator script. This interactive script lets you create multiple apps within a single stack, optionally configures a centralized Watchtower for automatic updates, and prepares `.env` files.

```bash
./scripts/client/create-new-stack.sh
```

After generation, configure your `docker-compose.yml` and `.env` files, then push them to Git. SOPS will automatically encrypt the `.env` files.

### 2. Run the Bootstrap Script on Proxmox

Log into your Proxmox host via SSH and run the bootstrap script to provision the LXC container.

**Usage:**

```bash
./scripts/host/proxmox-bootstrap-lxc.sh <VMID> <APP_NAME> <GITHUB_PAT> <AGE_PASSPHRASE> <GITHUB_USERNAME>
```

**Example:**

```bash
./scripts/host/proxmox-bootstrap-lxc.sh 101 pihole PLACEHOLDER_TOKEN PLACEHOLDER_PASSPHRASE PLACEHOLDER_GITHUB_USERNAME
```

**What this script does:**

1. Mounts the ZFS host storage (`/HDD2TB/appdata/<APP_NAME>`) to the container (`/appdata`) for large data and backups.
2. Installs Docker, SOPS, and Age inside the LXC container.
3. Performs a Git Sparse Checkout using your `PLACEHOLDER_TOKEN` to only fetch the necessary app directory.
4. Decrypts the Age key using your `PLACEHOLDER_PASSPHRASE` and automatically decrypts your `.env` files.
5. Pulls your public SSH keys from GitHub (`https://github.com/PLACEHOLDER_GITHUB_USERNAME.keys`) so you can access the container without a password.
6. Outputs the MAC address of the new container for network configuration.

---

## Phase 3: Network Configuration (OPNsense & Local SSH)

Because DHCP and routing are handled outside of Proxmox, we use OPNsense to assign static IPs.

### 1. Reserve the IP in OPNsense

1. Copy the MAC address outputted at the end of the `proxmox-bootstrap-lxc.sh` script.
2. Go to your OPNsense router interface.
3. Navigate to **Kea DHCP -> Reservations**.
4. Add a new reservation mapping the MAC address to your desired static IP.
5. Restart the LXC container in Proxmox to lease the new IP.

### 2. Local SSH Configuration (Pop!\_OS)

Once the LXC container has a static IP, you can register it on your local Pop!\_OS machine for easy SSH access.

First, ensure the script is executable (only needed once):

```bash
chmod +x scripts/client/register-local-node.sh
```

Then, run the interactive script:

```bash
./scripts/client/register-local-node.sh
```

**What this script does:**

1. Prompts you for a logical Host alias (e.g., `media-stack`).
2. Prompts you for the static IP assigned by OPNsense.
3. Safely appends a configuration block to your `~/.ssh/config`.
4. Automatically accepts new SSH host keys (`StrictHostKeyChecking accept-new`) to prevent errors when recreating containers.

After running, you can connect simply by typing: `ssh <alias>` (e.g., `ssh media-stack`).

---

## Phase 4: Container Synchronization (GitOps)

Inside each LXC container, the `node-sync.sh` script is used to keep the application state aligned with the Git repository. The initial bootstrap script automatically triggers this, but it can also be run manually or via a cron job to pull updates.

**What this script does:**

1. Pulls the latest changes from the `main` branch.
2. Transparently decrypts `.env` files via the Git SOPS smudge filter.
3. Finds and executes any `setup.sh` or `pre-sync.sh` scripts (useful for installing packages or setting permissions).
4. Recursively finds all `docker-compose.yml` or `compose.yaml` files for the specific stack.
5. Executes `docker compose up -d --remove-orphans` to apply declarative changes.

---

## Phase 5: Backup Strategy (Restic)

Backups are handled on the Proxmox host using Restic. The strategy is designed to protect fast SSD configurations by safely pausing active databases and saving snapshots to the larger, slower HDD storage.

### Running the Backup

Execute the backup script on the Proxmox host:

```bash
./scripts/host/proxmox-restic-backup.sh
```

**How the backup works:**

1. **Container Pausing:** The script dynamically finds all Docker containers across all running LXCs that have the label `com.homelab.backup.pause=true` and pauses them to prevent database corruption.
2. **Snapshot Creation:** Restic backs up the data directly to the repository (e.g., local NAS or cloud).
3. **Safety Traps:** A bash `trap` guarantees that containers are ALWAYS unpaused, even if the backup fails or is manually aborted.
4. **Retention Policy:** The script automatically prunes old backups (`--keep-daily 7 --keep-weekly 4 --keep-monthly 3`), ensuring your backup drive never fills up thanks to block-level deduplication.

---

## Directory Structure

```text
homelab/
├── .gitattributes          # Configures transparent SOPS encryption for Git
├── .sops.yaml              # Defines SOPS creation rules and Age public key
├── apps/                   # Contains individual application configurations (Docker Compose, .env)
│   └── <stack_name>/
│       └── <app_name>/
├── scripts/
│   ├── client/             # Scripts executed on the local workstation (Pop!_OS)
│   ├── host/               # Scripts executed on the Proxmox host
│   └── container/          # Scripts executed inside the LXC containers (e.g., node-sync.sh)
└── secrets/
    └── age.key.enc         # Encrypted Age private key
```
