# Phase 5: Backups & Host Management

While GitOps perfectly manages your application configurations and container states, you still have two critical responsibilities on the Proxmox host: safeguarding the persistent application data (databases, configs) and managing host-level hardware configurations (like GPU passthrough) and utility scripts.

This final document covers how to securely back up your fast NVMe storage and how to keep the Proxmox host synchronized with your repository.

---

## 1. Backup Strategy (Restic)

In this architecture, application configurations and databases are stored on the Proxmox host at `/opt/appdata/<STACK_NAME>` and bind-mounted into the LXC containers. We use **Restic** on the Proxmox host to back up these directories to a separate, larger, and slower storage drive (e.g., `/HDD2TB/backups/restic`).

### The Database Corruption Problem
Backing up a live database (like PostgreSQL or SQLite) while it is actively writing data can result in corrupted, unusable snapshots. 

### The Solution: Automated Pausing
To prevent corruption, we utilize a Docker label: `"com.homelab.backup.pause=true"`.
When you generate an app template via the `./client.sh` manager, this label is automatically added to the `docker-compose.yml`.

**How the backup script works:**
1. **Discovery:** The script dynamically queries all running LXC containers to find Docker containers carrying the backup pause label.
2. **Pause:** It temporarily pauses those specific containers (freezing their I/O operations).
3. **Snapshot:** Restic takes a fast, block-level deduplicated snapshot of the `/opt/appdata` directory.
4. **Resume (Trap-Secured):** A bash `trap` guarantees that the paused containers are ALWAYS unpaused immediately after the snapshot finishes—even if the backup fails, errors out, or is manually aborted (Ctrl+C).

### Running a Backup
1. SSH into your Proxmox host.
2. Run the host manager: `./host.sh`
3. Select **`2. Backup Stacks (Restic)`**.

The script will automatically handle the pausing, backup, unpausing, and will apply a retention policy (e.g., keeping 7 daily, 4 weekly, and 3 monthly backups) to ensure your backup drive never fills up unnecessarily.

## 2. Automated Host Script Synchronization

Your LXC containers automatically pull updates from Git every 5 minutes. However, the utility scripts residing on the Proxmox host itself (the `scripts/host/` directory and the `./host.sh` manager) also need to stay up to date.

Instead of manually running `git pull` on the host, you can configure an idempotent cronjob to keep the host repository synchronized.

1. Run `./host.sh` on the Proxmox server.
2. Select **`6. Setup Host Cronjob for automated sync`**.

This configures `sync-host.sh` to run hourly. It safely fetches the latest `main` branch into the host's repository, ensuring your backup and deployment scripts are always running the latest code.

## 3. Hardware GPU Passthrough

Some applications, such as Jellyfin or Plex in the `media` stack, require Intel/AMD hardware acceleration for efficient video transcoding. Because our LXC containers are unprivileged, they cannot access the host's hardware (like `/dev/dri`) by default.

Opening up hardware access globally to all containers is a major security risk. Instead, we use a targeted, **idempotent** script to grant specific LXCs access.

1. Ensure the LXC container you want to grant access to is stopped.
2. Run `./host.sh` on the Proxmox host.
3. Select **`3. Enable GPU Passthrough for an LXC`**.
4. Enter the VMID of the target container.

### Safety & Recovery
The script safely appends the required `cgroup2` permissions and bind mounts to the container's configuration file (`/etc/pve/lxc/<VMID>.conf`). It checks if the rules already exist to prevent duplicate entries.

If a container fails to start after applying this (e.g., due to different driver requirements), recovery is trivial:
1. Open the config file: `nano /etc/pve/lxc/<VMID>.conf`
2. Delete the appended `lxc.cgroup2` and `lxc.mount` lines at the bottom of the file.
3. Restart the LXC.

---

## Conclusion

You have reached the end of the Homelab GitOps Wiki! 

Your architecture is now fully documented: from securely encrypting secrets on your local machine (**Ground Zero**), to provisioning dynamic LXC containers (**Bootstrapping**), managing state automatically (**GitOps**), and safeguarding your persistent data (**Backups**). 

Always remember to follow the guidelines in `PHILOSOPHY.md` when expanding your homelab. Happy hosting!