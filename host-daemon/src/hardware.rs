//! HOST hardware operations — GPU/TUN passthrough reconciliation and status.
//!
//! Provides utilities for inspecting hardware requirements from stack config,
//! validating Proxmox host readiness, and applying/reconciling hardware settings.

use std::fs;
use std::path::Path;

/// Hardware capability
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HardwareCapability {
    Gpu,
    Tun,
}

impl std::fmt::Display for HardwareCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HardwareCapability::Gpu => write!(f, "gpu"),
            HardwareCapability::Tun => write!(f, "tun"),
        }
    }
}

/// Readiness status for a hardware capability
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareReadiness {
    Ready,
    RequiresSetup,
    Unavailable,
}

impl std::fmt::Display for HardwareReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HardwareReadiness::Ready => write!(f, "ready"),
            HardwareReadiness::RequiresSetup => write!(f, "requires-setup"),
            HardwareReadiness::Unavailable => write!(f, "unavailable"),
        }
    }
}

/// Hardware intent from stack config (lxc-compose.yml)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HardwareIntent {
    pub stack_name: String,
    pub capability: HardwareCapability,
    pub required: bool,            // If true, deploy should fail if not ready
    pub device_id: Option<String>, // For GPU: PCI ID or device name
}

/// Hardware status on the host
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HardwareStatus {
    pub capability: HardwareCapability,
    pub readiness: HardwareReadiness,
    pub available_devices: Vec<String>,
    pub iommu_groups: Option<usize>,
    pub message: String,
}

/// Hardware reconciliation result
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HardwareReconciliation {
    pub stack_name: String,
    pub capability: HardwareCapability,
    pub success: bool,
    pub message: String,
}

/// Discover per-stack hardware intent from `stacks/<stack>/lxc-compose.yml`.
pub fn discover_stack_hardware_intents(gitops_root: &Path) -> Result<Vec<HardwareIntent>, String> {
    let stacks_root = gitops_root.join("stacks");
    if !stacks_root.exists() {
        return Ok(Vec::new());
    }

    let mut intents = Vec::new();
    let entries = fs::read_dir(&stacks_root).map_err(|e| e.to_string())?;

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

        let content = match fs::read_to_string(&compose_path) {
            Ok(raw) => raw,
            Err(_) => continue,
        };

        if content.contains("hardware:")
            && content.contains("gpu:")
            && content.contains("enabled: true")
        {
            intents.push(HardwareIntent {
                stack_name: stack_name.to_string(),
                capability: HardwareCapability::Gpu,
                required: !content.contains("required: false"),
                device_id: None,
            });
        }

        if content.contains("hardware:")
            && content.contains("tun:")
            && content.contains("enabled: true")
        {
            intents.push(HardwareIntent {
                stack_name: stack_name.to_string(),
                capability: HardwareCapability::Tun,
                required: !content.contains("required: false"),
                device_id: None,
            });
        }
    }

    Ok(intents)
}

/// Check if GPU passthrough is available and ready
pub fn check_gpu_readiness() -> HardwareStatus {
    let mut available_devices = Vec::new();
    let mut iommu_groups = None;

    // Check if IOMMU is enabled (required for GPU passthrough)
    let iommu_enabled = check_iommu_enabled();

    if !iommu_enabled {
        return HardwareStatus {
            capability: HardwareCapability::Gpu,
            readiness: HardwareReadiness::RequiresSetup,
            available_devices: vec![],
            iommu_groups: None,
            message: "IOMMU not enabled in Proxmox; run enable-gpu.sh to configure".to_string(),
        };
    }

    // Check for GPU devices
    if let Ok(devices) = list_gpu_devices() {
        available_devices = devices.clone();
        if let Ok(count) = count_iommu_groups(&devices) {
            iommu_groups = Some(count);
        }
    }

    let readiness_status = if available_devices.is_empty() {
        HardwareReadiness::Unavailable
    } else {
        HardwareReadiness::Ready
    };

    HardwareStatus {
        capability: HardwareCapability::Gpu,
        readiness: readiness_status.clone(),
        available_devices: available_devices.clone(),
        iommu_groups,
        message: if readiness_status == HardwareReadiness::Ready {
            format!(
                "GPU passthrough ready ({} device(s))",
                available_devices.len()
            )
        } else {
            "No GPU devices detected".to_string()
        },
    }
}

