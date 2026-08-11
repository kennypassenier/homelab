//! Safety gates (A1, A2): whitelist-only management, the no-touch list, and
//! the hostname guard. These run BEFORE any mutating command, always.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};
use crate::manifest::StackManifest;

/// Guests that must never be managed regardless of what a manifest claims.
/// VM 100 OPNsense · 101 Home Assistant · CT 102/103 infra · 104-107/111
/// legacy stacks (104-106 migrate later via explicit config change) ·
/// 201-203 k3s.
pub const DEFAULT_NO_TOUCH: &[u16] = &[100, 101, 102, 103, 104, 105, 106, 107, 111, 201, 202, 203];

pub struct SafetyConfig {
    pub no_touch: Vec<u16>,
    pub gateway_vmid: u16,
    pub gateway_routes_dir: String,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            no_touch: DEFAULT_NO_TOUCH.to_vec(),
            gateway_vmid: 104,
            gateway_routes_dir: "/opt/traefik-config/routes".into(),
        }
    }
}

/// A1 + A2 for a deploy target. Read-only probes only; returns whether the
/// container already exists (and is ours).
pub async fn check_deploy_target(
    exec: &dyn Executor,
    cfg: &SafetyConfig,
    manifest: &StackManifest,
) -> Result<bool, CoreError> {
    if cfg.no_touch.contains(&manifest.vmid) {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} is on the no-touch list",
            manifest.vmid
        )));
    }
    let expected = manifest.canonical_hostname();
    if manifest.hostname != expected {
        return Err(CoreError::SafetyAbort(format!(
            "hostname '{}' does not match canonical '{}'",
            manifest.hostname, expected
        )));
    }

    let vm = manifest.vmid.to_string();
    // A QEMU VM on this id is always fatal — protects every VM including ones
    // not on the list (e.g. template 9000).
    let qm = exec.run(&Cmd::new("qm", &["status", &vm], 30)).await?;
    if qm.success() {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} is a QEMU VM",
            manifest.vmid
        )));
    }

    let cfg_out = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
    if !cfg_out.success() {
        return Ok(false); // does not exist yet — free to create
    }
    let live_hostname = cfg_out
        .stdout
        .lines()
        .find(|l| l.starts_with("hostname:"))
        .map(|l| l.trim_start_matches("hostname:").trim().to_string())
        .unwrap_or_default();
    if live_hostname != expected {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} exists with hostname '{}', expected '{}' — refusing",
            manifest.vmid, live_hostname, expected
        )));
    }
    Ok(true)
}

/// Gate for the single allowed cross-stack write (H1).
pub fn check_gateway_route(
    cfg: &SafetyConfig,
    gateway_vmid: u16,
    filename: &str,
) -> Result<String, CoreError> {
    if gateway_vmid != cfg.gateway_vmid {
        return Err(CoreError::SafetyAbort(format!(
            "gateway routes may only target vmid {}",
            cfg.gateway_vmid
        )));
    }
    if filename.contains('/') || filename.contains("..") || !filename.ends_with(".yml") {
        return Err(CoreError::SafetyAbort(format!(
            "bad route filename '{}'",
            filename
        )));
    }
    Ok(format!("{}/{}", cfg.gateway_routes_dir, filename))
}

/// A6: gate for the remote-exec endpoint. Deny-by-default: the config flag
/// must be explicitly on, and no-touch vmids are refused regardless of it.
pub fn exec_guard(
    enabled: bool,
    cfg: &SafetyConfig,
    vmid: u16,
) -> Result<(), crate::error::CoreError> {
    if !enabled {
        return Err(crate::error::CoreError::SafetyAbort(
            "remote exec is disabled (set exec_enabled = true in host.toml to allow it)".into(),
        ));
    }
    if cfg.no_touch.contains(&vmid) {
        return Err(crate::error::CoreError::SafetyAbort(format!(
            "vmid {} is on the no-touch list — exec refused regardless of config",
            vmid
        )));
    }
    Ok(())
}
