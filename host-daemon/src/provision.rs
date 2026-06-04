use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

// ============================================================================
// SAFETY GUARANTEE: This module uses a WHITELIST approach
// ============================================================================
// Only LXCs with a corresponding stacks/{stack}/lxc-compose.yml file are
// managed. All other VMs/LXCs on the Proxmox host are completely ignored.
//
// This ensures that non-GitOps containers (e.g., PiHole, legacy VMs, etc.)
// are never touched by the provisioning system.
//
// Additional safety: Before destroying any container, we validate that:
// 1. The VMID exists in our intent list (has lxc-compose.yml)
// 2. The container name matches expected pattern (prevents accidental deletion)
// 3. host_management.managed=true (opt-out available)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackIntent {
    pub stack_name: String,
    pub vmid: u32,
    pub hostname: String,
    pub hwaddr: String,
    pub deploy_enabled: bool,
    pub bridge: String,
    pub ip_mode: String,
    pub reserved_ipv4: Option<String>,
    pub autostart: bool,
    pub startup_order: u32,
    pub cpu_cores: u8,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub host_storage_path: String,
    pub mount_point: String,
    pub lxc_template: String,
    pub unprivileged: bool,
    pub features: Vec<String>,
    pub tun_device: Option<bool>,
    pub managed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProvisionAction {
    Ok {
        stack: String,
        vmid: u32,
        name: String,
    },
    Create {
        stack: String,
        vmid: u32,
        name: String,
    },
    Recreate {
        stack: String,
        vmid: u32,
        current_name: String,
        expected_name: String,
        reason: String,
    },
    Update {
        stack: String,
        vmid: u32,
        name: String,
        drift: Vec<String>,
    },
    Skip {
        stack: String,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub exists: bool,
    pub name_matches: bool,
    pub current_name: Option<String>,
    pub config_drift: Vec<String>,
}

/// Scan all stacks/*/lxc-compose.yml files and parse them into StackIntent structs
pub fn scan_stack_intents(repo_root: &Path) -> Result<Vec<StackIntent>, String> {
    let stacks_dir = repo_root.join("stacks");
    if !stacks_dir.exists() {
        return Ok(Vec::new());
    }

    let mut intents = Vec::new();

    for entry in
        std::fs::read_dir(&stacks_dir).map_err(|e| format!("Failed to read stacks dir: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {}", e))?;
        let stack_path = entry.path();

        if !stack_path.is_dir() {
            continue;
        }

        let lxc_compose_path = stack_path.join("lxc-compose.yml");
        if !lxc_compose_path.exists() {
            continue;
        }

        match parse_lxc_compose(&lxc_compose_path) {
            Ok(intent) => intents.push(intent),
            Err(e) => eprintln!(
                "Warning: Failed to parse {}: {}",
                lxc_compose_path.display(),
                e
            ),
        }
    }

    Ok(intents)
}

/// Parse a single lxc-compose.yml file
fn parse_lxc_compose(path: &Path) -> Result<StackIntent, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse YAML: {}", e))?;

    let stack_name = yaml["stack_name"]
        .as_str()
        .ok_or("Missing stack_name")?
        .to_string();

    let vmid = yaml["vmid"].as_u64().ok_or("Missing vmid")? as u32;

    let hostname = yaml["hostname"]
        .as_str()
        .ok_or("Missing hostname")?
        .to_string();

    let hwaddr = yaml["hwaddr"].as_str().ok_or("Missing hwaddr")?.to_string();

    let deploy_enabled = yaml["deploy"]["enabled"].as_bool().unwrap_or(false);

    let bridge = yaml["network"]["bridge"]
        .as_str()
        .unwrap_or("vmbr0")
        .to_string();

    let ip_mode = yaml["network"]["ip_mode"]
        .as_str()
        .unwrap_or("dhcp-reserved")
        .to_string();

    let reserved_ipv4 = yaml["network"]["reserved_ipv4"]
        .as_str()
        .map(|s| s.to_string());

    let autostart = yaml["boot"]["autostart"].as_bool().unwrap_or(true);

    let startup_order = yaml["boot"]["order"].as_u64().unwrap_or(90) as u32;

    let cpu_cores = yaml["resources"]["cores"].as_u64().unwrap_or(1) as u8;

    let memory_mb = yaml["resources"]["memory_mb"].as_u64().unwrap_or(512) as u32;

    let disk_gb = yaml["resources"]["disk_gb"].as_u64().unwrap_or(8) as u32;

    let host_storage_path = yaml["storage"]["host_path"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("/opt/appdata/{}", stack_name));

    let mount_point = yaml["storage"]["mount_point"]
        .as_str()
        .unwrap_or("/appdata")
        .to_string();

    let lxc_template = yaml["lxc"]["template"]
        .as_str()
        .unwrap_or("debian-12-standard 12.12-1 amd64")
        .to_string();

    let unprivileged = yaml["lxc"]["unprivileged"].as_bool().unwrap_or(true);

    let features = yaml["lxc"]["features"]
        .as_sequence()
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(|| vec!["nesting=1".to_string()]);

    let tun_device = match &yaml["hardware"]["tun_device"] {
        serde_yaml::Value::Bool(b) => Some(*b),
        serde_yaml::Value::Null => None,
        _ => None,
    };

    let managed = yaml["host_management"]["managed"].as_bool().unwrap_or(true);

    Ok(StackIntent {
        stack_name,
        vmid,
        hostname,
        hwaddr,
        deploy_enabled,
        bridge,
        ip_mode,
        reserved_ipv4,
        autostart,
        startup_order,
        cpu_cores,
        memory_mb,
        disk_gb,
        host_storage_path,
        mount_point,
        lxc_template,
        unprivileged,
        features,
        tun_device,
        managed,
    })
}

