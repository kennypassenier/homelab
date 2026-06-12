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
    format!("{}/config/.env", default_host_gitops_repo())
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
pub fn bootstrap_lxc(
    vmid: u32,
    intent: &StackIntent,
    log: &dyn Fn(&str, &str),
) -> Result<BootstrapResult, String> {
    let start_time = std::time::Instant::now();

    log(
        "info",
        &format!(
            "[bootstrap] Starting bootstrap for LXC {} (stack={})",
            vmid, intent.stack_name
        ),
    );

    // Stop container for configuration
    log(
        "info",
        &format!(
            "[bootstrap] Stopping LXC {} for pre-boot configuration",
            vmid
        ),
    );
    pct_stop(vmid)?;

    // Storage
    log(
        "info",
        &format!(
            "[bootstrap] Configuring host storage at {}",
            intent.host_storage_path
        ),
    );
    setup_storage(vmid, intent)?;

    // Hardware (TUN device)
    if intent.tun_device.unwrap_or(false) {
        log(
            "info",
            &format!(
                "[bootstrap] Configuring TUN device passthrough for LXC {}",
                vmid
            ),
        );
        setup_tun_device(vmid)?;
    }

    // Hardware (GPU passthrough)
    if intent.gpu_passthrough.unwrap_or(false) {
        log(
            "info",
            &format!("[bootstrap] Configuring GPU passthrough for LXC {}", vmid),
        );
        setup_gpu_passthrough(vmid)?;
    }

    // Start container
    log("info", &format!("[bootstrap] Starting LXC {}", vmid));
    pct_start(vmid)?;
    log(
        "info",
        &format!("[bootstrap] Waiting for LXC {} to become ready...", vmid),
    );
    wait_for_container_ready(vmid, Duration::from_secs(60))?;
    log("ok", &format!("[bootstrap] LXC {} is ready", vmid));

    // Create directories
    log(
        "info",
        &format!(
            "[bootstrap] Creating /appdata directory inside LXC {}",
            vmid
        ),
    );
    create_appdata_directories(vmid, &intent.stack_name)?;

    // Inject secrets (LATCH_* only) into container /root/.env
    log(
        "info",
        &format!("[bootstrap] Injecting LATCH_* secrets into LXC {}", vmid),
    );
    inject_secrets(vmid)?;

    // Install dependencies
    log(
        "info",
        &format!(
            "[bootstrap] Installing system dependencies in LXC {} (apt, Docker)...",
            vmid
        ),
    );
    install_dependencies(vmid)?;
    log(
        "ok",
        &format!("[bootstrap] System dependencies installed in LXC {}", vmid),
    );

    // Install latch CLI binary
    log(
        "info",
        &format!("[bootstrap] Installing latch CLI in LXC {}", vmid),
    );
    install_latch_cli(vmid)?;
    log(
        "ok",
        &format!("[bootstrap] latch CLI installed in LXC {}", vmid),
    );

    // NOTE: No latch login here. The LXC daemon receives full credentials
    // (--PAT --KEY --REPO --project) from CLIENT on every sync request and
    // calls `latch pull` directly with those flags. A stored ~/.latch/config.toml
    // is not needed and can interfere by supplying wrong defaults.

    // Git setup
    log(
        "info",
        &format!(
            "[bootstrap] Configuring sparse Git checkout for stack '{}' in LXC {}",
            intent.stack_name, vmid
        ),
    );
    setup_git_sparse_checkout(vmid, &intent.stack_name)?;
    log(
        "ok",
        &format!("[bootstrap] Sparse checkout configured for LXC {}", vmid),
    );

    // SSH access
    let github_username =
        std::env::var("GITHUB_USERNAME").unwrap_or_else(|_| "kennypassenier".to_string());
    log(
        "info",
        &format!(
            "[bootstrap] Installing SSH keys from GitHub user '{}'",
            github_username
        ),
    );
    setup_ssh_access(vmid, &github_username)?;
    log(
        "ok",
        &format!("[bootstrap] SSH access configured in LXC {}", vmid),
    );

    // Install LXC daemon
    log(
        "info",
        &format!("[bootstrap] Installing LXC daemon binary in LXC {}", vmid),
    );
    install_lxc_daemon(vmid)?;
    log(
        "info",
        &format!(
            "[bootstrap] Creating LXC daemon service config for stack '{}'",
            intent.stack_name
        ),
    );
    create_daemon_config(vmid, &intent.stack_name)?;
    log(
        "ok",
        &format!(
            "[bootstrap] LXC daemon installed and started in LXC {}",
            vmid
        ),
    );

    let duration = start_time.elapsed();
    log(
        "ok",
        &format!(
            "[bootstrap] Bootstrap complete for LXC {} in {:.1}s",
            vmid,
            duration.as_secs_f64()
        ),
    );

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

    eprintln!("Storage configured: {} -> {}", host_path, mount_point);

    Ok(())
}

