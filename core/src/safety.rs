//! Safety gates (A1, A2): whitelist-only management, the no-touch list, and
//! the hostname guard. These run BEFORE any mutating command, always.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};
use crate::manifest::StackManifest;

/// Guests that must never be managed regardless of what a manifest claims.
/// VM 100 OPNsense · CT 102 omada · CT 103 fileserver — untouchable under
/// every circumstance, Kenny's words, no exception clause. VM 101 Home
/// Assistant stays here too: its VM lifecycle is out of scope, while changes
/// inside HA travel through its own API under explicit consent, which this
/// list does not govern.
///
/// Narrowed 2026-08-30 (deployment project, Phase 0 gate item C5) from
/// 100-107/111/201-203. The legacy stacks 104-107 and 111 come under
/// management as that project integrates them one at a time; the k3s VMs
/// 201-203 no longer exist. Removing a vmid from this list does NOT make it
/// deployable on its own — A2 still refuses any container whose hostname is
/// not the canonical `<vmid>-app-<stack>`, which every legacy stack fails.
pub const DEFAULT_NO_TOUCH: &[u16] = &[100, 101, 102, 103];

/// fix-120 (expert panel, api-token-is-root, 2026-09-27): the privileged
/// containers the fleet runs today, downloader (105) and media (106), both
/// for their disk and device access. The daemon uses this list when
/// `host.toml` names none, so the policy arrived without changing a deploy.
pub const FLEET_PRIVILEGED_VMIDS: &[u16] = &[105, 106];

/// fix-120: the host directories the fleet's `data_mounts:` borrow today.
/// Same role as [`FLEET_PRIVILEGED_VMIDS`]: the default when `host.toml`
/// names none.
pub const FLEET_DATA_MOUNT_ROOTS: &[&str] = &[
    "/HDD18TB/subvol-103-disk-0",
    "/HDD12TB/subvol-103-disk-0",
    "/HDD4TB/jellyfin-metadata",
    "/HDD2TB/logs/traefik",
];

#[derive(Debug, Clone)]
pub struct SafetyConfig {
    pub no_touch: Vec<u16>,
    pub gateway_vmid: u16,
    pub gateway_routes_dir: String,
    /// fix-120: the vmids a deploy may create as a PRIVILEGED container. A
    /// privileged container is root on the host, and a stack file is
    /// something anyone holding the API token can send, so it may not ask for
    /// one on its own. `None` = no host policy, which is what the library and
    /// its tests assume; the daemon always sets it.
    pub privileged_vmids: Option<Vec<u16>>,
    /// fix-120: host directories a `data_mounts:` entry may borrow, each with
    /// everything under it. A bind mount of `/` or `/etc/pve` is root on the
    /// host just as a privileged container is. `None` = no host policy.
    pub data_mount_roots: Option<Vec<String>>,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            no_touch: DEFAULT_NO_TOUCH.to_vec(),
            gateway_vmid: 104,
            gateway_routes_dir: "/opt/traefik-config/routes".into(),
            privileged_vmids: None,
            data_mount_roots: None,
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
    check_host_policy(cfg, manifest)?;

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

/// fix-120 (expert panel, api-token-is-root, 2026-09-27): what a stack file
/// may ask of the host beyond an ordinary container. The policy lives in
/// `host.toml`, which no RPC can change, so a token holder can run apps but
/// cannot build a container that is root on the host.
pub fn check_host_policy(cfg: &SafetyConfig, manifest: &StackManifest) -> Result<(), CoreError> {
    if let Some(allowed) = &cfg.privileged_vmids
        && !manifest.lxc.unprivileged
        && !allowed.contains(&manifest.vmid)
    {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} asks for a privileged container, and host.toml's privileged_vmids {:?} \
                 does not name it — a privileged container is root on the host, so the host \
                 decides (add the vmid there by ssh if it is meant)",
            manifest.vmid, allowed
        )));
    }
    if let Some(roots) = &cfg.data_mount_roots {
        for dm in &manifest.data_mounts {
            if !under_a_root(&dm.host_path, roots) {
                return Err(CoreError::SafetyAbort(format!(
                    "data mount '{}' is outside host.toml's data_mount_roots {:?} — a bind \
                     mount of a host directory is host access, so the host decides (add the \
                     directory there by ssh if it is meant)",
                    dm.host_path, roots
                )));
            }
        }
    }
    Ok(())
}

/// `path` is one of `roots` or inside one, compared by whole components.
/// Any `.` or `..` component refuses outright: it could step out of a root
/// that the string prefix still matches.
fn under_a_root(path: &str, roots: &[String]) -> bool {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if !path.starts_with('/') || parts.iter().any(|p| *p == "." || *p == "..") {
        return false;
    }
    roots.iter().any(|root| {
        let r: Vec<&str> = root.split('/').filter(|p| !p.is_empty()).collect();
        !r.is_empty() && parts.len() >= r.len() && parts[..r.len()] == r[..]
    })
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
