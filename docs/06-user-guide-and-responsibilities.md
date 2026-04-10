# Part 6: User Guide & Responsibilities

In a GitOps architecture, the line between human responsibility and machine automation is strictly defined. The goal is to minimize manual server administration so you can focus on simply defining what you want your homelab to look like.

This document serves as the definitive guide to your daily workflow, clearly mapping out **what you are required to do** (Your Responsibilities) versus **what the system handles for you** (Automated Processes).

---

## 1. The Core Principle: Git is the Interface

As a user, **you should almost never SSH into a container or host to change a configuration.** 
Your primary interface for interacting with the homelab is your local Git repository on your workstation (e.g., Linux desktop). 

If you want to change a port, update a secret, or add a new service, you make the change in the local repository, commit it, and push it to the remote branch. The system will automatically reconcile the server's state to match Git.

---

## 2. The User's Domain (What YOU do)

As the administrator of this homelab, your manual interactions are limited to the following specific areas:

### A. Infrastructure Configuration (Using the Managers)
You are responsible for generating templates and orchestrating high-level changes using the central manager scripts:
*   **`./client.sh` (Local Workstation):** Use this to initialize Ground Zero encryption, generate new Stack/App templates, safely remove Apps/Stacks (with double confirmation), and register new SSH aliases.
*   **`./host.sh` (Proxmox Server):** Use this to bootstrap brand new LXC containers, force a manual Restic backup, enable GPU passthrough, or reset a corrupted stack.
*   **`./container.sh` (Inside LXC):** Use this if you are impatient and want to manually trigger the GitOps sync instead of waiting for the 5-minute cronjob.

### B. Defining the State (Code & Secrets)
*   **Editing `docker-compose.yml`:** You must define the Docker images, ports, volumes, and labels for your stacks.
*   **Managing `.env` files:** You write your secrets into `.env` files locally. You are responsible for ensuring that you have initialized Ground Zero so that Git automatically encrypts these files via SOPS/Age before pushing.
*   **Safeguarding Keys:** You are solely responsible for securely storing your Age master passphrase in a password manager (e.g., Bitwarden). If you lose this, you lose access to all encrypted secrets.

### C. External Infrastructure (GUI Tasks)
The GitOps loop cannot control external hardware or hypervisor-level networking. You must manually:
1.  **Create the base LXC:** Go to the Proxmox Web GUI and create a new, unprivileged LXC container (assigning CPU, RAM, and base Disk space) before running `./host.sh`.
2.  **Assign Static IPs:** Go to your OPNsense router GUI and map the LXC's newly generated MAC address to a static IP via Kea DHCP Reservations.

---

## 3. The System's Domain (What AUTOMATION does)

Once you have pushed your changes to the `main` branch, the homelab's automation takes over completely. **Do not interfere with these processes manually.**

### A. The 5-Minute Reconciliation Loop
Every 5 minutes, a cronjob inside every LXC container wakes up and performs the following sequence without any human input:
1.  **Pulls** the latest Git commits using Sparse Checkout (only downloading what it needs).
2.  **Decrypts** any changed `.env` files automatically in memory using the Age key.
3.  **Executes** any `pre-sync.sh` scripts idempotently (e.g., creating Docker networks).
4.  **Deploys** the updated `docker-compose.yml` state (`docker compose up -d --remove-orphans`).

### B. Garbage Collection (Automated Destruction)
If you use `./client.sh` to remove an app or stack from Git, the automated Garbage Collection (GC) kicks in during the next sync loop:
*   It detects that the app exists on the host but not in Git.
*   It automatically **stops and removes** the Docker containers.
*   It automatically **wipes the physical data** from the Proxmox host's SSD (`/opt/appdata/<stack_name>/<app_name>`).

### C. Software Updates (Watchtower)
You do not need to manually pull new Docker images for routine software updates. 
*   The central **Watchtower** container in each stack scans for updates to running images.
*   If an update is found, it automatically downloads the image, gracefully stops the old container, and recreates it with the exact same configuration.
*   *Note:* It respects lifecycle hooks (e.g., it will abort updating Jellyfin if someone is currently streaming a movie).

### D. Safe Backups (Restic)
When the scheduled host cronjob runs the backup script (`backup-stacks.sh`):
*   It dynamically finds all containers marked with the `"com.homelab.backup.pause=true"` label.
*   It automatically **pauses** them to prevent database corruption.
*   It takes a block-level deduplicated snapshot to your HDD storage.
*   It automatically **unpauses** the containers, even if the backup encounters an error.

---

## 4. Day-to-Day Scenarios (Quick Reference)

Here is exactly how you handle common tasks, illustrating the split between your actions and system automation:

### Scenario 1: Deploying a Brand New App
1.  **User:** Run `./client.sh` -> *Create a new App inside a Stack*.
2.  **User:** Edit the generated `docker-compose.yml` and `.env` to suit your needs.
3.  **User:** `git add .`, `git commit -m "feat: add app"`, and `git push`.
4.  **System:** Waits 0-5 minutes. Pulls the code, decrypts the `.env`, and starts the container.

### Scenario 2: Changing an Environment Variable
1.  **User:** Open the `.env` file locally (it is automatically decrypted for you by the Git clean filter).
2.  **User:** Change the value.
3.  **User:** `git add .`, `git commit`, `git push`. (Git encrypts it via the smudge filter).
4.  **System:** Pulls the new encrypted file, decrypts it on the node, notices the config change, and recreates the specific Docker container.

### Scenario 3: Deleting an App
1.  **User:** Run `./client.sh` -> *Remove an App*.
2.  **User:** Read the red warning, and provide the double confirmation by typing the app's name. The script commits and pushes the deletion.
3.  **System:** The GC detects the missing Git folder, kills the container, and permanently wipes the application data from the Proxmox host.

### Scenario 4: Investigating a Broken App
If an app isn't working after a push:
1.  **User:** SSH into the container (e.g., `ssh media`).
2.  **User:** Run `./container.sh` -> *Trigger Node Sync* to force the loop and read the live output for errors (e.g., invalid compose syntax).
3.  **User:** Check container logs (`docker logs <container_name>`).
4.  **User:** *Do not fix the error in the container.* Go back to your local workstation, fix the code in Git, and push again.

---

## 5. Maintenance Checklist

While day-to-day operations are automated, you are responsible for the following periodic maintenance:

- [ ] **Proxmox Host Updates:** Periodically log into the Proxmox Web GUI or SSH into the host to run `apt update && apt upgrade` to keep the hypervisor secure.
- [ ] **OPNsense Updates:** Keep your router firmware updated.
- [ ] **Age Key Backup:** Verify at least twice a year that you still know and have access to your Age master passphrase.
- [ ] **Host Script Sync:** Ensure `./host.sh` -> *Setup Host Cronjob* was run at least once so the host automatically updates its utility scripts from Git.