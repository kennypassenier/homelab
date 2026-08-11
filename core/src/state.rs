//! State handling (AR4): typed JSON documents with a schema version, written
//! atomically through the Executor so tests capture them and power loss can
//! never leave a half-written file.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::executor::Executor;

pub const STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackState {
    pub vmid: u16,
    pub hostname: String,
    pub apps: Vec<String>,
    /// Set by the caller (host) — core never reads clocks.
    pub applied_at: u64,
    /// Unix time of the last successful backup (E4 scheduler input).
    #[serde(default)]
    pub last_backup: u64,
    /// Full manifest as last applied, so host-side operations (scheduled
    /// backup, fleet update) can run without the client being connected.
    #[serde(default)]
    pub manifest: Option<crate::manifest::StackManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostState {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub stacks: BTreeMap<String, StackState>,
}

pub struct StateStore<'a> {
    exec: &'a dyn Executor,
    path: String,
}

impl<'a> StateStore<'a> {
    pub fn new(exec: &'a dyn Executor, state_dir: &str) -> Self {
        Self {
            exec,
            path: format!("{}/state.json", state_dir),
        }
    }

    pub async fn load(&self) -> HostState {
        match self.exec.read_file(&self.path).await {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => HostState::default(),
        }
    }

    pub async fn save(&self, mut state: HostState) -> Result<(), CoreError> {
        state.schema_version = STATE_SCHEMA_VERSION;
        let raw =
            serde_json::to_string_pretty(&state).map_err(|e| CoreError::State(e.to_string()))?;
        self.exec.write_file(&self.path, &raw, 0o644).await
    }
}
