//! Proxmox reconciliation for boot policy and hot-applicable resource updates.

use serde_yaml::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
struct StackIntent {
    stack_name: String,
    vmid: u32,
    hostname: String,
    boot_autostart: bool,
    boot_order: u32,
    cpu_cores: u32,
    memory_mb: u32,
    managed: bool,
}

#[derive(Debug, Clone, Default)]
struct PctConfig {
    hostname: String,
    onboot: Option<bool>,
    startup_order: Option<u32>,
    cores: Option<u32>,
    memory_mb: Option<u32>,
    running: Option<bool>,
}

pub fn reconcile_boot_policies(gitops_root: &Path, apply: bool) -> Vec<String> {
    let intents = match load_stack_intents(gitops_root) {
        Ok(intents) => intents,
        Err(e) => return vec![format!("boot-policy scan failed: {}", e)],
    };

    let mut lines = Vec::new();
    lines.push(format!(
        "BOOT reconcile mode={} stack_count={}",
        if apply { "apply" } else { "preview" },
        intents.len()
    ));

    for intent in intents {
        if !intent.managed {
            lines.push(format!(
                "BOOT [{}] skip: host_management.managed=false",
                intent.stack_name
            ));
            continue;
        }

        if intent.vmid == 0 {
            lines.push(format!("BOOT [{}] skip: vmid=0", intent.stack_name));
            continue;
        }

        let pct = match read_pct_config(intent.vmid) {
            Ok(cfg) => cfg,
            Err(e) => {
                lines.push(format!(
                    "BOOT [{}] fail: could not read pct config for vmid {} ({})",
                    intent.stack_name, intent.vmid, e
                ));
                continue;
            }
        };

        if !pct.hostname.is_empty() && pct.hostname != intent.hostname {
            lines.push(format!(
                "BOOT [{}] skip: hostname mismatch intent={} runtime={}",
                intent.stack_name, intent.hostname, pct.hostname
            ));
            continue;
        }

        let drift_onboot = pct.onboot != Some(intent.boot_autostart);
        let drift_order = pct.startup_order != Some(intent.boot_order);

        if !drift_onboot && !drift_order {
            lines.push(format!("BOOT [{}] OK no drift", intent.stack_name));
            continue;
        }

        if !apply {
            lines.push(format!(
                "BOOT [{}] drift onboot={:?}->{}, order={:?}->{}, action=preview",
                intent.stack_name,
                pct.onboot,
                intent.boot_autostart,
                pct.startup_order,
                intent.boot_order
            ));
            continue;
        }

        match apply_boot_policy(&intent) {
            Ok(_) => lines.push(format!(
                "BOOT [{}] applied vmid={} onboot={} order={}",
                intent.stack_name, intent.vmid, intent.boot_autostart, intent.boot_order
            )),
            Err(e) => lines.push(format!(
                "BOOT [{}] fail apply vmid={} ({})",
                intent.stack_name, intent.vmid, e
            )),
        }
    }

    lines
}

pub fn reconcile_hot_resources(gitops_root: &Path, apply: bool) -> Vec<String> {
    let intents = match load_stack_intents(gitops_root) {
        Ok(intents) => intents,
        Err(e) => return vec![format!("resource scan failed: {}", e)],
    };

    let mut lines = Vec::new();
    lines.push(format!(
        "RESOURCE reconcile mode={} stack_count={}",
        if apply { "apply" } else { "preview" },
        intents.len()
    ));

    for intent in intents {
        if !intent.managed {
            lines.push(format!(
                "RESOURCE [{}] skip: host_management.managed=false",
                intent.stack_name
            ));
            continue;
        }

        if intent.vmid == 0 {
            lines.push(format!("RESOURCE [{}] skip: vmid=0", intent.stack_name));
            continue;
        }

        let pct = match read_pct_config(intent.vmid) {
            Ok(cfg) => cfg,
            Err(e) => {
                lines.push(format!(
                    "RESOURCE [{}] fail: could not read pct config for vmid {} ({})",
                    intent.stack_name, intent.vmid, e
                ));
                continue;
            }
        };

        if !pct.hostname.is_empty() && pct.hostname != intent.hostname {
            lines.push(format!(
                "RESOURCE [{}] skip: hostname mismatch intent={} runtime={}",
                intent.stack_name, intent.hostname, pct.hostname
            ));
            continue;
        }

        let cur_cores = pct.cores.unwrap_or(intent.cpu_cores);
        let cur_memory = pct.memory_mb.unwrap_or(intent.memory_mb);
        let running = pct.running.unwrap_or(false);

        let cpu_change = intent.cpu_cores as i64 - cur_cores as i64;
        let mem_change = intent.memory_mb as i64 - cur_memory as i64;

        if cpu_change == 0 && mem_change == 0 {
            lines.push(format!("RESOURCE [{}] OK no drift", intent.stack_name));
            continue;
        }

        let restart_required = running && (cpu_change < 0 || mem_change < 0);

        if restart_required {
            lines.push(format!(
                "RESOURCE [{}] restart-required current={}c/{}MB target={}c/{}MB",
                intent.stack_name, cur_cores, cur_memory, intent.cpu_cores, intent.memory_mb
            ));
            continue;
        }

        if !apply {
            lines.push(format!(
                "RESOURCE [{}] hot-applicable current={}c/{}MB target={}c/{}MB action=preview",
                intent.stack_name, cur_cores, cur_memory, intent.cpu_cores, intent.memory_mb
            ));
            continue;
        }

        match apply_resources(&intent) {
            Ok(_) => lines.push(format!(
                "RESOURCE [{}] applied vmid={} cores={} memory_mb={}",
                intent.stack_name, intent.vmid, intent.cpu_cores, intent.memory_mb
            )),
            Err(e) => lines.push(format!(
                "RESOURCE [{}] fail apply vmid={} ({})",
                intent.stack_name, intent.vmid, e
            )),
        }
    }

    lines
}

