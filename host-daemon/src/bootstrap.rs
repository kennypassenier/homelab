use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::provision::StackIntent;

fn default_host_gitops_repo() -> String {
    std::env::var("GITOPS_REPO").unwrap_or_else(|_| {
        std::env::var("HOME")
            .map(|home| format!("{}/homelab", home))
            .unwrap_or_else(|_| "/root/homelab".to_string())
    })
}

fn default_host_env_file() -> String {
    format!("{}/host-daemon/.env", default_host_gitops_repo())
}

#[derive(Debug, Clone)]
pub struct BootstrapResult {
    #[allow(dead_code)]
    pub success: bool,
    pub duration: Duration,
    #[allow(dead_code)]
    pub error: Option<String>,
}

/// Bootstrap a newly created LXC container with all necessary configuration
pub fn bootstrap_lxc(vmid: u32, intent: &StackIntent) -> Result<BootstrapResult, String> {
    let start_time = std::time::Instant::now();

    println!("Bootstrapping LXC {} for stack {}", vmid, intent.stack_name);

    // Stop container for configuration
    pct_stop(vmid)?;

    // Storage
    setup_storage(vmid, intent)?;

    // Hardware (TUN device)
    if intent.tun_device.unwrap_or(false) {
        setup_tun_device(vmid)?;
    }

    // Hardware (GPU passthrough)
    if intent.gpu_passthrough.unwrap_or(false) {
        setup_gpu_passthrough(vmid)?;
    }

    // Start container
    pct_start(vmid)?;
    wait_for_container_ready(vmid, Duration::from_secs(30))?;

    // Create directories
    create_appdata_directories(vmid, &intent.stack_name)?;

    // Inject secrets (LATCH_ only)
    inject_secrets(vmid)?;

    // Install dependencies
    install_dependencies(vmid)?;

    // Git setup
    setup_git_sparse_checkout(vmid, &intent.stack_name)?;

    // SSH access
    let github_username =
        std::env::var("GITHUB_USERNAME").unwrap_or_else(|_| "kennypassenier".to_string());
    setup_ssh_access(vmid, &github_username)?;

    // Install LXC daemon
    install_lxc_daemon(vmid)?;
    create_daemon_config(vmid, &intent.stack_name)?;

    let duration = start_time.elapsed();

    Ok(BootstrapResult {
        success: true,
        duration,
        error: None,
    })
}

/// Stop LXC container
fn pct_stop(vmid: u32) -> Result<(), String> {
    let output = Command::new("pct")
        .arg("stop")
        .arg(vmid.to_string())
        .output()
        .map_err(|e| format!("Failed to execute pct stop: {}", e))?;

    if !output.status.success() {
        // Container might already be stopped, that's ok
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("not running") {
            return Err(format!("pct stop failed: {}", stderr));
        }
    }

    Ok(())
}

