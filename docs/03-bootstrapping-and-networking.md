# Phase 2 & 3: Bootstrapping and Networking

With your local workstation fully configured with SOPS and Age encryption (Ground Zero), it is time to provision your actual infrastructure. 

This phase covers how to dynamically create new applications locally, spin up isolated LXC containers on your Proxmox server, and link them to your network using OPNsense and local SSH configurations.

---

## 1. Generating a Stack Template (Local Workstation)

Before creating a container, you must define its configuration in Git. 

1. On your local machine (e.g., Linux desktop), open a terminal in the repository root.
2. Run the client manager:
   ```bash
   ./client.sh
   ```
3. Select **`1. Create a new Stack`**.

The interactive wizard will ask you for a stack name (e.g., `media`, `gateway`, `monitoring`). It will optionally configure a centralized Watchtower container (for automated updates) and a Promtail container (for forwarding logs to Loki). 

Once generated, review the `docker-compose.yml` and `.env` files in `apps/<stack_name>/`. Customize them as needed, then push them to GitHub. Your `.env` files will automatically be encrypted by SOPS.

```bash
git add apps/<stack_name>
git commit -m "feat: add <stack_name> stack"
git push
```

## 2. Bootstrapping the LXC (Proxmox Host)

Now that your configuration is securely stored in Git, you need to provision the server that will run it. 

Log into your Proxmox host via SSH. We will use the central host manager to build the LXC.

### Recommended LXC Specifications
Create a new unprivileged LXC container in the Proxmox Web GUI (do not start it yet) with standard resources:
- **CPU:** 2 Cores
- **RAM:** 1024 MB (1 GB)
- **Swap:** 512 MB
- **Disk:** 8 GB (Application data is stored externally on the host).

### Running the Bootstrap Wizard
Execute the host manager from the repository root on your Proxmox server:

```bash
./host.sh
```
Select **`1. Bootstrap a new LXC container`**.

The script is fully interactive. It will ask for the VMID of the container you just created and present a numbered list of all the Stacks currently available in your Git repository.

> **💡 Pro-Tip: Full Automation**
> To avoid typing your GitHub PAT (Personal Access Token) and Age passphrase every time you bootstrap a node, you can create a local `.env` file on the Proxmox host at `scripts/host/.env`. 
> 
> *Security Note:* This file is strictly ignored by Git (`.gitignore`) and the bootstrap script enforces `chmod 600` so only the root user can read it.

### What the Bootstrap Script Does Automatically:
1. **Storage Binding:** It mounts the fast NVMe host storage (`/opt/appdata/<STACK_NAME>`) directly into the container at `/appdata`.
2. **Provisioning:** It installs Docker, Docker Compose, SOPS, Age, and `unattended-upgrades` inside the unprivileged LXC.
3. **Sparse Checkout:** It uses your GitHub PAT to securely clone *only* the specific `apps/<STACK_NAME>` directory and the `scripts/` directory, saving disk space.
4. **Decryption:** It uses your Age passphrase to unlock the `secrets/age.key.enc` file, permanently enabling the container to decrypt its `.env` files.
5. **SSH Access:** It pulls your public SSH keys from GitHub so you can access the container seamlessly.
6. **MAC Address:** At the very end, it outputs the unique MAC address of the container. **Copy this address!**

## 3. Network Configuration (OPNsense)

By design, IP assignment is decoupled from Proxmox. We use OPNsense to manage static IP addresses.

1. Open your OPNsense router web interface.
2. Navigate to **Services -> Kea DHCP -> Reservations**.
3. Create a new reservation using the **MAC address** outputted by the bootstrap script.
4. Assign your desired static IP (e.g., `10.10.10.20`).
5. Go back to Proxmox and **restart the LXC container** so it requests the new static IP.

## 4. Local SSH Configuration (Local Workstation)

Once your container has a static IP, you want to be able to access it easily from your local machine without remembering the IP address or worrying about strict host key checking errors when destroying/recreating containers.

1. On your local machine, run the client manager again:
   ```bash
   ./client.sh
   ```
2. Select **`5. Register SSH alias for a new LXC container`**.

The interactive script will prompt you for an alias (e.g., `media`) and the static IP you just assigned in OPNsense.

### Idempotency & Safety
The script safely and idempotently parses your `~/.ssh/config` file. 
- If the alias already exists but has the wrong IP, it securely updates it without corrupting the rest of your file.
- It automatically injects `StrictHostKeyChecking accept-new`, preventing annoying terminal errors if you ever rebuild the container on the same IP.

You can now connect to your new server simply by typing:
```bash
ssh media
```

---

**Next Steps:**
Your server is running, securely connected, and initialized. To learn how the applications actually start and update themselves automatically, proceed to **[Part 4: GitOps & Lifecycle Management](04-gitops-and-lifecycle.md)**.