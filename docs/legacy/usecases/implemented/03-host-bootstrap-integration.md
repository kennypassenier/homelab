# Use Case: HOST Bootstrap Integration (Eliminate bootstrap-lxc.sh)

**Tier:** HOST  
**Status:** ✅ Implemented  
**Priority:** High  
**Completed:** June 4, 2025  
**Dependencies:** 01-host-automated-lxc-provisioning.md, 02-lxc-daemon-gitops-sync.md

---

## Implementation Note

⚠️ **Before implementing**: Audit existing code to determine what bootstrap logic is already integrated into HOST daemon. Much of this functionality may already exist.

---

## Problem Statement

Currently, after HOST creates an LXC container (via `01-host-automated-lxc-provisioning`), the container needs to be bootstrapped with:
- Storage configuration
- TUN device passthrough (if needed)
- Directory creation
- Secrets injection
- Dependency installation (Docker, Infisical, etc.)
- Git sparse checkout
- SSH key setup
- LXC daemon installation

`bootstrap-lxc.sh` does this, but it's:
- A separate manual step
- Shell script instead of Rust
- Not integrated with HOST daemon
- Inconsistent error handling

---

## Desired Behavior

HOST daemon should automatically bootstrap newly created LXC containers as part of the provisioning flow:

1. **Create LXC** (from `01-host-automated-lxc-provisioning`)
2. **Bootstrap LXC** (this use case) — all setup automated
3. **Start LXC daemon** (from `02-lxc-daemon-gitops-sync`)
4. **GitOps sync begins automatically**

The entire flow should be atomic: "commit lxc-compose.yml → push → HOST provisions and bootstraps".

---

## Technical Requirements

### HOST Daemon Changes

**New Module**: `host-daemon/src/bootstrap.rs`

Functions:
- `bootstrap_lxc(vmid: u32, intent: &StackIntent) -> Result<BootstrapResult>`
- `setup_storage(vmid: u32, intent: &StackIntent) -> Result<()>`
- `setup_tun_device(vmid: u32, intent: &StackIntent) -> Result<()>`
- `inject_secrets(vmid: u32) -> Result<()>`
- `install_dependencies(vmid: u32) -> Result<()>`
- `setup_git_sparse_checkout(vmid: u32, stack_name: &str) -> Result<()>`
- `setup_ssh_access(vmid: u32, github_username: &str) -> Result<()>`
- `install_lxc_daemon(vmid: u32) -> Result<()>`
- `create_daemon_config(vmid: u32, stack_name: &str) -> Result<()>`

### Bootstrap Flow

```rust
impl HostDaemon {
    pub fn provision_and_bootstrap(&self, intent: &StackIntent) -> Result<()> {
        // Phase 1: Create LXC (from use case 01)
        if !self.lxc_exists(intent.vmid)? {
            self.create_lxc(intent)?;
        }
        
        // Phase 2: Bootstrap (this use case)
        let bootstrap_result = self.bootstrap_lxc(intent.vmid, intent)?;
        
        if !bootstrap_result.success {
            return Err(anyhow!("Bootstrap failed: {}", bootstrap_result.error));
        }
        
        // Phase 3: Start LXC daemon service
        self.start_lxc_daemon(intent.vmid)?;
        
        Ok(())
    }
    
    fn bootstrap_lxc(&self, vmid: u32, intent: &StackIntent) -> Result<BootstrapResult> {
        info!("Bootstrapping LXC {} for stack {}", vmid, intent.stack_name);
        
        // Stop container for configuration
        self.pct_stop(vmid)?;
        
        // Storage
        self.setup_storage(vmid, intent)?;
        
        // Hardware (TUN, GPU)
        if intent.hardware.tun_device {
            self.setup_tun_device(vmid, intent)?;
        }
        if intent.hardware.gpu.enabled {
            self.setup_gpu_passthrough(vmid, intent)?;
        }
        
        // Start container
        self.pct_start(vmid)?;
        self.wait_for_container_ready(vmid, Duration::from_secs(30))?;
        
        // Create directories
        self.create_appdata_directories(vmid, &intent.stack_name)?;
        
        // Inject secrets (INFISICAL_ only)
        self.inject_secrets(vmid)?;
        
        // Install dependencies
        self.install_dependencies(vmid)?;
        
        // Git setup
        self.setup_git_sparse_checkout(vmid, &intent.stack_name)?;
        
        // SSH access
        let github_username = std::env::var("GITHUB_USERNAME")
            .unwrap_or_else(|_| "kennypassenier".to_string());
        self.setup_ssh_access(vmid, &github_username)?;
        
        // Install LXC daemon
        self.install_lxc_daemon(vmid)?;
        self.create_daemon_config(vmid, &intent.stack_name)?;
        
        // Cleanup
        self.cleanup_bootstrap_artifacts(vmid)?;
        
        Ok(BootstrapResult {
            success: true,
            duration: Duration::from_secs(45),
            error: None,
        })
    }
}
```