/// Enable TUN device passthrough for VPN containers
fn setup_tun_device(vmid: u32) -> Result<(), String> {
    let conf_file = format!("/etc/pve/lxc/{}.conf", vmid);
    let conf_content = std::fs::read_to_string(&conf_file)
        .map_err(|e| format!("Failed to read LXC config: {}", e))?;

    // Check if already configured
    if conf_content.contains("lxc.cgroup2.devices.allow: c 10:200") {
        eprintln!("TUN device already configured for LXC {}", vmid);
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

    eprintln!("TUN device passthrough configured for LXC {}", vmid);
    Ok(())
}

/// Enable Intel/AMD GPU passthrough for media/transcoding containers
fn setup_gpu_passthrough(vmid: u32) -> Result<(), String> {
    let conf_file = format!("/etc/pve/lxc/{}.conf", vmid);
    let conf_content = std::fs::read_to_string(&conf_file)
        .map_err(|e| format!("Failed to read LXC config: {}", e))?;

    // Idempotency: skip if DRM cgroup entry already present
    if conf_content.contains("lxc.cgroup2.devices.allow: c 226:") {
        eprintln!("GPU passthrough already configured for LXC {}", vmid);
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

    eprintln!("GPU passthrough configured for LXC {}", vmid);
    Ok(())
}

/// Create /appdata directory inside container
fn create_appdata_directories(vmid: u32, _stack_name: &str) -> Result<(), String> {
    pct_exec(vmid, "mkdir -p /appdata")?;
    pct_exec(vmid, "chmod 755 /appdata")?;

    eprintln!("Created /appdata directory in LXC {}", vmid);
    Ok(())
}

/// Inject LATCH_* secrets from HOST environment
fn inject_secrets(vmid: u32) -> Result<(), String> {
    // Look for env file in multiple locations
    let host_env_file = std::env::var("HOST_ENV_FILE").ok();
    let default_env_file = default_host_env_file();
    let persisted_env_file = "/var/lib/homelab/host-latch.env";
    let possible_paths = vec![
        host_env_file.as_deref().unwrap_or(""),
        default_env_file.as_str(),
        "/root/.env",
        persisted_env_file,
    ];

    let env_file = possible_paths
        .iter()
        .find(|p| !p.is_empty() && Path::new(p).exists());

    // Extract LATCH_ variables from file and fallback to process env.
    // Also inject GITOPS_REPO_URL and GITOPS_REPO_TOKEN so the in-container
    // git operations can re-authenticate if the credential store is ever cleared.
    let mut vars: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(env_file) = env_file {
        let content = std::fs::read_to_string(env_file)
            .map_err(|e| format!("Failed to read env file '{}': {}", env_file, e))?;

        for raw_line in content.lines() {
            let mut line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("export ") {
                line = rest.trim();
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if key.starts_with("LATCH_")
                    || key == "GITOPS_REPO_URL"
                    || key == "GITOPS_REPO_TOKEN"
                    || key == "GITHUB_PAT"
                {
                    let clean = value
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    vars.insert(key.to_string(), clean);
                }
            }
        }
    } else {
        eprintln!(
            "No HOST env file found; falling back to process environment for secrets injection"
        );
    }

    // Env vars take precedence over the file for these critical keys
    for key in [
        "LATCH_PAT",
        "LATCH_KEY",
        "LATCH_SECRETS_REPO",
        "GITOPS_REPO_URL",
        "GITOPS_REPO_TOKEN",
        "GITHUB_PAT",
    ] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                vars.insert(key.to_string(), value);
            }
        }
    }

    // GITOPS_REPO_URL default if not set anywhere
    if !vars.contains_key("GITOPS_REPO_URL") {
        vars.insert(
            "GITOPS_REPO_URL".to_string(),
            "https://github.com/kennypassenier/homelab.git".to_string(),
        );
    }

    if vars.is_empty() {
        eprintln!("No secret variables found to inject");
        return Ok(());
    }

    // Persist only the core latch credentials so bootstrap can recover if HOST_ENV_FILE
    // is temporarily unavailable after a restart.
    if let (Some(pat), Some(key), Some(repo)) = (
        vars.get("LATCH_PAT"),
        vars.get("LATCH_KEY"),
        vars.get("LATCH_SECRETS_REPO"),
    ) {
        let persisted_content = format!(
            "LATCH_PAT={}\nLATCH_KEY={}\nLATCH_SECRETS_REPO={}\n",
            pat, key, repo
        );
        let persisted_dir = Path::new("/var/lib/homelab");
        let _ = std::fs::create_dir_all(persisted_dir);
        let _ = std::fs::write(persisted_env_file, persisted_content);
        let _ = Command::new("chmod")
            .args(["600", persisted_env_file])
            .output();
    }

    let mut keys: Vec<String> = vars.keys().cloned().collect();
    keys.sort();
    let secrets_content = keys
        .into_iter()
        .filter_map(|key| vars.get(&key).map(|value| format!("{}={}", key, value)))
        .collect::<Vec<String>>()
        .join("\n");

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

    eprintln!("Secrets injected into LXC {}", vmid);
    Ok(())
}