/// Start LXC container
fn pct_start(vmid: u32) -> Result<(), String> {
    let output = Command::new("pct")
        .arg("start")
        .arg(vmid.to_string())
        .output()
        .map_err(|e| format!("Failed to execute pct start: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "pct start failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Execute command inside LXC container
fn pct_exec(vmid: u32, command: &str) -> Result<String, String> {
    let output = Command::new("pct")
        .arg("exec")
        .arg(vmid.to_string())
        .arg("--")
        .arg("bash")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| format!("Failed to execute pct exec: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Wait for container to be ready for commands
fn wait_for_container_ready(vmid: u32, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            return Err(format!(
                "Container {} did not become ready within {:?}",
                vmid, timeout
            ));
        }

        // Try to run a simple command
        if pct_exec(vmid, "echo ready").is_ok() {
            return Ok(());
        }

        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Configure storage bind mount
fn setup_storage(_vmid: u32, intent: &StackIntent) -> Result<(), String> {
    let host_path = &intent.host_storage_path;
    let mount_point = &intent.mount_point;

    // Create host directory with correct ownership for unprivileged containers
    std::fs::create_dir_all(host_path)
        .map_err(|e| format!("Failed to create host storage path: {}", e))?;

    if intent.unprivileged {
        // Unprivileged containers use UID/GID offset of 100000
        let output = Command::new("chown")
            .args(["-R", "100000:100000", host_path])
            .output()
            .map_err(|e| format!("Failed to chown storage path: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "chown failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    println!("Storage configured: {} -> {}", host_path, mount_point);

    Ok(())
}

/// Enable TUN device passthrough for VPN containers
fn setup_tun_device(vmid: u32) -> Result<(), String> {
    let conf_file = format!("/etc/pve/lxc/{}.conf", vmid);
    let conf_content = std::fs::read_to_string(&conf_file)
        .map_err(|e| format!("Failed to read LXC config: {}", e))?;

    // Check if already configured
    if conf_content.contains("lxc.cgroup2.devices.allow: c 10:200") {
        println!("TUN device already configured for LXC {}", vmid);
        return Ok(());
    }

    // Verify host has TUN
    if !Path::new("/dev/net/tun").exists() {
        return Err("/dev/net/tun not found on host. Run: modprobe tun".to_string());
    }

    // Append configuration
    let tun_config = r#"
# --- TUN Device Passthrough (auto-configured) ---
lxc.cgroup2.devices.allow: c 10:200 rwm
lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file
"#;

    let mut file = OpenOptions::new()
        .append(true)
        .open(&conf_file)
        .map_err(|e| format!("Failed to open LXC config for writing: {}", e))?;

    file.write_all(tun_config.as_bytes())
        .map_err(|e| format!("Failed to write TUN config: {}", e))?;

    println!("TUN device passthrough configured for LXC {}", vmid);
    Ok(())
}

/// Enable Intel/AMD GPU passthrough for media/transcoding containers
fn setup_gpu_passthrough(vmid: u32) -> Result<(), String> {
    let conf_file = format!("/etc/pve/lxc/{}.conf", vmid);
    let conf_content = std::fs::read_to_string(&conf_file)
        .map_err(|e| format!("Failed to read LXC config: {}", e))?;

    // Idempotency: skip if DRM cgroup entry already present
    if conf_content.contains("lxc.cgroup2.devices.allow: c 226:") {
        println!("GPU passthrough already configured for LXC {}", vmid);
        return Ok(());
    }

    // Verify DRM devices exist on host
    if !Path::new("/dev/dri/card0").exists() {
        return Err(
            "/dev/dri/card0 not found on host — no GPU available for passthrough".to_string(),
        );
    }

    let gpu_config = r#"
# --- GPU Passthrough (auto-configured) ---
# 226 is the DRM major device number
lxc.cgroup2.devices.allow: c 226:0 rwm
lxc.cgroup2.devices.allow: c 226:128 rwm
lxc.mount.entry: /dev/dri/card0 dev/dri/card0 none bind,optional,create=file
lxc.mount.entry: /dev/dri/renderD128 dev/dri/renderD128 none bind,optional,create=file
"#;

    let mut file = OpenOptions::new()
        .append(true)
        .open(&conf_file)
        .map_err(|e| format!("Failed to open LXC config for writing: {}", e))?;

    file.write_all(gpu_config.as_bytes())
        .map_err(|e| format!("Failed to write GPU config: {}", e))?;

    println!("GPU passthrough configured for LXC {}", vmid);
    Ok(())
}

/// Create /appdata directory inside container
fn create_appdata_directories(vmid: u32, _stack_name: &str) -> Result<(), String> {
    pct_exec(vmid, "mkdir -p /appdata")?;
    pct_exec(vmid, "chmod 755 /appdata")?;

    println!("Created /appdata directory in LXC {}", vmid);
    Ok(())
}

/// Inject LATCH_* secrets from HOST environment
fn inject_secrets(vmid: u32) -> Result<(), String> {
    // Look for env file in multiple locations
    let host_env_file = std::env::var("HOST_ENV_FILE").ok();
    let default_env_file = default_host_env_file();
    let possible_paths = vec![
        host_env_file.as_deref().unwrap_or(""),
        default_env_file.as_str(),
        "host-daemon/.env",
        "/root/.env",
    ];

    let env_file = possible_paths
        .iter()
        .find(|p| !p.is_empty() && Path::new(p).exists());

    let Some(env_file) = env_file else {
        println!("No HOST env file found, skipping secrets injection");
        return Ok(());
    };

    // Extract only LATCH_ variables
    let content =
        std::fs::read_to_string(env_file).map_err(|e| format!("Failed to read env file: {}", e))?;

    let latch_vars: Vec<&str> = content
        .lines()
        .filter(|line| line.starts_with("LATCH_"))
        .collect();

    if latch_vars.is_empty() {
        println!("No LATCH_ variables found");
        return Ok(());
    }

    let secrets_content = latch_vars.join("\n");

    // Write to container /root/.env
    let temp_file = format!("/tmp/lxc-secrets-{}", vmid);
    std::fs::write(&temp_file, &secrets_content)
        .map_err(|e| format!("Failed to write temp secrets file: {}", e))?;

    // Push to container
    let output = Command::new("pct")
        .args(["push", &vmid.to_string(), &temp_file, "/root/.env"])
        .output()
        .map_err(|e| format!("Failed to push secrets: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to push secrets: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Also append to /etc/environment so latch can run non-interactively.
    pct_exec(
        vmid,
        "bash -c 'if [ -f /root/.env ]; then grep ^LATCH_ /root/.env >> /etc/environment 2>/dev/null || true; fi'",
    )?;

    // Cleanup
    std::fs::remove_file(&temp_file).ok();

    println!("Secrets injected into LXC {}", vmid);
    Ok(())
}

/// Install dependencies (Docker, Latch wrapper, Git, etc.)
fn install_dependencies(vmid: u32) -> Result<(), String> {
    println!("Installing dependencies in LXC {}...", vmid);

    let install_script = r#"
set -euo pipefail

# Update package lists
apt-get update -qq

# Install base packages
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    curl git wget openssl jq unattended-upgrades ca-certificates

# Configure unattended upgrades
dpkg-reconfigure -f noninteractive unattended-upgrades

# Install Docker
if ! command -v docker &> /dev/null; then
    curl -fsSL https://get.docker.com | sh
    systemctl enable docker
    systemctl start docker
fi

# Install Latch CLI wrapper backed by the official container image.
if ! command -v latch &> /dev/null; then
    cat > /usr/local/bin/latch <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

exec docker run --rm -i \
    -v "$PWD:$PWD" \
    -w "$PWD" \
    -e LATCH_KEY \
    -e LATCH_PAT \
    -e RUST_LOG \
    ghcr.io/kennypassenier/latch-rs:latest "$@"
EOF
    chmod +x /usr/local/bin/latch
fi

echo "Dependencies installed successfully"
"#;

    pct_exec(vmid, install_script)?;

    println!("Dependencies installed in LXC {}", vmid);
    Ok(())
}

/// Setup Git sparse checkout for the stack
fn setup_git_sparse_checkout(vmid: u32, stack_name: &str) -> Result<(), String> {
    let github_pat = std::env::var("GITHUB_PAT")
        .or_else(|_| std::env::var("GITOPS_REPO_TOKEN"))
        .map_err(|_| "GITHUB_PAT or GITOPS_REPO_TOKEN not set in HOST environment".to_string())?;

    let repo_url = std::env::var("GITOPS_REPO_URL")
        .unwrap_or_else(|_| "https://github.com/kennypassenier/homelab.git".to_string());

    // Extract repo path from URL
    let repo_path = repo_url
        .strip_prefix("https://github.com/")
        .or_else(|| repo_url.strip_prefix("https://"))
        .unwrap_or(&repo_url);

    let setup_script = format!(
        r#"
set -euo pipefail

GITOPS_DIR="/opt/gitops"
STACK_NAME="{}"
REPO_PATH="{}"
GITHUB_PAT="{}"

# Remove existing gitops dir if present
rm -rf $GITOPS_DIR

# Clone with PAT authentication
git clone --filter=blob:none --no-checkout \
    https://${{GITHUB_PAT}}@${{REPO_PATH}} $GITOPS_DIR

cd $GITOPS_DIR

# Configure sparse checkout
git sparse-checkout init --cone
git sparse-checkout set stacks/${{STACK_NAME}}

# Checkout main branch
git checkout main

# Store PAT for future pulls
git config credential.helper store
echo "https://${{GITHUB_PAT}}@${{REPO_PATH}}" > ~/.git-credentials
chmod 600 ~/.git-credentials

echo "Sparse checkout completed for stack: ${{STACK_NAME}}"
"#,
        stack_name, repo_path, github_pat
    );

    pct_exec(vmid, &setup_script)?;

    println!("Git sparse checkout configured for LXC {}", vmid);
    Ok(())
}

/// Setup SSH access from GitHub user
fn setup_ssh_access(vmid: u32, github_username: &str) -> Result<(), String> {
    let setup_script = format!(
        r#"
set -euo pipefail

GITHUB_USER="{}"

# Create .ssh directory
mkdir -p /root/.ssh
chmod 700 /root/.ssh

# Fetch GitHub SSH keys
curl -fsSL "https://github.com/${{GITHUB_USER}}.keys" > /root/.ssh/authorized_keys
chmod 600 /root/.ssh/authorized_keys

echo "SSH access configured for GitHub user: ${{GITHUB_USER}}"
"#,
        github_username
    );

    pct_exec(vmid, &setup_script)?;

    println!(
        "SSH access configured for LXC {} (GitHub user: {})",
        vmid, github_username
    );
    Ok(())
}

/// Install LXC daemon binary
fn install_lxc_daemon(vmid: u32) -> Result<(), String> {
    println!("Installing LXC daemon in LXC {}...", vmid);

    // Strategy 1: Try to pull the LXC daemon Docker image (preferred)
    if let Ok(lxc_daemon_image) = std::env::var("LXC_DAEMON_IMAGE") {
        let image = format!("{}:latest", lxc_daemon_image);
        println!("Attempting to pull LXC daemon image: {}", image);

        let docker_pull = format!(
            r#"
if docker pull {} &>/dev/null; then
    docker run --rm {} tar xOf /usr/local/bin/LXC > /tmp/lxc-daemon
    mv /tmp/lxc-daemon /usr/local/bin/lxc-daemon
    chmod +x /usr/local/bin/lxc-daemon
    echo "LXC daemon extracted from docker image successfully"
else
    echo "Failed to pull docker image"
    exit 1
fi
"#,
            image, image
        );

        if pct_exec(vmid, &docker_pull).is_ok() {
            println!("LXC daemon installed from docker image");
            return Ok(());
        }
        println!("Docker image pull failed, falling back to binary method");
    }

    // Strategy 2: Copy binary from HOST build artifacts
    let binary_paths = vec![
        format!("{}/apps/LXC", default_host_gitops_repo()),
        format!(
            "{}/apps/LXC-linux-x86_64-unknown-linux-gnu",
            default_host_gitops_repo()
        ),
        format!(
            "{}/lxc-daemon/target/release/LXC",
            default_host_gitops_repo()
        ),
        "/opt/homelab/lxc-daemon/target/release/LXC".to_string(),
        "apps/LXC-linux-x86_64-unknown-linux-gnu".to_string(),
        "lxc-daemon/target/release/LXC".to_string(),
    ];

    for binary_path in binary_paths {
        if Path::new(&binary_path).exists() {
            println!("Found LXC daemon binary at: {}", binary_path);
            let output = Command::new("pct")
                .args([
                    "push",
                    &vmid.to_string(),
                    &binary_path,
                    "/usr/local/bin/lxc-daemon",
                ])
                .output()
                .map_err(|e| format!("Failed to push LXC daemon: {}", e))?;

            if !output.status.success() {
                return Err(format!(
                    "Failed to push LXC daemon: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            pct_exec(vmid, "chmod +x /usr/local/bin/lxc-daemon")?;
            println!("LXC daemon installed from binary");
            return Ok(());
        }
    }

    // Strategy 3: Fallback to placeholder (should not reach in production)
    println!("Warning: LXC daemon binary not found in standard locations");
    println!("Creating placeholder - build and deploy the actual daemon from `make release-lxc`");

    pct_exec(
        vmid,
        r#"
        mkdir -p /usr/local/bin
        echo '#!/bin/bash' > /usr/local/bin/lxc-daemon
        echo 'echo "LXC daemon placeholder - replace with actual binary or docker image"' >> /usr/local/bin/lxc-daemon
        echo 'sleep infinity' >> /usr/local/bin/lxc-daemon
        chmod +x /usr/local/bin/lxc-daemon
    "#,
    )?;

    println!("LXC daemon installed (placeholder - update required)");
    Ok(())
}

/// Create LXC daemon configuration file
fn create_daemon_config(vmid: u32, stack_name: &str) -> Result<(), String> {
    let config_content = format!(
        r#"[sync]
interval_seconds = 1800
gitops_repo = "/opt/gitops"
stack_name = "{}"

[git]
remote = "origin"
branch = "main"
sparse_checkout = true

[logging]
format = "logfmt"
output = "/var/log/lxc-daemon-sync.log"
level = "info"

[api]
listen = "0.0.0.0:8080"
auth_token_env = "LXC_API_TOKEN"
"#,
        stack_name
    );

    // Create config directory
    pct_exec(vmid, "mkdir -p /etc/homelab")?;

    // Write config
    let temp_file = format!("/tmp/lxc-daemon-config-{}.toml", vmid);
    std::fs::write(&temp_file, config_content)
        .map_err(|e| format!("Failed to write temp config: {}", e))?;

    let output = Command::new("pct")
        .args([
            "push",
            &vmid.to_string(),
            &temp_file,
            "/etc/homelab/lxc-daemon.toml",
        ])
        .output()
        .map_err(|e| format!("Failed to push config: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to push config: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    std::fs::remove_file(&temp_file).ok();

    // Create systemd service
    let service_content = r#"[Unit]
Description=Homelab LXC GitOps Daemon
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
EnvironmentFile=-/root/.env
ExecStart=/usr/local/bin/lxc-daemon --config /etc/homelab/lxc-daemon.toml
Restart=always
RestartSec=10
StandardOutput=append:/var/log/lxc-daemon.log
StandardError=append:/var/log/lxc-daemon.log

[Install]
WantedBy=multi-user.target
"#;

    let temp_service = format!("/tmp/lxc-daemon-service-{}", vmid);
    std::fs::write(&temp_service, service_content)
        .map_err(|e| format!("Failed to write temp service: {}", e))?;

    let output = Command::new("pct")
        .args([
            "push",
            &vmid.to_string(),
            &temp_service,
            "/etc/systemd/system/lxc-daemon.service",
        ])
        .output()
        .map_err(|e| format!("Failed to push service: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to push service: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    std::fs::remove_file(&temp_service).ok();

    // Enable and start service
    pct_exec(vmid, "systemctl daemon-reload")?;
    pct_exec(vmid, "systemctl enable lxc-daemon")?;
    pct_exec(vmid, "systemctl start lxc-daemon")?;

    println!("LXC daemon service configured and started in LXC {}", vmid);
    Ok(())
}