### Storage Setup

```rust
fn setup_storage(&self, vmid: u32, intent: &StackIntent) -> Result<()> {
    let host_path = intent.storage.host_path
        .as_ref()
        .unwrap_or(&format!("/opt/appdata/{}", intent.stack_name));
    let mount_point = intent.storage.mount_point
        .as_ref()
        .unwrap_or(&"/appdata".to_string());
    
    // Create host directory with correct ownership
    std::fs::create_dir_all(host_path)?;
    Command::new("chown")
        .args(["-R", "100000:100000", host_path])
        .output()?;
    
    // Configure bind mount
    Command::new("pct")
        .args(["set", &vmid.to_string(), "-mp0", &format!("{},mp={}", host_path, mount_point)])
        .output()?;
    
    Ok(())
}
```

### TUN Device Passthrough

```rust
fn setup_tun_device(&self, vmid: u32, intent: &StackIntent) -> Result<()> {
    let conf_file = format!("/etc/pve/lxc/{}.conf", vmid);
    let conf_content = std::fs::read_to_string(&conf_file)?;
    
    // Check if already configured
    if conf_content.contains("lxc.cgroup2.devices.allow: c 10:200") {
        info!("TUN device already configured for LXC {}", vmid);
        return Ok(());
    }
    
    // Verify host has TUN
    if !Path::new("/dev/net/tun").exists() {
        return Err(anyhow!("/dev/net/tun not found on host. Run: modprobe tun"));
    }
    
    // Append configuration
    let tun_config = r#"
# --- TUN Device Passthrough (auto-configured) ---
lxc.cgroup2.devices.allow: c 10:200 rwm
lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file
"#;
    
    let mut file = OpenOptions::new()
        .append(true)
        .open(&conf_file)?;
    file.write_all(tun_config.as_bytes())?;
    
    info!("TUN device passthrough configured for LXC {}", vmid);
    Ok(())
}
```

### Secrets Injection

```rust
fn inject_secrets(&self, vmid: u32) -> Result<()> {
    let env_file = std::env::var("HOST_ENV_FILE")
        .unwrap_or_else(|_| "host-daemon/.env".to_string());
    
    if !Path::new(&env_file).exists() {
        warn!("No HOST env file found, skipping secrets injection");
        return Ok(());
    }
    
    // Extract only INFISICAL_ variables
    let content = std::fs::read_to_string(&env_file)?;
    let infisical_vars: Vec<&str> = content
        .lines()
        .filter(|line| line.starts_with("INFISICAL_"))
        .collect();
    
    if infisical_vars.is_empty() {
        warn!("No INFISICAL_ variables found");
        return Ok(());
    }
    
    let secrets_content = infisical_vars.join("\n");
    
    // Write to container
    let temp_file = format!("/tmp/lxc-secrets-{}", vmid);
    std::fs::write(&temp_file, secrets_content)?;
    
    // Push to container
    Command::new("pct")
        .args(["push", &vmid.to_string(), &temp_file, "/root/.env"])
        .output()?;
    
    // Also append to /etc/environment
    self.pct_exec(vmid, "bash -c 'grep ^INFISICAL_ /root/.env >> /etc/environment'")?;
    
    // Cleanup
    std::fs::remove_file(&temp_file)?;
    
    info!("Secrets injected into LXC {}", vmid);
    Ok(())
}
```

### Dependency Installation