/// Install base dependencies (Docker, Git, unattended-upgrades, etc.)
fn install_dependencies(vmid: u32) -> Result<(), String> {
    eprintln!("Installing dependencies in LXC {}...", vmid);

    let install_script = r#"
set -euo pipefail

# Update package lists
apt-get update -qq

# Install base packages
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    curl git wget openssl jq tar unattended-upgrades ca-certificates

# Configure unattended upgrades
dpkg-reconfigure -f noninteractive unattended-upgrades

# Install Docker
if ! command -v docker &> /dev/null; then
    curl -fsSL https://get.docker.com | sh
    systemctl enable docker
    systemctl start docker
fi

echo "Dependencies installed successfully"
"#;

    pct_exec(vmid, install_script)?;

    Ok(())
}

/// Install the native Latch CLI in the LXC using the shared setup script.
fn install_latch_cli(vmid: u32) -> Result<(), String> {
    let latch_binary_path = acquire_lxc_compatible_latch_binary_on_host()?;

    let script_candidates = [
        format!("{}/scripts/lxc/setup-latch.sh", default_host_gitops_repo()),
        "scripts/lxc/setup-latch.sh".to_string(),
    ];

    let script_path = script_candidates
        .iter()
        .find(|path| Path::new(path.as_str()).exists())
        .ok_or_else(|| "Cannot find scripts/lxc/setup-latch.sh on HOST".to_string())?;

    let push_binary = Command::new("pct")
        .args(["push", &vmid.to_string(), &latch_binary_path, "/root/latch"])
        .output()
        .map_err(|e| format!("Failed to push latch binary to LXC: {}", e))?;

    if !push_binary.status.success() {
        return Err(format!(
            "Failed to push latch binary to LXC: {}",
            String::from_utf8_lossy(&push_binary.stderr)
        ));
    }

    let remote_script = "/root/setup-latch.sh";
    let push_output = Command::new("pct")
        .args(["push", &vmid.to_string(), script_path, remote_script])
        .output()
        .map_err(|e| format!("Failed to push latch setup script: {}", e))?;

    if !push_output.status.success() {
        return Err(format!(
            "Failed to push latch setup script: {}",
            String::from_utf8_lossy(&push_output.stderr)
        ));
    }

    pct_exec(
        vmid,
        &format!(
            "export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH && chmod +x {} && {}",
            remote_script, remote_script
        ),
    )?;

    pct_exec(
        vmid,
        "if [[ -x /usr/local/bin/latch ]]; then /usr/local/bin/latch --version; elif command -v latch >/dev/null 2>&1; then latch --version; else echo 'latch binary missing after setup' >&2; exit 1; fi",
    )?;

    Ok(())
}