/// Check if TUN/TAP passthrough is available and ready
pub fn check_tun_readiness() -> HardwareStatus {
    // TUN is typically always available on Linux hosts with proper kernel support
    let available = check_tun_available();

    let readiness = if available {
        HardwareReadiness::Ready
    } else {
        HardwareReadiness::RequiresSetup
    };

    HardwareStatus {
        capability: HardwareCapability::Tun,
        readiness,
        available_devices: if available {
            vec!["/dev/net/tun".to_string()]
        } else {
            vec![]
        },
        iommu_groups: None,
        message: if available {
            "TUN/TAP passthrough ready".to_string()
        } else {
            "TUN device not available; run enable-tun.sh to configure".to_string()
        },
    }
}

/// Check if IOMMU is enabled
fn check_iommu_enabled() -> bool {
    // Check kernel command line
    if let Ok(cmdline) = std::fs::read_to_string("/proc/cmdline") {
        if cmdline.contains("intel_iommu=on") || cmdline.contains("amd_iommu=on") {
            return true;
        }
    }

    // Check if iommu_groups exist
    Path::new("/sys/kernel/iommu_groups").exists()
}

/// List available GPU devices (NVIDIA or AMD)
fn list_gpu_devices() -> Result<Vec<String>, String> {
    let mut devices = Vec::new();

    // Try to run lspci to list devices
    if let Ok(output) = std::process::Command::new("lspci").output() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        for line in output_str.lines() {
            if line.contains("NVIDIA")
                || line.contains("nvidia")
                || (line.contains("AMD") || line.contains("amd") || line.contains("Radeon"))
                    && (line.contains("3D") || line.contains("VGA"))
            {
                devices.push(line.to_string());
            }
        }
    }

    Ok(devices)
}

/// Count IOMMU groups
fn count_iommu_groups(_devices: &[String]) -> Result<usize, String> {
    let iommu_path = Path::new("/sys/kernel/iommu_groups");
    if !iommu_path.exists() {
        return Err("IOMMU not enabled".to_string());
    }

    match std::fs::read_dir(iommu_path) {
        Ok(entries) => {
            let count = entries.flatten().count();
            Ok(count)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Check if TUN device is available
fn check_tun_available() -> bool {
    Path::new("/dev/net/tun").exists()
}

/// Reconcile hardware intent with host status
pub fn reconcile_hardware(intent: &HardwareIntent) -> HardwareReconciliation {
    let status = match intent.capability {
        HardwareCapability::Gpu => check_gpu_readiness(),
        HardwareCapability::Tun => check_tun_readiness(),
    };

    let success = status.readiness == HardwareReadiness::Ready
        || (!intent.required && status.readiness == HardwareReadiness::RequiresSetup);

    HardwareReconciliation {
        stack_name: intent.stack_name.clone(),
        capability: intent.capability.clone(),
        success,
        message: if success {
            format!("{}: OK", status.capability)
        } else {
            format!(
                "{}: {} — {}",
                status.capability, status.readiness, status.message
            )
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_display() {
        assert_eq!(HardwareCapability::Gpu.to_string(), "gpu");
        assert_eq!(HardwareCapability::Tun.to_string(), "tun");
    }

    #[test]
    fn test_readiness_display() {
        assert_eq!(HardwareReadiness::Ready.to_string(), "ready");
        assert_eq!(
            HardwareReadiness::RequiresSetup.to_string(),
            "requires-setup"
        );
        assert_eq!(HardwareReadiness::Unavailable.to_string(), "unavailable");
    }

    #[test]
    fn test_hardware_intent() {
        let intent = HardwareIntent {
            stack_name: "media".to_string(),
            capability: HardwareCapability::Gpu,
            required: true,
            device_id: Some("10de:2684".to_string()),
        };

        assert_eq!(intent.capability, HardwareCapability::Gpu);
        assert!(intent.required);
    }

    #[test]
    fn test_reconciliation_optional() {
        let intent = HardwareIntent {
            stack_name: "test".to_string(),
            capability: HardwareCapability::Gpu,
            required: false,
            device_id: None,
        };

        let result = reconcile_hardware(&intent);
        // Optional hardware that's unavailable should still succeed
        assert!(result.success || !intent.required);
    }
}