```rust
fn install_dependencies(&self, vmid: u32) -> Result<()> {
    info!("Installing dependencies in LXC {}", vmid);
    
    let install_script = r#"
set -euo pipefail

# Update package lists
apt-get update

# Install base packages
apt-get install -y curl git wget openssl jq unattended-upgrades

# Configure unattended upgrades
dpkg-reconfigure -f noninteractive unattended-upgrades

# Install Docker
curl -fsSL https://get.docker.com | sh

# Install Infisical CLI
curl -1sLf 'https://artifacts-cli.infisical.com/setup.deb.sh' | bash
apt-get update && apt-get install -y infisical

echo "Dependencies installed successfully"
"#;
    
    self.pct_exec(vmid, &format!("bash -c '{}'", install_script))?;
    
    Ok(())
}
```

### Git Sparse Checkout

```rust
fn setup_git_sparse_checkout(&self, vmid: u32, stack_name: &str) -> Result<()> {
    let github_pat = std::env::var("GITHUB_PAT")
        .map_err(|_| anyhow!("GITHUB_PAT not set in HOST environment"))?;
    
    let repo_url = std::env::var("GITOPS_REPO_URL")
        .unwrap_or_else(|_| "https://github.com/kennypassenier/homelab.git".to_string());
    
    let setup_script = format!(r#"
set -euo pipefail

GITOPS_DIR="/opt/gitops"
STACK_NAME="{}"
REPO_URL="{}"
GITHUB_PAT="{}"

# Clone with PAT authentication
git clone --filter=blob:none --no-checkout \
    https://${{GITHUB_PAT}}@${{REPO_URL#https://}} $GITOPS_DIR

cd $GITOPS_DIR

# Configure sparse checkout
git sparse-checkout init --cone
git sparse-checkout set stacks/${{STACK_NAME}}

# Checkout main branch
git checkout main

echo "Sparse checkout completed for stack: ${{STACK_NAME}}"
"#, stack_name, repo_url, github_pat);
    
    self.pct_exec(vmid, &format!("bash -c '{}'", setup_script))?;
    
    Ok(())
}
```

### LXC Daemon Installation

```rust
fn install_lxc_daemon(&self, vmid: u32) -> Result<()> {
    // Download latest LXC daemon from GHCR or GitHub Releases
    let daemon_url = std::env::var("LXC_DAEMON_URL")
        .unwrap_or_else(|_| "ghcr.io/kennypassenier/homelab/lxc-daemon:latest".to_string());
    
    if daemon_url.starts_with("ghcr.io") || daemon_url.starts_with("docker.io") {
        // Extract binary from Docker image
        self.pct_exec(vmid, &format!(r#"
            docker pull {} && \
            docker create --name temp-daemon {} && \
            docker cp temp-daemon:/usr/local/bin/lxc-daemon /usr/local/bin/lxc-daemon && \
            docker rm temp-daemon && \
            chmod +x /usr/local/bin/lxc-daemon
        "#, daemon_url, daemon_url))?;
    } else {
        // Download from GitHub Releases
        self.pct_exec(vmid, &format!(r#"
            curl -L -o /usr/local/bin/lxc-daemon {} && \
            chmod +x /usr/local/bin/lxc-daemon
        "#, daemon_url))?;
    }
    
    Ok(())
}

fn create_daemon_config(&self, vmid: u32, stack_name: &str) -> Result<()> {
    let config_content = format!(r#"
[sync]
interval_seconds = 300
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
"#, stack_name);
    
    // Create config directory
    self.pct_exec(vmid, "mkdir -p /etc/homelab")?;
    
    // Write config
    let temp_file = format!("/tmp/lxc-daemon-config-{}.toml", vmid);
    std::fs::write(&temp_file, config_content)?;
    
    Command::new("pct")
        .args(["push", &vmid.to_string(), &temp_file, "/etc/homelab/lxc-daemon.toml"])
        .output()?;
    
    std::fs::remove_file(&temp_file)?;
    
    // Create systemd service
    let service_content = r#"
[Unit]
Description=Homelab LXC GitOps Daemon
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
EnvironmentFile=/root/.env
ExecStart=/usr/local/bin/lxc-daemon --config /etc/homelab/lxc-daemon.toml
Restart=always
RestartSec=10
StandardOutput=append:/var/log/lxc-daemon.log
StandardError=append:/var/log/lxc-daemon.log

[Install]
WantedBy=multi-user.target
"#;
    
    let temp_service = format!("/tmp/lxc-daemon-service-{}", vmid);
    std::fs::write(&temp_service, service_content)?;
    
    Command::new("pct")
        .args(["push", &vmid.to_string(), &temp_service, "/etc/systemd/system/lxc-daemon.service"])
        .output()?;
    
    std::fs::remove_file(&temp_service)?;
    
    // Enable and start service
    self.pct_exec(vmid, "systemctl daemon-reload")?;
    self.pct_exec(vmid, "systemctl enable lxc-daemon")?;
    self.pct_exec(vmid, "systemctl start lxc-daemon")?;
    
    Ok(())
}
```

