// Storage, hardware passthrough, and appdata directory setup for a new LXC.

use std::path::Path;
use std::process::Command;

use crate::provision::StackIntent;
use super::container::pct_exec;

/// Create host-side appdata directory and set unprivileged ownership.
pub fn setup_storage(_vmid: u32, intent: &StackIntent) -> Result<(), String> {
    let host_path = &intent.host_storage_path;
    std::fs::create_dir_all(host_path)
        .map_err(|e| format!("create_dir_all {}: {}", host_path, e))?;
    if intent.unprivileged {
        let out = Command::new("chown")
            .args(["-R", "100000:100000", host_path])
            .output()
            .map_err(|e| format!("chown: {}", e))?;
        if !out.status.success() {
            return Err(format!("chown: {}", String::from_utf8_lossy(&out.stderr)));
        }
    }
    Ok(())
}

/// Wire the TUN device into the LXC config (idempotent).
pub fn setup_tun_device(vmid: u32) -> Result<(), String> {
    let conf = format!("/etc/pve/lxc/{}.conf", vmid);
    let content = std::fs::read_to_string(&conf)
        .map_err(|e| format!("read LXC config: {}", e))?;
    if content.contains("lxc.cgroup2.devices.allow: c 10:200") {
        return Ok(()); // already configured
    }
    if !Path::new("/dev/net/tun").exists() {
        return Err("/dev/net/tun not found on host — run: modprobe tun".to_string());
    }
    let entry = "\n# TUN passthrough (auto)\nlxc.cgroup2.devices.allow: c 10:200 rwm\nlxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file\n";
    std::fs::write(&conf, format!("{}{}", content, entry))
        .map_err(|e| format!("write LXC config: {}", e))
}

/// Wire Intel/AMD GPU DRM nodes into the LXC config (idempotent).
pub fn setup_gpu_passthrough(vmid: u32) -> Result<(), String> {
    let conf = format!("/etc/pve/lxc/{}.conf", vmid);
    let content = std::fs::read_to_string(&conf)
        .map_err(|e| format!("read LXC config: {}", e))?;
    if content.contains("lxc.cgroup2.devices.allow: c 226:") {
        return Ok(()); // already configured
    }
    if !Path::new("/dev/dri/card0").exists() {
        return Err("/dev/dri/card0 not found — no GPU available for passthrough".to_string());
    }
    let entry = "\n# GPU passthrough (auto)\nlxc.cgroup2.devices.allow: c 226:0 rwm\nlxc.cgroup2.devices.allow: c 226:128 rwm\nlxc.mount.entry: /dev/dri/card0 dev/dri/card0 none bind,optional,create=file\nlxc.mount.entry: /dev/dri/renderD128 dev/dri/renderD128 none bind,optional,create=file\n";
    std::fs::write(&conf, format!("{}{}", content, entry))
        .map_err(|e| format!("write LXC config: {}", e))
}

/// Create /appdata inside the running container.
pub fn create_appdata_dir(vmid: u32) -> Result<(), String> {
    pct_exec(vmid, "mkdir -p /appdata && chmod 755 /appdata")
        .map(|_| ())
}
