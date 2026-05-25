# Fase 3: De HOST Daemon (Tier 2)
**Doel:** Native Rust binary (systemd) op de Proxmox host. Beheert de API-communicatie voor LXC creatie, configuratie (GPU/TUN) via atomaire file-writes, en de Restic back-up coördinatie met de LXC daemons.

## Stap 3.1: Project Set-up & Dependencies
**Copilot Prompt:**
`I am building a Rust-based Proxmox Host Daemon (Tier 2).
Task 1: Initialize a new cargo project named host-daemon.
Task 2: Add crates: ratatui, crossterm, tokio, reqwest, serde, serde_json, toml, anyhow, color-eyre, and tempfile.
Task 3: Write main.rs setting up a Ratatui TUI with a color-eyre panic hook for disable_raw_mode().
Task 4: Implement a TUI loop with Tabs: Dashboard, Backups, Settings.`

## Stap 3.2: LXC Provisioning (Proxmox API) & Post-Provisioning Hook
**Copilot Prompt:**
`We need to implement LXC provisioning via Proxmox REST API with a Post-Provisioning Hook.
Task 1: Create a ProxmoxClient struct holding API URL, Node, and Token (via reqwest).
Task 2: Write provision_lxc(config: &StackConfig) -> Result<(), anyhow::Error>. 
Task 3: Send a POST to /api2/json/nodes/{node}/lxc cloning a Debian template, passing mac_address to net0 to prevent DHCP collisions. Wait for completion.
Task 4: POST-PROVISIONING HOOK: Use the Proxmox Exec API (POST /api2/json/nodes/{node}/lxc/{vmid}/exec) to run: "apt-get update && apt-get upgrade -y && apt-get install -y unattended-upgrades curl". Then run the Docker installation script.
Task 5: If the Exec hook fails (non-zero exit), delete the LXC immediately and return a critical error (Atomic Transaction).`

## Stap 3.3: Hardware Passthrough (Atomaire Parser)
**Copilot Prompt:**
`We need to implement hardware passthrough by safely modifying Proxmox LXC config files.
Task 1: Create enable_hardware_passthrough(vmid: u32, needs_gpu: bool, needs_tun: bool).
Task 2: Read /etc/pve/lxc/{vmid}.conf. Idempotently check for lxc.cgroup2.devices.allow lines (c 226:128 for GPU, c 10:200 for TUN).
Task 3: CRITICAL: Write modified config to a temporary file using the tempfile crate. 
Task 4: Use std::fs::rename to atomically overwrite the original {vmid}.conf file, preventing corruption on crash.`

## Stap 3.4: Back-up Orchestratie (Strikte Tier Scheiding)
**Copilot Prompt:**
`We need a Restic orchestrator communicating with Tier 3 LXC daemons via HTTP.
Task 1: Create run_backup_cycle(lxc_ips: Vec<String>).
Task 2: Step 1 (Pause): Iterate over lxc_ips and POST to http://{ip}:8080/api/backup/pause. Await all 200 OKs.
Task 3: Step 2 (Backup): Use tokio::process::Command to execute "restic backup /opt/appdata --cleanup-cache".
Task 4: Step 3 (Resume): CRITICAL RULE: Use a Drop guard (or defer) to GUARANTEE a POST to http://{ip}:8080/api/backup/resume for all IPs, even if Restic failed or panicked.`

## Stap 3.5: CI/CD Self-Updater
**Copilot Prompt:**
`Implement a self-updater downloading compiled binaries from GitHub Releases.
Task 1: Create update_self(repo_url: &str, current_version: &str).
Task 2: Call GitHub API to check for newer releases.
Task 3: Download the binary asset to /tmp/, make executable.
Task 4: Use self_replace::self_replace (or atomic std::fs::rename) to overwrite the running binary.
Task 5: Initiate a systemd restart command (systemctl restart host-daemon) and exit.`

## Stap 3.6: Genereer Documentatie
**Prompt 1 (Menselijke Wiki):**
`Write docs/wiki/host-daemon.md. Document LXC provisioning, MAC collision prevention, the Post-Provisioning Hook (apt-get/docker), the Hybrid Hardware Passthrough atomic parser, Backup Orchestrator API pauses, and the GitHub Releases self-update.`
**Prompt 2 (LLM Context):**
`Write docs/LLM_CONTEXT_HOST_DAEMON.md. Emphasize "Strict Tier Separation" (no pct exec except for the initial bootstrap hook). Detail the atomic rename for .conf files and the fail-safe Drop guards for Restic resumes.`