fn load_stack_intents(gitops_root: &Path) -> Result<Vec<StackIntent>, String> {
    let stacks_root = gitops_root.join("stacks");
    if !stacks_root.exists() {
        return Ok(Vec::new());
    }

    let mut intents = Vec::new();
    let entries = fs::read_dir(stacks_root).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let stack_dir = entry.path();
        if !stack_dir.is_dir() {
            continue;
        }

        let Some(stack_name) = stack_dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        let compose_path = stack_dir.join("lxc-compose.yml");
        if !compose_path.exists() {
            continue;
        }

        let raw = match fs::read_to_string(compose_path) {
            Ok(raw) => raw,
            Err(_) => continue,
        };

        let doc: Value = match serde_yaml::from_str(&raw) {
            Ok(doc) => doc,
            Err(_) => continue,
        };

        let vmid = value_u64(&doc, &["vmid"]).unwrap_or(0) as u32;
        let hostname = value_str(&doc, &["hostname"])
            .unwrap_or_else(|| format!("lxc-{}", stack_name));
        let boot_autostart = value_bool(&doc, &["boot", "autostart"]).unwrap_or(true);
        let boot_order = value_u64(&doc, &["boot", "order"]).unwrap_or(90) as u32;
        let cpu_cores = value_u64(&doc, &["resources", "cores"]).unwrap_or(2) as u32;
        let memory_mb = value_u64(&doc, &["resources", "memory_mb"]).unwrap_or(2048) as u32;
        let managed = value_bool(&doc, &["host_management", "managed"]).unwrap_or(true);

        intents.push(StackIntent {
            stack_name: stack_name.to_string(),
            vmid,
            hostname,
            boot_autostart,
            boot_order,
            cpu_cores,
            memory_mb,
            managed,
        });
    }

    Ok(intents)
}

fn read_pct_config(vmid: u32) -> Result<PctConfig, String> {
    let output = Command::new("pct")
        .args(["config", &vmid.to_string()])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let mut cfg = PctConfig::default();
    let raw = String::from_utf8_lossy(&output.stdout);

    for line in raw.lines() {
        if let Some(v) = line.strip_prefix("hostname: ") {
            cfg.hostname = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("onboot: ") {
            cfg.onboot = Some(v.trim() == "1");
        } else if let Some(v) = line.strip_prefix("startup: ") {
            for part in v.split(',') {
                if let Some(order) = part.trim().strip_prefix("order=") {
                    if let Ok(parsed) = order.parse::<u32>() {
                        cfg.startup_order = Some(parsed);
                    }
                }
            }
        } else if let Some(v) = line.strip_prefix("cores: ") {
            cfg.cores = v.trim().parse::<u32>().ok();
        } else if let Some(v) = line.strip_prefix("memory: ") {
            cfg.memory_mb = v.trim().parse::<u32>().ok();
        }
    }

    cfg.running = Some(is_running(vmid)?);
    Ok(cfg)
}

fn is_running(vmid: u32) -> Result<bool, String> {
    let out = Command::new("pct")
        .args(["status", &vmid.to_string()])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Ok(false);
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout.contains("running"))
}

fn apply_boot_policy(intent: &StackIntent) -> Result<(), String> {
    let onboot = if intent.boot_autostart { "1" } else { "0" };
    let startup = format!("order={}", intent.boot_order);

    let out = Command::new("pct")
        .args([
            "set",
            &intent.vmid.to_string(),
            "--onboot",
            onboot,
            "--startup",
            &startup,
        ])
        .output()
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn apply_resources(intent: &StackIntent) -> Result<(), String> {
    let out = Command::new("pct")
        .args([
            "set",
            &intent.vmid.to_string(),
            "--cores",
            &intent.cpu_cores.to_string(),
            "--memory",
            &intent.memory_mb.to_string(),
        ])
        .output()
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn value_str(root: &Value, path: &[&str]) -> Option<String> {
    value_at(root, path).and_then(Value::as_str).map(ToOwned::to_owned)
}

fn value_u64(root: &Value, path: &[&str]) -> Option<u64> {
    value_at(root, path).and_then(Value::as_u64)
}

fn value_bool(root: &Value, path: &[&str]) -> Option<bool> {
    value_at(root, path).and_then(Value::as_bool)
}

fn value_at<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut node = root;
    for key in path {
        node = node.get(*key)?;
    }
    Some(node)
}