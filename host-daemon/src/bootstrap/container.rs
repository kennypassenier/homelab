// Minimal pct wrappers used throughout bootstrap sub-modules.
// Every heavy command goes through these two primitives so callers only
// need to depend on this one file.

use std::process::Command;
use std::time::Duration;

/// Stop an LXC container (idempotent — ignores "not running").
pub fn pct_stop(vmid: u32) -> Result<(), String> {
    let out = Command::new("pct")
        .args(["stop", &vmid.to_string()])
        .output()
        .map_err(|e| format!("pct stop: {}", e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains("not running") {
            return Err(format!("pct stop: {}", stderr));
        }
    }
    Ok(())
}

/// Boot an LXC container.
pub fn pct_start(vmid: u32) -> Result<(), String> {
    let out = Command::new("pct")
        .args(["start", &vmid.to_string()])
        .output()
        .map_err(|e| format!("pct start: {}", e))?;
    if !out.status.success() {
        return Err(format!("pct start: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

/// Execute a bash command inside a running LXC container.
pub fn pct_exec(vmid: u32, command: &str) -> Result<String, String> {
    let out = Command::new("pct")
        .args(["exec", &vmid.to_string(), "--", "bash", "-c", command])
        .output()
        .map_err(|e| format!("pct exec: {}", e))?;
    if !out.status.success() {
        return Err(format!("pct exec: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Block until the container answers a trivial command or the timeout expires.
pub fn wait_for_ready(vmid: u32, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(format!("LXC {} did not become ready within {:?}", vmid, timeout));
        }
        if pct_exec(vmid, "echo ready").is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}
