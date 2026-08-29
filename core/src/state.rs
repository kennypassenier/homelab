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
    /// B4: fingerprint of the intent that was last applied (see
    /// `manifest::intent_hash`); the client compares its local dir to this.
    #[serde(default)]
    pub applied_hash: String,
    /// Full manifest as last applied, so host-side operations (scheduled
    /// backup, fleet update) can run without the client being connected.
    #[serde(default)]
    pub manifest: Option<crate::manifest::StackManifest>,
    /// H8 (light variant): gates the nightly scheduler for this stack and is
    /// flipped off automatically when a nightly run fails (one loud message,
    /// then silence until the operator looks). Never touches the container's
    /// run state — manual `pct stop` and this flag are independent worlds.
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// C7: set for native-service stacks (bare binary under systemd). A
    /// stack has either `manifest` (compose) or `native`, never both.
    #[serde(default)]
    pub native: Option<crate::native::NativeServiceManifest>,
}

fn enabled_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostState {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub stacks: BTreeMap<String, StackState>,
    /// H10: unix time of the last successful host-meta snapshot (vault,
    /// state, TLS, intent repo). 0 = never — the nightly run then takes one
    /// at the first opportunity.
    #[serde(default)]
    pub last_host_meta: u64,
    /// E8: unix time of the last successful ZFS snapshot+replication run.
    #[serde(default)]
    pub last_zfs: u64,
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

    /// Load state. A MISSING file is a fresh install (empty state); an
    /// UNPARSEABLE file is an error — silently continuing with an empty
    /// fleet would stop all scheduled work and the next save would erase
    /// every other stack permanently (hardening H7). The corrupt content is
    /// preserved next to the original before failing.
    pub async fn load(&self) -> Result<HostState, CoreError> {
        let raw = match self.exec.read_file(&self.path).await {
            Ok(raw) => raw,
            Err(_) => return Ok(HostState::default()),
        };
        match serde_json::from_str::<HostState>(&raw) {
            Ok(state) if state.schema_version <= STATE_SCHEMA_VERSION => Ok(state),
            Ok(state) => Err(CoreError::State(format!(
                "state.json schema v{} is newer than this binary understands (v{}) — refusing to touch it; update the host binary. File: {}",
                state.schema_version, STATE_SCHEMA_VERSION, self.path
            ))),
            Err(e) => {
                let quarantine = format!("{}.corrupt", self.path);
                let _ = self.exec.write_file(&quarantine, &raw, 0o600).await;
                Err(CoreError::State(format!(
                    "state.json does not parse ({}) — copy preserved at {}; fix or remove the original before running mutating operations",
                    e, quarantine
                )))
            }
        }
    }

    pub async fn save(&self, mut state: HostState) -> Result<(), CoreError> {
        state.schema_version = STATE_SCHEMA_VERSION;
        let raw =
            serde_json::to_string_pretty(&state).map_err(|e| CoreError::State(e.to_string()))?;
        self.exec.write_file(&self.path, &raw, 0o644).await
    }
}
