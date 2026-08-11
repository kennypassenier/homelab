//! Stack manifest (lxc-compose.yml v2, intent only) + THE validator (D10).
//! This is the single validation implementation — the client imports it for
//! instant wizard feedback, the host imports it as its trust boundary.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackManifest {
    pub stack_name: String,
    pub vmid: u16,
    pub hostname: String,
    pub network: NetworkSpec,
    pub resources: ResourceSpec,
    pub lxc: LxcSpec,
    pub boot: BootSpec,
    #[serde(default)]
    pub storage: Vec<MountSpec>,
    pub apps: Vec<String>,
}

impl StackManifest {
    pub fn canonical_hostname(&self) -> String {
        format!("{}-app-{}", self.vmid, self.stack_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSpec {
    /// e.g. "10.10.10.10/24"
    pub ip: String,
    pub gateway: String,
    #[serde(default = "default_bridge")]
    pub bridge: String,
    #[serde(default)]
    pub vlan: Option<u16>,
}

fn default_bridge() -> String {
    "vmbr0".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSpec {
    pub cores: u16,
    pub memory_mb: u32,
    #[serde(default)]
    pub swap_mb: u32,
    pub disk_gb: u32,
    #[serde(default = "default_storage")]
    pub storage: String,
}

fn default_storage() -> String {
    "local-lvm".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LxcSpec {
    pub template: String,
    #[serde(default = "yes")]
    pub unprivileged: bool,
    #[serde(default = "default_features")]
    pub features: String,
}

fn yes() -> bool {
    true
}
fn default_features() -> String {
    "nesting=1,keyctl=1".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootSpec {
    #[serde(default = "yes")]
    pub onboot: bool,
    #[serde(default)]
    pub order: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSpec {
    pub host_path: String,
    pub mount_point: String,
    #[serde(default)]
    pub host_owner_uid: Option<u32>,
}

// ── Deploy payload ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileBlob {
    /// Relative to the stack root, e.g. "syncthing/docker-compose.yml".
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub mode: Option<u32>,
}

/// The only cross-stack write the system allows: one traefik route fragment
/// into the gateway's watched routes directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRoute {
    pub gateway_vmid: u16,
    pub filename: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploySpec {
    pub manifest: StackManifest,
    pub files: Vec<FileBlob>,
    /// Per-app .env content — the secrets channel; never enters git (A5).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub gateway_route: Option<GatewayRoute>,
}

// ── Validation (D10) ─────────────────────────────────────────────────────────

/// Validate a full deploy spec. Returns every problem found (not just the
/// first) so the wizard can show them all at once.
pub fn validate(spec: &DeploySpec) -> Result<(), CoreError> {
    let mut problems: Vec<String> = Vec::new();
    let m = &spec.manifest;

    if m.stack_name.is_empty()
        || !m
            .stack_name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        problems.push(format!(
            "stack_name '{}' must be non-empty lowercase [a-z0-9-]",
            m.stack_name
        ));
    }
    if !(100..=354).contains(&m.vmid) {
        problems.push(format!("vmid {} outside the allowed range 100-354", m.vmid));
    }
    if m.hostname != m.canonical_hostname() {
        problems.push(format!(
            "hostname '{}' must be canonical '{}'",
            m.hostname,
            m.canonical_hostname()
        ));
    }
    if !m.network.ip.contains('/') {
        problems.push(format!(
            "network.ip '{}' must be CIDR (e.g. 10.10.10.10/24)",
            m.network.ip
        ));
    }
    if m.resources.memory_mb < 128 {
        problems.push(format!(
            "memory_mb {} below the sane floor of 128",
            m.resources.memory_mb
        ));
    }
    if m.resources.cores == 0 {
        problems.push("cores must be at least 1".into());
    }
    if m.resources.disk_gb < 2 {
        problems.push(format!(
            "disk_gb {} below the sane floor of 2",
            m.resources.disk_gb
        ));
    }
    if m.apps.is_empty() {
        problems.push("a stack needs at least one app".into());
    }
    for mount in &m.storage {
        if !mount.host_path.starts_with("/appdata/") {
            problems.push(format!(
                "storage host_path '{}' must live under /appdata/",
                mount.host_path
            ));
        }
        if !mount.mount_point.starts_with('/') {
            problems.push(format!(
                "mount_point '{}' must be absolute",
                mount.mount_point
            ));
        }
    }
    for f in &spec.files {
        if f.path.contains("..") || f.path.starts_with('/') {
            problems.push(format!("file path '{}' escapes the stack root", f.path));
        }
    }
    for app in spec.env.keys() {
        if !m.apps.contains(app) {
            problems.push(format!("env provided for unknown app '{}'", app));
        }
    }
    if let Some(route) = &spec.gateway_route {
        if route.filename.contains('/')
            || route.filename.contains("..")
            || !route.filename.ends_with(".yml")
        {
            problems.push(format!(
                "gateway route filename '{}' invalid",
                route.filename
            ));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(CoreError::Validation(problems.join("; ")))
    }
}