fn acquire_lxc_compatible_latch_binary_on_host() -> Result<String, String> {
    if let Ok(path) = std::env::var("LATCH_LXC_BINARY_PATH") {
        if !path.trim().is_empty() && Path::new(&path).exists() {
            return Ok(path);
        }
    }

    let update_repo = std::env::var("LATCH_UPDATE_REPO")
        .unwrap_or_else(|_| "kennypassenier/latch-rs".to_string());
    let update_asset = std::env::var("LATCH_LXC_UPDATE_ASSET")
        .unwrap_or_else(|_| "latch-linux-x86_64-lxc.tar.gz".to_string());
    let api_url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        update_repo
    );

    let client = reqwest::blocking::Client::builder()
        .user_agent("homelab-host-daemon/latch-bootstrap")
        .build()
        .map_err(|e| {
            format!(
                "Failed to build HTTP client for latch release lookup: {}",
                e
            )
        })?;

    let mut req = client.get(api_url);
    if let Ok(token) = std::env::var("HOST_UPDATE_TOKEN") {
        if !token.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
    }

    let release_text = req
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Failed to fetch latch release metadata: {}", e))?
        .text()
        .map_err(|e| format!("Failed to read latch release metadata body: {}", e))?;

    let release_json: serde_json::Value = serde_json::from_str(&release_text)
        .map_err(|e| format!("Failed to parse latch release metadata JSON: {}", e))?;

    let asset_url = release_json
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|asset| {
                let name = asset.get("name")?.as_str()?;
                if name == update_asset {
                    asset
                        .get("browser_download_url")?
                        .as_str()
                        .map(str::to_string)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            format!(
                "Latest latch release does not contain asset '{}'",
                update_asset
            )
        })?;

    let temp_dir = std::env::temp_dir().join(format!("latch-lxc-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp dir {}: {}", temp_dir.display(), e))?;
    let archive_path = temp_dir.join(&update_asset);

    let mut download_req = client.get(asset_url);
    if let Ok(token) = std::env::var("HOST_UPDATE_TOKEN") {
        if !token.trim().is_empty() {
            download_req = download_req.header("Authorization", format!("Bearer {}", token));
        }
    }

    let archive_bytes = download_req
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Failed to download latch LXC asset: {}", e))?
        .bytes()
        .map_err(|e| format!("Failed to read latch LXC asset body: {}", e))?;

    std::fs::write(&archive_path, &archive_bytes).map_err(|e| {
        format!(
            "Failed to write latch LXC archive to {}: {}",
            archive_path.display(),
            e
        )
    })?;

    let output = Command::new("tar")
        .args([
            "-xzf",
            &archive_path.to_string_lossy(),
            "-C",
            &temp_dir.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("Failed to run tar for latch LXC asset: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to extract latch LXC asset: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let binary_path = find_latch_binary_in_dir(&temp_dir)
        .ok_or_else(|| "Extracted latch LXC asset did not contain 'latch' binary".to_string())?;

    Ok(binary_path.to_string_lossy().to_string())
}

fn find_latch_binary_in_dir(dir: &Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_latch_binary_in_dir(&path) {
                return Some(found);
            }
            continue;
        }

        if path.file_name().and_then(|n| n.to_str()) == Some("latch") {
            return Some(path);
        }
    }
    None
}

