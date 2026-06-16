// Secret injection and software installation inside a freshly created LXC.

use std::path::Path;
use std::process::Command;

use super::container::pct_exec;

fn default_gitops_repo() -> String {
    std::env::var("GITOPS_REPO").unwrap_or_else(|_| {
        std::env::var("HOME")
            .map(|h| format!("{}/homelab", h))
            .unwrap_or_else(|_| "/root/homelab".to_string())
    })
}

/// Push LATCH_* and GITOPS_* env vars into /root/.env inside the container.
pub fn inject_secrets(vmid: u32) -> Result<(), String> {
    let host_env = std::env::var("HOST_ENV_FILE")
        .ok()
        .unwrap_or_else(|| format!("{}/config/.env", default_gitops_repo()));

    let mut vars: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Read from file first, then override with process env.
    if Path::new(&host_env).exists() {
        let raw = std::fs::read_to_string(&host_env)
            .map_err(|e| format!("read env file: {}", e))?;
        for line in raw.lines() {
            let line = line.trim().trim_start_matches("export ").trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                if key.starts_with("LATCH_") || matches!(key, "GITOPS_REPO_URL" | "GITOPS_REPO_TOKEN" | "GITHUB_PAT") {
                    vars.insert(key.to_string(), v.trim().trim_matches('"').trim_matches('\'').to_string());
                }
            }
        }
    }

    // Process env overrides.
    for key in ["LATCH_PAT", "LATCH_KEY", "LATCH_SECRETS_REPO", "GITOPS_REPO_URL", "GITOPS_REPO_TOKEN", "GITHUB_PAT"] {
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                vars.insert(key.to_string(), v);
            }
        }
    }

    if !vars.contains_key("GITOPS_REPO_URL") {
        vars.insert("GITOPS_REPO_URL".to_string(), "https://github.com/kennypassenier/homelab.git".to_string());
    }

    if vars.is_empty() {
        return Ok(()); // nothing to inject
    }

    let mut keys: Vec<_> = vars.keys().cloned().collect();
    keys.sort();
    let content = keys.iter()
        .filter_map(|k| vars.get(k).map(|v| format!("{}={}", k, v)))
        .collect::<Vec<_>>()
        .join("\n");

    let tmp = format!("/tmp/lxc-secrets-{}", vmid);
    std::fs::write(&tmp, &content)
        .map_err(|e| format!("write secrets tmp: {}", e))?;

    let out = Command::new("pct")
        .args(["push", &vmid.to_string(), &tmp, "/root/.env"])
        .output()
        .map_err(|e| format!("pct push secrets: {}", e))?;
    std::fs::remove_file(&tmp).ok();
    if !out.status.success() {
        return Err(format!("pct push secrets: {}", String::from_utf8_lossy(&out.stderr)));
    }

    // Also add LATCH_ keys to /etc/environment for non-interactive processes.
    pct_exec(vmid, "bash -c 'grep ^LATCH_ /root/.env >> /etc/environment 2>/dev/null || true'")?;
    Ok(())
}

/// Install apt packages and Docker inside the container.
pub fn install_system_deps(vmid: u32) -> Result<(), String> {
    pct_exec(vmid, r#"
set -euo pipefail
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    curl git wget openssl jq tar unattended-upgrades ca-certificates
dpkg-reconfigure -f noninteractive unattended-upgrades
if ! command -v docker &>/dev/null; then
    curl -fsSL https://get.docker.com | sh
    systemctl enable docker
    systemctl start docker
fi
echo "System dependencies OK"
"#)?;
    Ok(())
}

/// Download the latch CLI binary from GitHub Releases and install it.
pub fn install_latch(vmid: u32) -> Result<(), String> {
    let binary_path = acquire_latch_binary()?;
    let script_path = find_setup_script()?;

    // Push binary then setup script, then run setup script.
    for (src, dst) in [(&binary_path, "/root/latch"), (&script_path, "/root/setup-latch.sh")] {
        let out = Command::new("pct")
            .args(["push", &vmid.to_string(), src, dst])
            .output()
            .map_err(|e| format!("pct push {}: {}", src, e))?;
        if !out.status.success() {
            return Err(format!("pct push {} failed: {}", src, String::from_utf8_lossy(&out.stderr)));
        }
    }

    pct_exec(vmid, "export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH && chmod +x /root/setup-latch.sh && /root/setup-latch.sh")?;
    Ok(())
}

fn find_setup_script() -> Result<String, String> {
    let candidates = [
        format!("{}/scripts/lxc/setup-latch.sh", default_gitops_repo()),
        "scripts/lxc/setup-latch.sh".to_string(),
    ];
    candidates.into_iter()
        .find(|p| Path::new(p).exists())
        .ok_or_else(|| "scripts/lxc/setup-latch.sh not found on HOST".to_string())
}

fn acquire_latch_binary() -> Result<String, String> {
    if let Ok(path) = std::env::var("LATCH_LXC_BINARY_PATH") {
        if !path.trim().is_empty() && Path::new(&path).exists() {
            return Ok(path);
        }
    }
    let repo = std::env::var("LATCH_UPDATE_REPO").unwrap_or_else(|_| "kennypassenier/latch-rs".to_string());
    let asset = std::env::var("LATCH_LXC_UPDATE_ASSET").unwrap_or_else(|_| "latch-linux-x86_64-lxc.tar.gz".to_string());
    let api = format!("https://api.github.com/repos/{}/releases/latest", repo);

    let client = reqwest::blocking::Client::builder()
        .user_agent("homelab-host-daemon/latch-bootstrap")
        .build()
        .map_err(|e| format!("http client: {}", e))?;

    let mut req = client.get(&api);
    if let Ok(tok) = std::env::var("HOST_UPDATE_TOKEN") {
        if !tok.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", tok));
        }
    }

    let json: serde_json::Value = req.send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("fetch release: {}", e))?
        .json()
        .map_err(|e| format!("parse release: {}", e))?;

    let url = json["assets"].as_array()
        .and_then(|a| a.iter().find_map(|x| {
            let name = x["name"].as_str()?;
            if name == asset { x["browser_download_url"].as_str().map(str::to_string) } else { None }
        }))
        .ok_or_else(|| format!("asset '{}' not found in release", asset))?;

    let tmp = std::env::temp_dir().join(format!("latch-lxc-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("create tmp: {}", e))?;
    let archive = tmp.join(&asset);

    let bytes = client.get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("download: {}", e))?
        .bytes()
        .map_err(|e| format!("read bytes: {}", e))?;
    std::fs::write(&archive, &bytes).map_err(|e| format!("write archive: {}", e))?;

    Command::new("tar").args(["-xzf", &archive.to_string_lossy(), "-C", &tmp.to_string_lossy()])
        .output().map_err(|e| format!("tar: {}", e))?;

    find_binary_in_dir(&tmp)
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "latch binary not found in extracted archive".to_string())
}

fn find_binary_in_dir(dir: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let p = e.path();
        if p.is_dir() { return find_binary_in_dir(&p); }
        if p.file_name().and_then(|n| n.to_str()) == Some("latch") { Some(p) } else { None }
    })
}
