# Phase 4: GitOps & Lifecycle Management

With your infrastructure bootstrapped and connected to the network, the day-to-day management of your homelab shifts almost entirely to Git. You rarely need to SSH into a server to make changes; instead, you declare your desired state in this repository and let the automated processes handle the rest.

This document outlines how the GitOps synchronization works and how to manage the lifecycle of your applications.

---

## 1. The Synchronization Loop (`node-sync.sh`)

Inside each LXC container, a synchronization script (`scripts/container/node-sync.sh`) runs automatically via a **cronjob every 5 minutes**. This script is the engine of our GitOps architecture. You can also trigger it manually at any time by SSHing into the container and running `./container.sh`.

### What happens during a sync?
1. **Git Pull:** The container pulls the latest changes from the `main` branch. Thanks to Git Sparse Checkouts, it only downloads its specific `stacks/<stack_name>` directory and the shared scripts.
2. **Transparent Decryption:** The Git SOPS smudge filter automatically decrypts any updated `.env` files using the Age key provisioned during bootstrap.
3. **Pre-Sync Hooks:** If the script finds a `pre-sync.sh` executable in the stack or app directories, it runs it. This is used for idempotent setup tasks, such as creating external Docker networks or fixing file permissions before containers start.
4. **Garbage Collection (GC):** The script compares the application directories present in Git with the data directories on the host (`/opt/appdata/<stack_name>`). If an app has been removed from Git, the GC routine automatically stops the container, removes it, and completely deletes the orphaned configuration data on the host.
5. **Deployment:** Finally, it recursively finds all `docker-compose.yml` files, runs `docker compose pull -q` to fetch updated images (if tags changed), and executes `docker compose up -d --remove-orphans` to apply the declarative state.

## 2. Adding and Updating Applications

### Adding a New App to an Existing Stack
1. On your local workstation, run `./client.sh`.
2. Select **`2. Create a new App inside a Stack`**.
3. Choose the target stack and provide an app name. The script generates a best-practice template and a `.env` file.
4. Configure your `docker-compose.yml` and `.env` as needed.
5. Commit and push the changes:
   ```bash
   git add stacks/<stack_name>/<app_name>
   git commit -m "feat(<stack_name>): add <app_name>"
   git push
   ```
6. Within 5 minutes, the container will detect the new folder, decrypt the `.env`, and spin up the new Docker container automatically.

### Updating Applications
*   **Configuration Updates (Declarative):** If you need to change a port, volume, or environment variable, simply edit the file locally, commit, and push. The 5-minute cronjob will apply the change.
*   **Software Updates (Automatic):** Standard image updates are handled automatically by **Watchtower**. Containers marked with the label `"com.centurylinklabs.watchtower.enable=true"` are monitored by a central Watchtower instance in each stack. Watchtower will pull the latest image and recreate the container. We also utilize lifecycle pre-hooks (e.g., preventing Jellyfin updates if a user is currently streaming) to ensure zero disruptive downtime.

## 3. Removing Apps and Stacks

Because of the aggressive Garbage Collection (GC) built into the sync script, removing an application is a destructive action that completely wipes host data. 

To prevent accidental deletion, we use interactive managers with strict guardrails.

### Removing an App
1. On your local workstation, run `./client.sh`.
2. Select **`3. Remove an App`**.
3. Select the stack and the specific app you wish to delete.
4. **Double Confirmation:** You will receive a red warning detailing exactly what will be destroyed. You must explicitly type the name of the app to confirm.
5. The script deletes the folder from Git, commits, and pushes.
6. The container's next cronjob will run GC: stopping the app, removing the Docker container, and wiping its `/opt/appdata` folder.

### Removing an Entire Stack
1. Run `./client.sh` and select **`4. Remove an entire Stack`**.
2. Follow the double-confirmation prompt (typing the stack name).
3. Once pushed, the GC will destroy *all* containers and data associated with that stack. You can then safely destroy the LXC container from the Proxmox Web GUI.

*(Note: While the scripts provide safety nets, you can achieve the exact same result by manually deleting the directories via Git and pushing the commit).*

## 4. Disaster Recovery: Resetting a Stack

If a stack becomes severely corrupted (e.g., a botched database upgrade) but you do not want to lose the LXC container, its static IP, or its SSH keys, you can perform a hard reset.

1. SSH into your Proxmox host.
2. Run `./host.sh` and select **`4. Reset a corrupted Stack`**.
3. Provide the VMID and the Stack name.

**What this does:** The script safely wipes all Docker containers, Docker volumes, and the local SSD application data (`/opt/appdata/<stack_name>`) for that specific LXC. Once complete, the container's standard `node-sync.sh` cronjob will pull a fresh copy from Git and rebuild the stack from scratch.

---

**Next Steps:**
To ensure your newly deployed data is safe and your Proxmox host remains up to date, proceed to **[Part 5: Backups & Host Management](05-backups-and-host-management.md)**.