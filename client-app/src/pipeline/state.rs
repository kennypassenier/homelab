// Deploy-state.json persistence.
//
// CLIENT writes one deploy-state.json per stack directly to disk at:
//   stacks/<stack>/deploy-state.json
//
// This file is NOT committed to git — it tracks live deployment progress so
// a partially completed pipeline can be resumed without destroying the container.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::step::StepRecord;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStatus {
    /// Pipeline has not been run yet.
    NotStarted,
    /// At least one step is running.
    Running,
    /// All required steps succeeded.
    Succeeded,
    /// A step failed and the pipeline is halted.
    Failed,
}

impl Default for PipelineStatus {
    fn default() -> Self {
        PipelineStatus::NotStarted
    }
}

/// Per-stack deployment state file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployState {
    pub schema_version: u32,
    pub stack: String,
    /// vmid at the time of last deploy attempt (0 = unprovisioned).
    pub vmid: u32,
    /// Which pipeline variant is being tracked.
    pub pipeline: String,
    #[serde(default)]
    pub status: PipelineStatus,
    /// 1-based index of the highest step that has reached a terminal state.
    pub last_completed_step: u32,
    /// The step index to resume from (1-based; 0 = start fresh).
    pub resume_from_step: u32,
    pub started_at: Option<u64>,
    pub updated_at: Option<u64>,
    #[serde(default)]
    pub steps: Vec<StepRecord>,
}

impl DeployState {
    pub fn new(stack: &str, vmid: u32, pipeline: &str, total_steps: u32) -> Self {
        let now = unix_now();
        Self {
            schema_version: 1,
            stack: stack.to_string(),
            vmid,
            pipeline: pipeline.to_string(),
            status: PipelineStatus::NotStarted,
            last_completed_step: 0,
            resume_from_step: 0,
            started_at: Some(now),
            updated_at: Some(now),
            steps: Vec::with_capacity(total_steps as usize),
        }
    }

    /// Returns the step record for a given 1-based index.
    pub fn step_mut(&mut self, index: u32) -> Option<&mut StepRecord> {
        self.steps.iter_mut().find(|s| s.index == index)
    }

    pub fn touch(&mut self) {
        self.updated_at = Some(unix_now());
    }
}

fn state_path(stack_name: &str) -> String {
    format!("stacks/{}/deploy-state.json", stack_name)
}

/// Load existing deploy-state.json for a stack, or None if it does not exist.
pub fn load_deploy_state(stack_name: &str) -> Option<DeployState> {
    let path = state_path(stack_name);
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Persist the deploy state for a stack to disk.
pub fn save_deploy_state(stack_name: &str, state: &DeployState) -> std::io::Result<()> {
    let path = state_path(stack_name);
    let dir = Path::new(&path).parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, json)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
