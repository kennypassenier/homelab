// LXC daemon binary installation and systemd service setup.

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

/// Push the LXC daemon binary into the container and start it as a systemd service.
pub fn install_lxc_daemon(vmid: u32, stack_name: &str) -> Result<(), String> {
    push_daemon_binary(vmid)?;
    push_daemon_config(vmid, stack_name)?;
    push_daemon_service(vmid)?;
    start_daemon_service(vmid)
}

fn push_daemon_binary(vmid: u32) -> Result<(), String> {
    let candidates = [
        format!("{}/apps/LXC", default_gitops_repo()),
        format!("{}/lxc-daemon/target/release/LXC", default_gitops_repo()),
        "apps/LXC".to_string(),
        "lxc-daemon/target/release/LXC".to_string(),
    ];

    for path in &candidates {
        if Path::new(path).exists() {
            let out = Command::new("pct")
                .args(["push", &vmid.to_string(), path, "/usr/local/bin/lxc-daemon"])
                .output()
                .map_err(|e| format!("pct push daemon binary: {}", e))?;
            if !out.status.success() {
                return Err(format!("push binary: {}", String::from_utf8_lossy(&out.stderr)));
            }
            pct_exec(vmid, "chmod +x /usr/local/bin/lxc-daemon")?;
            return Ok(());
        }
    }

    Err("LXC daemon binary not found — run `make release-lxc` first".to_string())
}

fn push_daemon_config(vmid: u32, stack_name: &str) -> Result<(), String> {
    pct_exec(vmid, "mkdir -p /etc/homelab")?;

    let config = format!(
        "[sync]\ninterval_seconds = 1800\ngitops_repo = \"/opt/gitops\"\nstack_name = \"{}\"\n\n[git]\nremote = \"origin\"\nbranch = \"main\"\nsparse_checkout = true\n\n[api]\nlisten = \"0.0.0.0:8080\"\nauth_token_env = \"LXC_API_TOKEN\"\n",
        stack_name
    );
    let tmp = format!("/tmp/lxc-cfg-{}", vmid);
    std::fs::write(&tmp, config).map_err(|e| format!("write config tmp: {}", e))?;
    let out = Command::new("pct")
        .args(["push", &vmid.to_string(), &tmp, "/etc/homelab/lxc-daemon.toml"])
        .output()
        .map_err(|e| { std::fs::remove_file(&tmp).ok(); format!("pct push config: {}", e) })?;
    std::fs::remove_file(&tmp).ok();
    if !out.status.success() {
        return Err(format!("push config: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

fn push_daemon_service(vmid: u32) -> Result<(), String> {
    let service = "[Unit]\nDescription=Homelab LXC GitOps Daemon\nAfter=network-online.target docker.service\nWants=network-online.target docker.service\n\n[Service]\nType=simple\nWorkingDirectory=/opt/gitops\nEnvironmentFile=-/root/.env\nEnvironment=GITOPS_REPO=/opt/gitops\nExecStart=/usr/local/bin/lxc-daemon\nRestart=always\nRestartSec=5\nStartLimitIntervalSec=0\nStandardOutput=append:/var/log/lxc-daemon.log\nStandardError=append:/var/log/lxc-daemon.log\n\n[Install]\nWantedBy=multi-user.target\n";
    let tmp = format!("/tmp/lxc-svc-{}", vmid);
    std::fs::write(&tmp, service).map_err(|e| format!("write service tmp: {}", e))?;
    let out = Command::new("pct")
        .args(["push", &vmid.to_string(), &tmp, "/etc/systemd/system/lxc-daemon.service"])
        .output()
        .map_err(|e| { std::fs::remove_file(&tmp).ok(); format!("pct push service: {}", e) })?;
    std::fs::remove_file(&tmp).ok();
    if !out.status.success() {
        return Err(format!("push service: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

fn start_daemon_service(vmid: u32) -> Result<(), String> {
    pct_exec(vmid, "systemctl daemon-reload && systemctl enable lxc-daemon && systemctl start lxc-daemon")?;

    // Wait up to 20 s for the daemon to become active.
    let health = r#"
for i in $(seq 1 20); do
    if systemctl is-active --quiet lxc-daemon; then
        echo "lxc-daemon active after ${i}s"; exit 0; fi
    sleep 1; done
echo "=== HEALTH FAIL ===" && systemctl status lxc-daemon --no-pager && exit 1
"#;
    let out = Command::new("pct")
        .args(["exec", &vmid.to_string(), "--", "bash", "-c", health])
        .output()
        .map_err(|e| format!("health check: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "lxc-daemon did not start in LXC {}: {}",
            vmid,
            String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or("")
        ));
    }
    Ok(())
}