/// Run `latch login` inside the LXC using LATCH_PAT, LATCH_KEY, and LATCH_SECRETS_REPO
/// from /root/.env (already injected by inject_secrets).
fn run_latch_login(vmid: u32, log: &dyn Fn(&str, &str)) -> Result<(), String> {
    let login_script = r#"
set -euo pipefail

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${PATH:-}"

if [[ -f /root/.env ]]; then
    set -a
    . /root/.env
    set +a
fi

PAT="${LATCH_PAT:-}"
KEY="${LATCH_KEY:-}"
REPO="${LATCH_SECRETS_REPO:-}"

if [[ -z "$PAT" || -z "$KEY" || -z "$REPO" ]]; then
    echo "WARN: LATCH_PAT, LATCH_KEY, or LATCH_SECRETS_REPO not set — skipping latch login"
    exit 0
fi

LATCH_BIN=""
if [[ -x /usr/local/bin/latch ]]; then
    LATCH_BIN="/usr/local/bin/latch"
elif command -v latch >/dev/null 2>&1; then
    LATCH_BIN="$(command -v latch)"
fi

if [[ -z "$LATCH_BIN" ]]; then
    echo "ERROR: latch binary not found — cannot run latch login"
    exit 1
fi

echo "Running: ${LATCH_BIN} login --REPO ${REPO}"
"${LATCH_BIN}" login --PAT "${PAT}" --KEY "${KEY}" --REPO "${REPO}"
echo "latch login succeeded"
"#;

    match pct_exec(vmid, login_script) {
        Ok(output) => {
            for line in output.lines() {
                if line.to_lowercase().contains("warn") || line.to_lowercase().contains("skip") {
                    log("warn", &format!("[latch-login] {}", line));
                } else if line.to_lowercase().contains("error")
                    || line.to_lowercase().contains("fail")
                {
                    log("error", &format!("[latch-login] {}", line));
                } else {
                    log("info", &format!("[latch-login] {}", line));
                }
            }
        }
        Err(e) => {
            // latch login failure is non-fatal — the container will fall back to env-backed mode
            log(
                "warn",
                &format!(
                    "[latch-login] Failed (non-fatal, will use env fallback): {}",
                    e
                ),
            );
        }
    }
    Ok(())
}

/// Setup Git sparse checkout for the stack
fn setup_git_sparse_checkout(vmid: u32, stack_name: &str) -> Result<(), String> {
    let github_pat = std::env::var("GITHUB_PAT")
        .or_else(|_| std::env::var("GITOPS_REPO_TOKEN"))
        .map_err(|_| "GITHUB_PAT or GITOPS_REPO_TOKEN not set in HOST environment".to_string())?;

    let repo_url = std::env::var("GITOPS_REPO_URL")
        .unwrap_or_else(|_| "https://github.com/kennypassenier/homelab.git".to_string());

    // Inject PAT directly into the https:// URL, preserving the full hostname.
    // e.g. https://github.com/owner/repo.git → https://<PAT>@github.com/owner/repo.git
    let auth_url = if repo_url.starts_with("https://") {
        format!("https://{}@{}", github_pat, &repo_url["https://".len()..])
    } else {
        repo_url.clone()
    };

    let setup_script = format!(
        r#"
set -euo pipefail

GITOPS_DIR="/opt/gitops"
STACK_NAME="{stack}"
AUTH_URL="{auth_url}"

# Remove existing gitops dir if present
rm -rf $GITOPS_DIR

# Clone with PAT authentication
git clone --filter=blob:none --no-checkout "$AUTH_URL" $GITOPS_DIR

cd $GITOPS_DIR

# Configure sparse checkout
git sparse-checkout init --cone
git sparse-checkout set stacks/$STACK_NAME

# Checkout main branch
git checkout main

# Store credentials for future pulls (strip PAT from display)
git config credential.helper store
echo "$AUTH_URL" > ~/.git-credentials
chmod 600 ~/.git-credentials

echo "Sparse checkout completed for stack: $STACK_NAME"
"#,
        stack = stack_name,
        auth_url = auth_url,
    );

    pct_exec(vmid, &setup_script)?;

    eprintln!("Git sparse checkout configured for LXC {}", vmid);
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

    eprintln!(
        "SSH access configured for LXC {} (GitHub user: {})",
        vmid, github_username
    );
    Ok(())
}

