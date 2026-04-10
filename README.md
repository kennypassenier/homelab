# Homelab GitOps Architecture

This repository contains the infrastructure as code (IaC) and GitOps configuration for managing a homelab environment using Proxmox, LXC containers, Docker, and transparent Git encryption (SOPS + Age).

## Architecture Overview

The goal of this setup is to manage all self-hosted applications centrally via this Git repository.

- **Security:** Secrets (`.env` files) are encrypted in the repository using Mozilla SOPS and Age. They are decrypted seamlessly on the local machine and inside the containers. _Note: Always keep your Age master private key safely backed up in a secure password manager (e.g., Bitwarden)._
- **Isolation:** Every application runs in its own unprivileged Proxmox LXC container.
- **Efficiency:** Containers use Git Sparse Checkouts to only download the files they need (their specific app directory and shared scripts).
- **Storage:** Fast-access app configurations (SSD) are stored in an isolated host directory (`/opt/appdata/<STACK_NAME>`) and bind-mounted into the containers at `/appdata`. Heavy media or backup data is separated onto other drives (e.g. `/HDD2TB` or NAS drives).
- **Networking:** IP assignments are handled centrally via OPNsense (Kea DHCP Reservations) using the container's MAC address.
- **Observability:** Centralized logging is supported via optional Promtail containers forwarding logs to a Loki/Grafana stack. Promtail configurations automatically expand environment variables, meaning the `LOKI_IP` is injected securely via `.env` files without manual config edits.

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

**Recommended LXC Specs (e.g., for the Gateway stack):**
- **CPU:** 2 Cores
- **RAM:** 1024 MB (1 GB)
- **Swap:** 512 MB
- **Disk:** 8 GB (App data is stored externally on the host bind-mount)

### 1. Generate the Stack Template Locally

From the root of your local repository, run the stack generator script. This interactive script lets you create multiple apps within a single stack, optionally configures a centralized Watchtower for automatic updates, optionally includes a Promtail container for centralized logging, and prepares `.env` files.

You can run it interactively:

```bash
./scripts/client/create-new-stack.sh
```

Or you can use CLI flags to bypass prompts for faster execution:

```bash
./scripts/client/create-new-stack.sh -d -w -p <stack_name>
```

- `-d`: Force use Docker without prompting.
- `-w`: Include centralized Watchtower (requires Docker).
- `-p`: Include centralized Promtail for Loki (requires Docker).
- `-h`: Show the help menu.

