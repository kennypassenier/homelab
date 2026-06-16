// Step types for the deployment pipeline.
//
// A PipelineStep is the *definition* of a step (what it does, where it runs).
// A StepRecord is the *result* after the step has been executed (status, output).

use serde::{Deserialize, Serialize};

/// Which tier executes a step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepExecutor {
    /// CLIENT performs this step locally (e.g. write config, validate files).
    Client,
    /// HOST daemon (Proxmox node) executes this step via WebSocket RPC.
    Host,
    /// LXC daemon inside the container executes this step via WebSocket RPC.
    Lxc,
}

/// Lifecycle state of a single step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Succeeded,
    Skipped,
    Failed,
}

impl StepStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            StepStatus::Succeeded | StepStatus::Skipped | StepStatus::Failed
        )
    }
}

/// Immutable definition of one pipeline step.
#[derive(Debug, Clone)]
pub struct PipelineStep {
    /// 1-based index within the pipeline.
    pub index: u32,
    /// Total number of steps in the pipeline (for display like "3/20").
    pub total: u32,
    /// Stable snake_case identifier for serialisation.
    pub id: &'static str,
    /// Human-readable label shown in the log step header.
    pub label: &'static str,
    /// Which runtime executes this step.
    pub executor: StepExecutor,
}

impl PipelineStep {
    /// Returns the formatted header string used in CLIENT log output.
    pub fn header(&self, stack: &str) -> String {
        format!(
            "[STEP {:>2}/{}] {} — {} ({})",
            self.index,
            self.total,
            self.label,
            stack,
            match self.executor {
                StepExecutor::Client => "CLIENT",
                StepExecutor::Host => "HOST",
                StepExecutor::Lxc => "LXC",
            }
        )
    }
}

/// Mutable runtime record for a step — written to deploy-state.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub index: u32,
    pub id: String,
    pub label: String,
    pub executor: StepExecutor,
    #[serde(default)]
    pub status: StepStatus,
    /// Unix timestamp (seconds) when this step started.
    pub started_at: Option<u64>,
    /// Unix timestamp (seconds) when this step finished.
    pub finished_at: Option<u64>,
    /// Combined stdout/stderr captured from HOST or LXC.
    #[serde(default)]
    pub output: String,
    pub error: Option<String>,
}

impl StepRecord {
    pub fn from_step(step: &PipelineStep) -> Self {
        Self {
            index: step.index,
            id: step.id.to_string(),
            label: step.label.to_string(),
            executor: step.executor.clone(),
            status: StepStatus::Pending,
            started_at: None,
            finished_at: None,
            output: String::new(),
            error: None,
        }
    }
}
