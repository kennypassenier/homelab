//! Wire protocol for the single CLIENT ↔ HOST line (AR5).
//!
//! Domain types (manifests, deploy specs) live in homelab-core and are
//! re-exported here so both sides compile against the same definitions.
//! Envelope: `{v, topic, id, payload}` — the version field lets a freshly
//! self-updated HOST tell an older CLIENT to upgrade instead of failing
//! cryptically.

use serde::{Deserialize, Serialize};

pub use homelab_core::manifest::{
    BootSpec, DeploySpec, FileBlob, GatewayRoute, LxcSpec, MountSpec, NetworkSpec, ResourceSpec,
    StackManifest,
};

pub const PROTO_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Ping,
    Status,
    DeployStack(Box<DeploySpec>),
    /// F6: self-diagnosis checks.
    Doctor,
    /// AR14: list captured incident bundles.
    Incidents,
    /// Structured fleet snapshot for the TUI dashboard.
    GetState,
    /// C2: gated destroy. `confirm` must equal the stack name.
    DestroyStack {
        manifest: Box<StackManifest>,
        confirm: String,
    },
    /// E1: back up a stack's /appdata.
    BackupStack(Box<StackManifest>),
    /// E2: restore a stack from a snapshot (default "latest").
    RestoreStack {
        manifest: Box<StackManifest>,
        snapshot: String,
    },
    /// D9/B6: managed update with rollback. `app: None` = whole stack.
    UpdateStack {
        manifest: Box<StackManifest>,
        app: Option<String>,
    },
    /// H5: replace the HOST binary. `binary_b64` is the new executable,
    /// base64-encoded; the host stages, selfchecks, installs with an armed
    /// rollback, and restarts itself.
    SelfUpdateHost {
        binary_b64: String,
    },
}

/// A stack as the TUI sees it — structured, not free text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackView {
    pub name: String,
    pub vmid: u16,
    pub hostname: String,
    pub apps: Vec<AppView>,
    /// intent hash differs from applied → true (B4).
    pub drift: bool,
    pub env_sealed: bool,
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppView {
    pub name: String,
    pub running: bool,
    pub restarts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostView {
    pub name: String,
    pub cpu_pct: u64,
    pub ram_pct: u64,
    pub disk_pct: u64,
    pub tls_fingerprint: String,
    /// C6 capacity. For LXC the honest constraint is ACTUAL usage vs physical
    /// total — committed limits routinely exceed 100% (overcommit is normal),
    /// so committed is shown only as context, not as the primary gauge.
    #[serde(default)]
    pub ram_total_mb: u32,
    /// Real RAM in use across the host (sum of actual, not limits).
    #[serde(default)]
    pub ram_used_mb: u32,
    /// Sum of per-stack RAM ceilings (informational; may exceed total).
    #[serde(default)]
    pub ram_committed_mb: u32,
    #[serde(default)]
    pub cores_total: u16,
    /// 1-minute load average ×100 (so 250 = 2.50), avoids f64 on the wire.
    #[serde(default)]
    pub load1_x100: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetState {
    pub host: HostView,
    pub stacks: Vec<StackView>,
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

impl From<homelab_core::sink::Level> for LogLevel {
    fn from(l: homelab_core::sink::Level) -> Self {
        use homelab_core::sink::Level as L;
        match l {
            L::Debug => LogLevel::Debug,
            L::Info => LogLevel::Info,
            L::Warn => LogLevel::Warn,
            L::Error => LogLevel::Error,
        }
    }
}

// ── Envelope (AR5) ──────────────────────────────────────────────────────────
// Every frame on the wire is wrapped: `{v, topic, id, payload}`. The version
// field lets a freshly self-updated HOST tell an older CLIENT to upgrade
// instead of failing on an unknown field.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Topic {
    Rpc,
    Log,
    Telemetry,
    Transfer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub v: u32,
    pub topic: Topic,
    #[serde(default)]
    pub id: u64,
    pub payload: serde_json::Value,
}

impl Envelope {
    pub fn wrap_server(msg: &ServerMsg) -> Self {
        let topic = match msg {
            ServerMsg::Log { .. } => Topic::Log,
            ServerMsg::Transfer { .. } => Topic::Transfer,
            _ => Topic::Rpc,
        };
        Self {
            v: PROTO_VERSION,
            topic,
            id: 0,
            payload: serde_json::to_value(msg).expect("serializable"),
        }
    }

    pub fn wrap_request(req: &RpcRequest) -> Self {
        Self {
            v: PROTO_VERSION,
            topic: Topic::Rpc,
            id: req.id,
            payload: serde_json::to_value(req).expect("serializable"),
        }
    }

    /// Err(version) when the peer speaks a different protocol version.
    pub fn check_version(&self) -> Result<(), u32> {
        if self.v == PROTO_VERSION {
            Ok(())
        } else {
            Err(self.v)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMsg {
    Hello {
        version: String,
        proto: u32,
    },
    Log {
        level: LogLevel,
        source: String,
        msg: String,
    },
    /// Real byte counters for transfer visuals (G6).
    Transfer {
        op: String,
        label: String,
        done: u64,
        total: Option<u64>,
    },
    /// Structured fleet snapshot (reply to GetState).
    State(Box<FleetState>),
    RpcDone(RpcResponse),
}