---

## Integration with Use Case 01

**Modified**: `host-daemon/src/provision.rs`

```rust
pub fn reconcile_provisioning(&self, apply: bool) -> Vec<String> {
    let intents = self.load_stack_intents();
    let mut logs = Vec::new();
    
    for intent in intents {
        if !intent.managed {
            logs.push(format!("PROVISION [{}] SKIP: managed=false", intent.stack_name));
            continue;
        }
        
        match self.validate_lxc(intent.vmid, &intent) {
            ValidationResult::NotExists => {
                if apply {
                    match self.provision_and_bootstrap(&intent) {
                        Ok(_) => logs.push(format!("PROVISION [{}] CREATE+BOOTSTRAP: vmid={} SUCCESS", intent.stack_name, intent.vmid)),
                        Err(e) => logs.push(format!("PROVISION [{}] FAIL: {}", intent.stack_name, e)),
                    }
                } else {
                    logs.push(format!("PROVISION [{}] CREATE+BOOTSTRAP: vmid={} (preview)", intent.stack_name, intent.vmid));
                }
            },
            ValidationResult::NameMismatch { current, expected } => {
                if apply {
                    self.destroy_lxc(intent.vmid)?;
                    self.provision_and_bootstrap(&intent)?;
                    logs.push(format!("PROVISION [{}] RECREATE+BOOTSTRAP: vmid={} old={} new={}", intent.stack_name, intent.vmid, current, expected));
                } else {
                    logs.push(format!("PROVISION [{}] RECREATE+BOOTSTRAP: vmid={} (preview)", intent.stack_name, intent.vmid));
                }
            },
            ValidationResult::ConfigDrift => {
                // Config updates don't require bootstrap
                if apply {
                    self.reconcile_lxc(intent.vmid, &intent)?;
                    logs.push(format!("PROVISION [{}] UPDATE: vmid={}", intent.stack_name, intent.vmid));
                }
            },
            ValidationResult::Ok => {
                logs.push(format!("PROVISION [{}] OK: vmid={}", intent.stack_name, intent.vmid));
            }
        }
    }
    
    logs
}
```

---

## Files to Modify

**New files:**
- `host-daemon/src/bootstrap.rs`

**Modified files:**
- `host-daemon/src/provision.rs` - Call bootstrap after create
- `host-daemon/src/main.rs` - Add bootstrap state tracking

**Deprecated files:**
- `scripts/host/bootstrap-lxc.sh` - Delete after migration

---

## Testing Checklist

- [ ] HOST creates new LXC and bootstraps automatically
- [ ] Storage mounted correctly
- [ ] TUN device configured when needed
- [ ] Secrets injected (INFISICAL_ only)
- [ ] Dependencies installed (Docker, Infisical, git)
- [ ] Git sparse checkout works
- [ ] SSH keys configured
- [ ] LXC daemon installed and running
- [ ] LXC daemon syncs automatically
- [ ] Rollback works on bootstrap failure

---

## Success Criteria

- ✅ HOST fully provisions LXCs without manual bootstrap script
- ✅ All bootstrap logic in Rust with proper error handling
- ✅ Atomic operation: create → bootstrap → start daemon
- ✅ No shell scripts required for provisioning
- ✅ GitOps workflow: commit → push → HOST handles everything
