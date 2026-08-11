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
    RpcDone(RpcResponse),
}
