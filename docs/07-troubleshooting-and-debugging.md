# Part 7: Troubleshooting & Debugging

Even with a highly automated GitOps architecture, things can occasionally break. A bad configuration push, a missed environment variable, or incorrect file permissions can cause containers to crash or the synchronization loop to halt.

This guide provides step-by-step instructions for diagnosing and resolving the most common issues in your homelab.

---

## 1. GitOps Synchronization Issues

The `node-sync.sh` script runs every 5 minutes. If you push a change to Git and it does not reflect on your server after 5 minutes, the sync loop might be failing.

### How to Debug the Sync Loop
1. SSH into the affected LXC container (e.g., `ssh media`).
2. Run the container manager: `./container.sh`.
3. Select **`1. Trigger Node Sync`** and input the stack name.
4. Watch the live terminal output. The script will print exactly where it fails.

### Common Sync Errors
*   **Merge Conflicts:** If someone manually edited a file inside the container (violating the GitOps principle), Git will refuse to pull new changes to prevent overwriting local work.
    *   **The Fix:** Inside the container, force Git to match the remote repository:
        ```bash
        cd /opt/gitops
        git fetch --all
        git reset --hard origin/main
        ```
    *   *Prevention:* Never manually edit files inside `/opt/gitops/apps/...` on the server. Always use your local workstation and push via Git.
*   **Invalid Compose Syntax:** If you made a typo in your `docker-compose.yml`, `docker compose up` will throw a YAML parsing error and abort the deployment.
    *   **The Fix:** Fix the typo in your local Git repository on your workstation, commit, and push. Trigger the sync again.

## 2. SOPS & Age Decryption Failures

If your containers start but immediately crash because they cannot connect to databases or APIs, they might be missing their decrypted `.env` files.

### Symptoms
*   The `.env` file inside the container's app directory contains encrypted SOPS metadata instead of plaintext variables.
*   The `node-sync.sh` output shows: `Failed to decrypt file`.

### The Fix
1. **Check the Key:** Ensure the private Age key exists in the container at `/opt/gitops/secrets/age.key` and is in plaintext (starts with `AGE-SECRET-KEY-`).
2. **Bootstrap Failure:** If the key is missing or still encrypted, the bootstrap process likely failed to decrypt `secrets/age.key.enc` due to a wrong passphrase.
3. **Manual Recovery:** You can manually decrypt the key on the Proxmox host and inject it, or simply use the host manager (`./host.sh` -> **`4. Reset a corrupted Stack`**) and rebuild the LXC, ensuring you type the correct Age passphrase this time.

## 3. Container CrashLoopBackOff & Permissions

If a container is stuck in a restart loop, it is almost always a file permission issue. Since we use unprivileged LXC containers and bind-mount host directories (`/opt/appdata`), UID/GID mappings can be tricky.

### Diagnosing the Crash
1. SSH into the container.
2. Check the container status:
   ```bash
   docker ps -a
   ```
3. Read the logs of the crashing container:
   ```bash
   docker logs <container_name>
   ```

### Fixing "Permission Denied" Errors
If the logs show `EACCES: permission denied` or `database is locked`, the Docker container (which usually runs as an internal user) does not have write access to the host-mounted `/appdata` folder.

1. **Verify PUID/PGID:** Ensure your `docker-compose.yml` includes the correct environment variables telling the container to run as user `1000`.
   ```yaml
   environment:
     - PUID=1000
     - PGID=1000
   ```
2. **Fix Host Permissions:** On the Proxmox host, ensure the application's data directory is owned by the correct UID mapped to the unprivileged LXC. 
   If using the default `1000:1000` mapping inside the LXC, run this *inside the container*:
   ```bash
   chown -R 1000:1000 /appdata/<stack_name>/<app_name>
   ```
3. **Automate the Fix (Pre-Sync Hook):** If this permission issue happens repeatedly (e.g., during data migrations), create a `pre-sync.sh` file in the app's Git directory:
   ```bash
   #!/bin/bash
   chown -R 1000:1000 /appdata/media/seerr
   ```
   Make it executable (`chmod +x pre-sync.sh`) and push it. The sync loop will automatically fix permissions before starting the container.

## 4. Watchtower Update Failures

If an application is not automatically updating to the latest image version:

1. **Check Labels:** Ensure your `docker-compose.yml` has the correct Watchtower label:
   ```yaml
   labels:
     - "com.centurylinklabs.watchtower.enable=true"
   ```
2. **Check Watchtower Logs:** SSH into the container and check the central Watchtower logs:
   ```bash
   docker logs watchtower-<stack_name>
   ```
3. **Lifecycle Hooks:** If you are using pre-update scripts (like checking for active Jellyfin streams), Watchtower will skip the update if the script returns an error or if the stream check constantly returns `true`. Verify that your lifecycle hook scripts in `/opt/gitops/...` have the correct execution permissions.

---

**Next Steps:**
If your containers are running smoothly but you want to monitor their health, logs, and resource usage centrally, proceed to **[Part 8: Centralized Monitoring (Loki & Grafana)](08-centralized-monitoring.md)**.