/// Validate an LXC container against the stack intent
pub fn validate_lxc(vmid: u32, intent: &StackIntent) -> Result<ValidationResult, String> {
    // Check if VMID exists
    let status_output = Command::new("pct")
        .arg("status")
        .arg(vmid.to_string())
        .output()
        .map_err(|e| format!("Failed to run pct status: {}", e))?;

    let exists = status_output.status.success();

    if !exists {
        return Ok(ValidationResult {
            exists: false,
            name_matches: false,
            current_name: None,
            config_drift: Vec::new(),
        });
    }

    // Get current config
    let config_output = Command::new("pct")
        .arg("config")
        .arg(vmid.to_string())
        .output()
        .map_err(|e| format!("Failed to run pct config: {}", e))?;

    let config_str = String::from_utf8_lossy(&config_output.stdout);
    let config = parse_pct_config(&config_str);

    // Check hostname
    let current_name = config.get("hostname").cloned();
    let name_matches = current_name.as_ref().map_or(false, |name| {
        name == &intent.hostname
            || (name.starts_with("lxc-") && name == &format!("lxc-{}", intent.stack_name))
    });

    // Detect config drift
    let mut drift = Vec::new();

    if let Some(cores_str) = config.get("cores") {
        if let Ok(cores) = cores_str.parse::<u8>() {
            if cores != intent.cpu_cores {
                drift.push(format!("cores:{}→{}", cores, intent.cpu_cores));
            }
        }
    }

    if let Some(memory_str) = config.get("memory") {
        if let Ok(memory) = memory_str.parse::<u32>() {
            if memory != intent.memory_mb {
                drift.push(format!("memory:{}→{}", memory, intent.memory_mb));
            }
        }
    }

    if let Some(onboot_str) = config.get("onboot") {
        let onboot = onboot_str == "1";
        if onboot != intent.autostart {
            drift.push(format!("autostart:{}→{}", onboot, intent.autostart));
        }
    }

    Ok(ValidationResult {
        exists: true,
        name_matches,
        current_name,
        config_drift: drift,
    })
}

