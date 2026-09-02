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
    /// C7: legacy single-service field. Kept only so state written before
    /// T5 still loads; `HostState::load` moves it into `natives` and clears
    /// it. Nothing should read this — read `natives`.
    #[serde(default)]
    pub native: Option<crate::native::NativeServiceManifest>,
    /// T5: native services on this stack (bare binaries under systemd). A
    /// stack has either `manifest` (compose) or `natives`, never both. A list
    /// because the layout puts kyu, kyu-runner and http-switchboard on one
    /// container, and one hostname per container means they cannot be three
    /// separate stacks: `native.rs` forces `<vmid>-app-<stack>` and
    /// `guard_target` re-checks it against the live container.
    #[serde(default)]
    pub natives: Vec<crate::native::NativeServiceManifest>,
    /// S2: the step a deploy stopped at, or None when it ran to the end.
    ///
    /// State used to be written only by the last step, so a deploy that
    /// stopped earlier left no record at all. On 2026-09-01 the media stack
    /// failed at "start apps" and therefore did not exist as far as the
    /// orchestrator was concerned: no drift detection, no retention, and —
    /// the part that mattered — no nightly backup of 12 GB of application
    /// configuration, with nothing anywhere saying so. A container that is
    /// running and unknown is worse than one that is plainly broken.
    #[serde(default)]
    pub incomplete_step: Option<String>,
}

impl StackState {
    /// Fold the pre-T5 single-service field into the list. Idempotent.
    fn migrate_natives(&mut self) {
        if let Some(one) = self.native.take() {
            if !self.natives.iter().any(|n| n.unit == one.unit) {
                self.natives.push(one);
            }
        }
    }

    /// True for a stack the orchestrator supervises as systemd units.
    pub fn is_native(&self) -> bool {
        !self.natives.is_empty()
    }
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
    /// G17: the checks only a person can answer, and whether anyone did.
    ///
    /// They were printed at the end of every deploy and stored nowhere, so
    /// "did anybody ever look at the front page after a deploy" had no answer
    /// — 94 of them across 28 files, measured 2026-09-02, and one of them is
    /// exactly the check that would have caught the empty homepage months
    /// earlier. The deploy that prints them now also registers them here, so
    /// the record is written by the thing that already knows.
    ///
    /// Keyed by `manualchecks::id_for`, which is stable across deploys as
    /// long as the wording does not change. If it does, the check is a
    /// different question and the old answer no longer applies.
    #[serde(default)]
    pub manual_checks: BTreeMap<String, ManualCheckRecord>,
    /// G16: unix time of the last notification that actually arrived.
    #[serde(default)]
    pub last_notify_ok: u64,
    /// G16: unix time of the last one that reached no route at all.
    #[serde(default)]
    pub last_notify_failed: u64,
    /// G16: what the last failure said. Kept so the finding can quote it
    /// rather than say "something went wrong".
    #[serde(default)]
    pub last_notify_error: Option<String>,
    /// G14: unix time of the last restore drill that PROVED something.
    #[serde(default)]
    pub last_restore_drill: u64,
    /// Which repository that was, so the finding can name it.
    #[serde(default)]
    pub last_restore_drill_repo: String,
    /// Why the last drill proved nothing. None = it did.
    #[serde(default)]
    pub last_restore_drill_error: Option<String>,
    /// Round-robin cursor, so a year of drills covers every repository
    /// instead of proving the same one twelve times.
    #[serde(default)]
    pub restore_drill_index: usize,
}

/// One thing only a person can confirm, and the last word on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualCheckRecord {
    pub stack: String,
    pub app: String,
    /// The question, verbatim from `checks.yml`.
    pub text: String,
    /// First time a deploy printed it.
    pub registered_at: u64,
    /// Unix time of the last answer. None = nobody has ever answered.
    #[serde(default)]
    pub answered_at: Option<u64>,
    /// What the answer was. None while unanswered.
    #[serde(default)]
    pub ok: Option<bool>,
    /// Whatever the person wanted to add. Empty is normal.
    #[serde(default)]
    pub note: String,
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
            Ok(mut state) if state.schema_version <= STATE_SCHEMA_VERSION => {
                // T5: state written before native services became a list.
                for st in state.stacks.values_mut() {
                    st.migrate_natives();
                }
                Ok(state)
            }
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
