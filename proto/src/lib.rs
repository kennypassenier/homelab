//! Shared protocol types for the single CLIENT ↔ HOST line.
//!
//! Transport: WebSocket carrying JSON. The client authenticates with a bearer
//! token on the upgrade request; after that, `RpcRequest` goes up and a stream
//! of `ServerMsg` (interleaved logs + RPC completions) comes down.

use serde::{Deserialize, Serialize};

pub const PROTO_VERSION: u32 = 1;

// ── Stack intent ─────────────────────────────────────────────────────────────

/// Parsed subset of `stacks/<name>/lxc-compose.yml` (schema v2: intent only,
/// no machine-written state).
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
    /// Absolute path on the Proxmox host (created if missing).
    pub host_path: String,
    /// Absolute mount point inside the LXC.
    pub mount_point: String,
    /// Host-side owner uid (already mapped for unprivileged containers).
    #[serde(default)]
    pub host_owner_uid: Option<u32>,
}

// ── Deploy payload ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileBlob {
    /// Path relative to the stack root, e.g. "syncthing/docker-compose.yml".
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub mode: Option<u32>,
}

/// A single file pushed into the gateway LXC's traefik route directory.
/// This is the ONLY way the protocol can touch a container it does not manage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRoute {
    pub gateway_vmid: u16,
    /// Bare filename; HOST constrains the destination directory.
    pub filename: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploySpec {
    pub manifest: StackManifest,
    pub files: Vec<FileBlob>,
    /// Optional per-app .env content, keyed by app name. Stored on HOST
    /// outside git and pushed to /opt/<stack>/<app>/.env (0600).
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub gateway_route: Option<GatewayRoute>,
}

// ── RPC envelope ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Ping,
    Status,
    DeployStack(Box<DeploySpec>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub id: u64,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMsg {
    Hello { version: String, proto: u32 },
    Log { level: LogLevel, source: String, msg: String },
    RpcDone(RpcResponse),
}