After generation, configure your `docker-compose.yml` and `.env` files (e.g., verifying the `LOKI_IP` in Promtail's `.env`), then push them to Git. SOPS will automatically encrypt the `.env` files.

### 2. Run the Bootstrap Script on Proxmox

Log into your Proxmox host via SSH and run the bootstrap script to provision the LXC container.

The script features an interactive wizard that will guide you through the process and lets you dynamically select the stack you want to deploy from a numbered list.

**Interactive Usage:**

```bash
./scripts/host/bootstrap-lxc.sh
```

**Automated Usage (Optional):**
To avoid typing your credentials and username every time, you can configure a `.env` file on your Proxmox host:

```bash
cp scripts/host/.env.example scripts/host/.env
nano scripts/host/.env # Fill in your GITHUB_USERNAME, GITHUB_PAT, and AGE_PASSPHRASE
```

Once configured, the script will automatically read these variables, leaving you to only pick the VMID and the stack!

**CLI Flags (For full automation):**
You can also bypass prompts entirely using flags (use `./scripts/host/bootstrap-lxc.sh -h` for all options):
```bash
./scripts/host/bootstrap-lxc.sh -v 101 -s media
```

**What this script does:**

1. Mounts the fast local NVMe host storage (`/opt/appdata/<STACK_NAME>`) to the container (`/appdata`) for isolated application configs.
2. Installs Docker, SOPS, Age, and `unattended-upgrades` (for automatic OS security patching) inside the LXC container.
3. Performs a Git Sparse Checkout using your GitHub PAT to only fetch the necessary app directory.
4. Decrypts the Age key using your passphrase and automatically decrypts your `.env` files.
5. Pulls your public SSH keys from GitHub so you can access the container without a password.
6. Outputs the MAC address of the new container for network configuration.

---

## Phase 3: Network Configuration (OPNsense & Local SSH)

Because DHCP and routing are handled outside of Proxmox, we use OPNsense to assign static IPs.

### 1. Reserve the IP in OPNsense

1. Copy the MAC address outputted at the end of the `bootstrap-lxc.sh` script.
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

Then, run the script (interactively or via flags):

```bash
./scripts/client/register-local-node.sh [-a <alias>] [-i <ip>] [-h]
```

**What this script does:**

1. Prompts you for a logical Host alias (e.g., `media-stack`).
2. Prompts you for the static IP assigned by OPNsense.
3. Safely appends a configuration block to your `~/.ssh/config`.
4. Automatically accepts new SSH host keys (`StrictHostKeyChecking accept-new`) to prevent errors when recreating containers.

After running, you can connect simply by typing: `ssh <alias>` (e.g., `ssh media-stack`).

---

## Phase 4: Container Synchronization (GitOps)

Inside each LXC container, the `node-sync.sh` script is used to keep the application state aligned with the Git repository. The bootstrap script automatically configures a **5-minute cronjob** to run this reconciliation loop, meaning your containers will automatically detect and apply Git changes. You can also run it manually for immediate results.

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
./scripts/host/backup-stacks.sh
```

**How the backup works:**

1. **Container Pausing:** The script dynamically finds all Docker containers across all running LXCs that have the label `com.homelab.backup.pause=true` and pauses them to prevent database corruption.
2. **Snapshot Creation:** Restic backs up the host configuration directory (`/opt/appdata`) directly to the dedicated 2TB backup repository (`/HDD2TB/backups/restic`).
3. **Safety Traps:** A bash `trap` guarantees that containers are ALWAYS unpaused, even if the backup fails or is manually aborted.
4. **Retention Policy:** The script automatically prunes old backups (`--keep-daily 7 --keep-weekly 4 --keep-monthly 3`), ensuring your backup drive never fills up thanks to block-level deduplication.

---

## Phase 6: Stack & App Lifecycle Management

Managing your homelab is primarily declarative via Git. Here are the common lifecycle use-cases:

### 1. Adding a New App to an Existing Stack

1. Navigate to your local Git repository.
2. Run the interactive generator script: `./scripts/client/create-new-app.sh`.
3. The script will prompt you for the stack and app name, generate the template, and prepare the `.env` file.
4. Configure your `docker-compose.yml` and `.env` files as needed.
5. Commit and push: `git add . && git commit -m "feat: add <app_name>" && git push`.
6. Wait up to 5 minutes for the automated GitOps cronjob to pull and deploy the app, or sync immediately by running `/opt/gitops/scripts/container/node-sync.sh <stack_name>` via SSH inside the LXC container.

### 2. Updating Apps

- **Automatic:** The centralized Watchtower container in each stack automatically pulls new images and restarts containers marked with the `com.centurylinklabs.watchtower.enable=true` label.
- **Declarative:** Modify your `docker-compose.yml` (e.g., change an environment variable) and push to Git. The container's 5-minute GitOps cronjob will automatically apply the new state.

### 3. Removing an App (Garbage Collection)

We use an automated Garbage Collection (GC) system. When an app is removed from Git, the 5-minute `node-sync.sh` cronjob will detect the orphaned data on the host, automatically stop the container, remove it, and cleanly delete the application's configuration data to prevent leftover artifacts.

1. Navigate to your local Git repository.
2. Run the removal script: `./scripts/client/remove-app.sh`.
3. Select the stack and app. The script will automatically delete the files, commit, and push.
4. Within 5 minutes, the GitOps cronjob will run its Garbage Collection routine on the LXC container, completely erasing the container and its `/opt/appdata/<stack_name>/<app_name>` directory.

### 4. Resetting a Corrupted Stack

If a stack is misbehaving and you want to start over without losing the LXC container, its static IP, or SSH keys, use the reset utility on the Proxmox host:

```bash
./scripts/host/reset-stack.sh <VMID> <STACK_NAME>
```

This script safely wipes all Docker containers, volumes, and local SSD app data (`/opt/appdata/<STACK_NAME>`). Afterwards, simply run `node-sync.sh` inside the LXC to cleanly rebuild the stack from your Git repository.

### 5. Destroying a Stack Entirely

If you no longer need a stack at all:

1. Delete the stack directory from your Git repository and push.
2. Manually stop and destroy the LXC container in the Proxmox Web GUI.
3. Open the Proxmox host shell and delete the leftover data: `rm -rf /opt/appdata/<STACK_NAME>`.

---

## Phase 7: Proxmox Host Management

While containers update automatically via GitOps, the Proxmox host itself also needs to keep its local utility scripts (`/scripts/host/*`) up to date. Furthermore, some hardware configurations (like GPU passthrough) must be handled at the host level rather than inside the unprivileged containers.

### 1. Automated Host Script Synchronization

To ensure the Proxmox host always runs the latest backup and deployment scripts from this repository, set up the automated host-sync cron job. This is an **idempotent** operation (safe to run multiple times).

Run this once on the Proxmox host:

```bash
./scripts/host/setup-cron.sh [-d <REPO_DIR>]
```

This configures `sync-host.sh` to run hourly, pulling the latest `main` branch into the host's repository safely.

### 2. Enabling Hardware GPU Passthrough

For media stacks (like Jellyfin) that require Intel/AMD hardware acceleration for video transcoding, the unprivileged LXC needs explicit permission to access `/dev/dri` on the host.

Instead of opening up hardware access to _all_ containers (which is a security risk), use the targeted, **idempotent** passthrough script on the Proxmox host:

```bash
./scripts/host/enable-gpu.sh [-h] <VMID>
```

**Safety & Recovery:** This script safely appends the required `cgroup2` permissions and bind mounts to `/etc/pve/lxc/<VMID>.conf`. It checks if the rules already exist to prevent duplicates. If a container fails to start or crashes after applying this, recovery is as simple as running `nano /etc/pve/lxc/<VMID>.conf` on the host, deleting the appended `lxc.cgroup2` and `lxc.mount` lines, and restarting the LXC.

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