/// Parse pct config output into a key-value map
fn parse_pct_config(output: &str) -> HashMap<String, String> {
    let mut config = HashMap::new();

    for line in output.lines() {
        if let Some((key, value)) = line.split_once(':') {
            config.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    config
}

/// Create a new LXC container based on stack intent
pub fn create_lxc(intent: &StackIntent, dry_run: bool) -> Result<(), String> {
    if dry_run {
        println!(
            "DRY-RUN: Would create LXC {} with template {}",
            intent.vmid, intent.lxc_template
        );
        return Ok(());
    }

    // Ensure storage path exists
    std::fs::create_dir_all(&intent.host_storage_path)
        .map_err(|e| format!("Failed to create storage path: {}", e))?;

    // Build pct create command
    let mut cmd = Command::new("pct");
    cmd.arg("create")
        .arg(intent.vmid.to_string())
        .arg(&intent.lxc_template)
        .arg("--hostname")
        .arg(&intent.hostname)
        .arg("--cores")
        .arg(intent.cpu_cores.to_string())
        .arg("--memory")
        .arg(intent.memory_mb.to_string())
        .arg("--rootfs")
        .arg(format!("local-lvm:{}", intent.disk_gb))
        .arg("--net0")
        .arg(format!(
            "name=eth0,bridge={},hwaddr={},ip=dhcp",
            intent.bridge, intent.hwaddr
        ))
        .arg("--onboot")
        .arg(if intent.autostart { "1" } else { "0" })
        .arg("--startup")
        .arg(format!("order={}", intent.startup_order))
        .arg("--unprivileged")
        .arg(if intent.unprivileged { "1" } else { "0" });

    // Add features
    if !intent.features.is_empty() {
        cmd.arg("--features").arg(intent.features.join(","));
    }

    // Add storage mount
    cmd.arg("--mp0").arg(format!(
        "{},mp={}",
        intent.host_storage_path, intent.mount_point
    ));

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute pct create: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "pct create failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Bootstrap the newly created container
    println!("LXC {} created, starting bootstrap...", intent.vmid);
    match crate::bootstrap::bootstrap_lxc(intent.vmid, intent) {
        Ok(result) => {
            println!(
                "Bootstrap completed for LXC {} in {:?}",
                intent.vmid, result.duration
            );
            Ok(())
        }
        Err(e) => Err(format!("Bootstrap failed: {}", e)),
    }
}

/// Destroy an LXC container
/// SAFETY: This should only be called for containers managed by GitOps
pub fn destroy_lxc(vmid: u32, expected_name: &str, dry_run: bool) -> Result<(), String> {
    if dry_run {
        println!(
            "DRY-RUN: Would destroy LXC {} (expected name: {})",
            vmid, expected_name
        );
        return Ok(());
    }

    // SAFETY CHECK: Verify the container name matches what we expect
    // This prevents accidental deletion of unrelated containers
    let config_output = Command::new("pct")
        .arg("config")
        .arg(vmid.to_string())
        .output()
        .map_err(|e| format!("Failed to read container config: {}", e))?;

    if config_output.status.success() {
        let config_str = String::from_utf8_lossy(&config_output.stdout);
        let config = parse_pct_config(&config_str);

        if let Some(actual_name) = config.get("hostname") {
            // Validate name matches expected pattern for managed containers
            let is_canonical = actual_name == expected_name;
            let is_legacy = actual_name.starts_with("lxc-");

            if !is_canonical && !is_legacy {
                return Err(format!(
                    "SAFETY ABORT: Container {} has unexpected name '{}' (expected '{}' or 'lxc-*'). \
                     This container may not be managed by GitOps. Refusing to destroy.",
                    vmid, actual_name, expected_name
                ));
            }

            println!(
                "Safety check passed: Container {} name '{}' matches GitOps pattern",
                vmid, actual_name
            );
        } else {
            return Err(format!(
                "SAFETY ABORT: Cannot determine hostname for container {}. Refusing to destroy.",
                vmid
            ));
        }
    }

    // Stop if running
    let _ = Command::new("pct")
        .arg("stop")
        .arg(vmid.to_string())
        .output();

    // Destroy
    let output = Command::new("pct")
        .arg("destroy")
        .arg(vmid.to_string())
        .output()
        .map_err(|e| format!("Failed to execute pct destroy: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "pct destroy failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Update LXC configuration to match intent
pub fn reconcile_lxc(vmid: u32, intent: &StackIntent, dry_run: bool) -> Result<(), String> {
    if dry_run {
        println!("DRY-RUN: Would reconcile LXC {} config", vmid);
        return Ok(());
    }

    // Update cores
    let _ = Command::new("pct")
        .arg("set")
        .arg(vmid.to_string())
        .arg("--cores")
        .arg(intent.cpu_cores.to_string())
        .output();

    // Update memory
    let _ = Command::new("pct")
        .arg("set")
        .arg(vmid.to_string())
        .arg("--memory")
        .arg(intent.memory_mb.to_string())
        .output();

    // Update autostart
    let _ = Command::new("pct")
        .arg("set")
        .arg(vmid.to_string())
        .arg("--onboot")
        .arg(if intent.autostart { "1" } else { "0" })
        .output();

    Ok(())
}

/// Analyze all stacks and determine what provisioning actions are needed
pub fn plan_provisioning_changes(repo_root: &Path) -> Result<Vec<ProvisionAction>, String> {
    let intents = scan_stack_intents(repo_root)?;
    let mut actions = Vec::new();

    for intent in intents {
        if !intent.managed {
            actions.push(ProvisionAction::Skip {
                stack: intent.stack_name.clone(),
                reason: "host_management.managed=false".to_string(),
            });
            continue;
        }

        if intent.vmid == 0 {
            actions.push(ProvisionAction::Skip {
                stack: intent.stack_name.clone(),
                reason: "vmid=0 (not provisioned)".to_string(),
            });
            continue;
        }

        match validate_lxc(intent.vmid, &intent) {
            Ok(validation) => {
                if !validation.exists {
                    actions.push(ProvisionAction::Create {
                        stack: intent.stack_name.clone(),
                        vmid: intent.vmid,
                        name: intent.hostname.clone(),
                    });
                } else if !validation.name_matches {
                    actions.push(ProvisionAction::Recreate {
                        stack: intent.stack_name.clone(),
                        vmid: intent.vmid,
                        current_name: validation
                            .current_name
                            .unwrap_or_else(|| "unknown".to_string()),
                        expected_name: intent.hostname.clone(),
                        reason: "name_mismatch".to_string(),
                    });
                } else if !validation.config_drift.is_empty() {
                    actions.push(ProvisionAction::Update {
                        stack: intent.stack_name.clone(),
                        vmid: intent.vmid,
                        name: intent.hostname.clone(),
                        drift: validation.config_drift,
                    });
                } else {
                    actions.push(ProvisionAction::Ok {
                        stack: intent.stack_name.clone(),
                        vmid: intent.vmid,
                        name: intent.hostname.clone(),
                    });
                }
            }
            Err(e) => {
                actions.push(ProvisionAction::Skip {
                    stack: intent.stack_name.clone(),
                    reason: format!("validation_error: {}", e),
                });
            }
        }
    }

    Ok(actions)
}

/// Apply provisioning changes
pub fn apply_provisioning_changes(
    repo_root: &Path,
    dry_run: bool,
) -> Result<Vec<ProvisionAction>, String> {
    let actions = plan_provisioning_changes(repo_root)?;
    let intents = scan_stack_intents(repo_root)?;
    let intent_map: HashMap<String, StackIntent> = intents
        .into_iter()
        .map(|i| (i.stack_name.clone(), i))
        .collect();

    for action in &actions {
        match action {
            ProvisionAction::Create { stack, vmid: _, .. } => {
                if let Some(intent) = intent_map.get(stack) {
                    create_lxc(intent, dry_run)?;
                }
            }
            ProvisionAction::Recreate {
                stack,
                vmid,
                expected_name,
                ..
            } => {
                if let Some(intent) = intent_map.get(stack) {
                    // Pass expected name for safety validation
                    destroy_lxc(*vmid, expected_name, dry_run)?;
                    create_lxc(intent, dry_run)?;
                }
            }
            ProvisionAction::Update { vmid, stack, .. } => {
                if let Some(intent) = intent_map.get(stack) {
                    reconcile_lxc(*vmid, intent, dry_run)?;
                }
            }
            _ => {}
        }
    }

    Ok(actions)
}

/// Format provisioning actions for display
pub fn format_provision_summary(actions: &[ProvisionAction]) -> Vec<String> {
    let mut lines = Vec::new();

    let ok_count = actions
        .iter()
        .filter(|a| matches!(a, ProvisionAction::Ok { .. }))
        .count();
    let create_count = actions
        .iter()
        .filter(|a| matches!(a, ProvisionAction::Create { .. }))
        .count();
    let recreate_count = actions
        .iter()
        .filter(|a| matches!(a, ProvisionAction::Recreate { .. }))
        .count();
    let update_count = actions
        .iter()
        .filter(|a| matches!(a, ProvisionAction::Update { .. }))
        .count();
    let skip_count = actions
        .iter()
        .filter(|a| matches!(a, ProvisionAction::Skip { .. }))
        .count();

    for action in actions {
        let line = match action {
            ProvisionAction::Ok { stack, vmid, name } => {
                format!("[{}] OK vmid={} name={} config=match", stack, vmid, name)
            }
            ProvisionAction::Create { stack, vmid, name } => {
                format!(
                    "[{}] CREATE vmid={} name={} reason=not_exist",
                    stack, vmid, name
                )
            }
            ProvisionAction::Recreate {
                stack,
                vmid,
                current_name,
                expected_name,
                reason,
            } => {
                format!(
                    "[{}] RECREATE vmid={} current_name={} expected_name={} reason={}",
                    stack, vmid, current_name, expected_name, reason
                )
            }
            ProvisionAction::Update {
                stack,
                vmid,
                name,
                drift,
            } => {
                format!(
                    "[{}] UPDATE vmid={} name={} drift={}",
                    stack,
                    vmid,
                    name,
                    drift.join(",")
                )
            }
            ProvisionAction::Skip { stack, reason } => {
                format!("[{}] SKIP reason={}", stack, reason)
            }
        };
        lines.push(line);
    }

    lines.push(String::new());
    lines.push(format!(
        "Summary: {} OK, {} CREATE, {} RECREATE, {} UPDATE, {} SKIP",
        ok_count, create_count, recreate_count, update_count, skip_count
    ));

    lines
}
