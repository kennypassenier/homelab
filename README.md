# Homelab GitOps Architecture

This repository contains the infrastructure as code (IaC) and GitOps configuration for managing a homelab environment using Proxmox, LXC containers, Docker, and transparent Git encryption (SOPS + Age).

## Architecture Overview

The goal of this setup is to manage all self-hosted applications centrally via this Git repository.

- **Security:** Secrets (`.env` files) are encrypted in the repository using Mozilla SOPS and Age. They are decrypted seamlessly on the local machine and inside the containers.
- **Isolation:** Every application runs in its own unprivileged Proxmox LXC container.
- **Efficiency:** Containers use Git Sparse Checkouts to only download the files they need (their specific app directory and shared scripts).
- **Storage:** Application data is stored centrally on the Proxmox host and bind-mounted into the LXC containers.
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

## Phase 2: Bootstrapping a New App (Proxmox Host)

When you want to deploy a new application, you create a new LXC container in Proxmox and run the bootstrap script.

### 1. Create App Directory in Git

Create a directory for your app (e.g., `apps/pihole`), add your `docker-compose.yml` and `.env` file, and push them to Git. SOPS will automatically encrypt the `.env` file.

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

1. Mounts host storage (`/mnt/storage/appdata/<APP_NAME>`) to the container (`/appdata`).
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

## Directory Structure

```text
homelab/
├── .gitattributes          # Configures transparent SOPS encryption for Git
├── .sops.yaml              # Defines SOPS creation rules and Age public key
├── apps/                   # Contains individual application configurations (Docker Compose, .env)
│   └── <app_name>/
├── scripts/
│   ├── client/             # Scripts executed on the local workstation (Pop!_OS)
│   ├── host/               # Scripts executed on the Proxmox host
│   └── container/          # Scripts executed inside the LXC containers (e.g., node-sync.sh)
└── secrets/
    └── age.key.enc         # Encrypted Age private key
```