/// Install LXC daemon binary
fn install_lxc_daemon(vmid: u32) -> Result<(), String> {
    eprintln!("Installing LXC daemon in LXC {}...", vmid);

    // Strategy 1: Copy binary from HOST build artifacts first.
    // This prevents stale remote images from overriding a freshly built compatible binary.
    let binary_paths = vec![
        format!("{}/apps/LXC", default_host_gitops_repo()),
        format!(
            "{}/lxc-daemon/target/release/LXC",
            default_host_gitops_repo()
        ),
        "/opt/homelab/lxc-daemon/target/release/LXC".to_string(),
        "apps/LXC".to_string(),
        "lxc-daemon/target/release/LXC".to_string(),
    ];

    for binary_path in binary_paths {
        if Path::new(&binary_path).exists() {
            eprintln!("Found LXC daemon binary at: {}", binary_path);
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
            eprintln!("LXC daemon installed from binary");
            return Ok(());
        }
    }

    // Strategy 2: Try to pull the LXC daemon Docker image.
    if let Ok(lxc_daemon_image) = std::env::var("LXC_DAEMON_IMAGE") {
        let image = format!("{}:latest", lxc_daemon_image);
        eprintln!("Attempting to pull LXC daemon image: {}", image);

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
            eprintln!("LXC daemon installed from docker image");
            return Ok(());
        }
        eprintln!("Docker image pull failed, falling back to placeholder");
    }

    // Strategy 3: Fallback to placeholder (should not reach in production)
    eprintln!("Warning: LXC daemon binary not found in standard locations");
    eprintln!("Creating placeholder - build and deploy the actual daemon from `make release-lxc`");

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

    eprintln!("LXC daemon installed (placeholder - update required)");
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

    // Create systemd service.
    // Intentionally use Wants= (not Requires=) for docker so the daemon starts
    // even when docker is slow to initialise; the docker poller retries gracefully.
    let service_content = r#"[Unit]
Description=Homelab LXC GitOps Daemon
After=network-online.target docker.service
Wants=network-online.target docker.service

[Service]
Type=simple
WorkingDirectory=/opt/gitops
EnvironmentFile=-/root/.env
Environment=GITOPS_REPO=/opt/gitops
ExecStart=/usr/local/bin/lxc-daemon
Restart=always
RestartSec=5
# Disable burst limit so the service always restarts after updates or crashes.
StartLimitIntervalSec=0
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

    // Health check: wait up to 20 seconds for the daemon to be active and listening.
    let health_script = r#"
for i in $(seq 1 20); do
    if systemctl is-active --quiet lxc-daemon; then
        echo "lxc-daemon active after ${i}s"
        exit 0
    fi
    sleep 1
done
echo "=== HEALTH CHECK FAILED ==="
echo "--- systemctl status ---"
systemctl status lxc-daemon --no-pager 2>&1 | tail -20
echo "--- journal ---"
journalctl -u lxc-daemon -n 40 --no-pager 2>&1
echo "--- log file ---"
cat /var/log/lxc-daemon.log 2>/dev/null | tail -30
echo "--- port check ---"
ss -tlnp | grep 8080 || echo 'port 8080 not listening'
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
echo "--- manual test ---"
/usr/local/bin/lxc-daemon 2>&1 &
DPID=$!
sleep 3
kill $DPID 2>/dev/null
exit 1
"#;
    let output = Command::new("pct")
        .arg("exec")
        .arg(vmid.to_string())
        .arg("--")
        .arg("bash")
        .arg("-c")
        .arg(health_script)
        .output()
        .map_err(|e| format!("Failed to execute lxc-daemon health check: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            eprintln!("[lxc-daemon health] {}", stdout.trim());
        }
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = format!("{}\n{}", stdout.trim(), stderr.trim())
            .trim()
            .to_string();
        let msg = if detail.is_empty() {
            "health script returned a non-zero exit code without diagnostic output".to_string()
        } else {
            detail
        };
        return Err(format!(
            "lxc-daemon did not start cleanly in LXC {}: {}",
            vmid, msg
        ));
    }

    eprintln!("LXC daemon service configured and healthy in LXC {}", vmid);
    Ok(())
}
