// Pipeline runner — drives a DeployPipeline step by step.
//
// The runner is not async itself; the CLIENT event loop calls
// `advance()` after each step completes.  The runner decides:
//   - Which step to run next.
//   - Whether to stop (on failure) or continue.
//   - When to persist state to deploy-state.json.
//
// Step execution is delegated back to the caller through `PipelineAction`
// so the runner never has to know about HTTP clients or tokio channels.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::pipeline::state::{DeployState, PipelineStatus, save_deploy_state};
use crate::pipeline::step::{StepRecord, StepStatus};
use crate::pipeline::stages::DeployPipeline;

/// Instructions returned to the CLIENT event loop after each runner operation.
#[derive(Debug)]
pub enum PipelineAction {
    /// Emit a numbered step-header log line, then dispatch this step to the
    /// indicated executor.  The `rpc_kind` is the WebSocket request `kind` field.
    DispatchStep {
        index: u32,
        total: u32,
        id: String,
        label: String,
        rpc_kind: String,
        header_line: String,
    },
    /// All steps have completed (or skipped).
    Finished { success: bool },
    /// An unrecoverable problem before the pipeline could start.
    Aborted { reason: String },
}

pub struct PipelineRunner {
    pub stack: String,
    pub pipeline: DeployPipeline,
    pub state: DeployState,
    /// The next step index to dispatch (1-based).
    next: u32,
}

impl PipelineRunner {
    /// Create a fresh pipeline runner for a stack.
    /// If `resume_from_step` is > 0, steps before that index are marked Skipped.
    pub fn new(
        stack: String,
        vmid: u32,
        pipeline: DeployPipeline,
        resume_from_step: u32,
    ) -> Self {
        let total = pipeline.total();
        let mut state = DeployState::new(&stack, vmid, pipeline.name, total);

        for step in &pipeline.steps {
            let mut rec = StepRecord::from_step(step);
            if resume_from_step > 0 && step.index < resume_from_step {
                rec.status = StepStatus::Skipped;
            }
            state.steps.push(rec);
        }

        let next = if resume_from_step > 0 {
            resume_from_step.min(total)
        } else {
            1
        };

        Self { stack, pipeline, state, next }
    }

    /// Restore a runner from a previously saved state.
    pub fn from_saved(stack: String, pipeline: DeployPipeline, state: DeployState) -> Self {
        // Resume from the first non-terminal step.
        let next = state
            .steps
            .iter()
            .find(|s| !s.status.is_terminal())
            .map(|s| s.index)
            .unwrap_or(pipeline.total() + 1);
        Self { stack, pipeline, state, next }
    }

    /// Advance the pipeline.  Returns the action the event loop must perform next.
    pub fn advance(&mut self) -> PipelineAction {
        if self.next > self.pipeline.total() {
            self.state.status = PipelineStatus::Succeeded;
            self.state.last_completed_step = self.pipeline.total();
            self.state.touch();
            let _ = save_deploy_state(&self.stack, &self.state);
            return PipelineAction::Finished { success: true };
        }

        let step = match self.pipeline.step(self.next) {
            Some(s) => s.clone(),
            None => {
                return PipelineAction::Aborted {
                    reason: format!("no step at index {}", self.next),
                }
            }
        };

        // Mark step as running in state.
        self.state.status = PipelineStatus::Running;
        if let Some(rec) = self.state.step_mut(step.index) {
            rec.status = StepStatus::Running;
            rec.started_at = Some(unix_now());
        }
        self.state.touch();
        let _ = save_deploy_state(&self.stack, &self.state);

        let rpc_kind = step_to_rpc_kind(step.id);
        let header_line = step.header(&self.stack);

        PipelineAction::DispatchStep {
            index: step.index,
            total: step.total,
            id: step.id.to_string(),
            label: step.label.to_string(),
            rpc_kind,
            header_line,
        }
    }

    /// Record the result of the last dispatched step and persist to disk.
    /// Returns true if the pipeline should continue, false if it should halt.
    pub fn record_step_result(
        &mut self,
        index: u32,
        ok: bool,
        output: String,
        error: Option<String>,
    ) -> bool {
        if let Some(rec) = self.state.step_mut(index) {
            rec.status = if ok { StepStatus::Succeeded } else { StepStatus::Failed };
            rec.finished_at = Some(unix_now());
            rec.output = output;
            rec.error = error;
        }

        if ok {
            self.state.last_completed_step = index;
            self.next = index + 1;
        } else {
            self.state.status = PipelineStatus::Failed;
            self.state.resume_from_step = index; // can be retried from here
        }

        self.state.touch();
        let _ = save_deploy_state(&self.stack, &self.state);
        ok
    }
}

/// Map a step id to the WebSocket RPC `kind` sent to HOST or LXC.
fn step_to_rpc_kind(step_id: &str) -> String {
    match step_id {
        // HOST steps — HOST daemon handles these via provision_step_request
        "lxc_create"
        | "lxc_configure"
        | "storage_init"
        | "lxc_start"
        | "lxc_wait_ready"
        | "appdata_mkdir"
        | "inject_secrets"
        | "install_system"
        | "install_latch"
        | "configure_git"
        | "install_ssh"
        | "install_daemon" => format!("provision_step:{}", step_id),

        // LXC steps — LXC daemon handles these via sync_step_request
        "git_fetch"
        | "latch_pull"
        | "pre_sync_hook"
        | "compose_pull"
        | "compose_up"
        | "orphan_gc" => format!("sync_step:{}", step_id),

        // CLIENT-local steps (no RPC needed; runner handles inline)
        _ => format!("client_step:{}", step_id),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
