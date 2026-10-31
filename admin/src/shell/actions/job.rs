//! The job: what a press becomes, and how the page sees it.

use serde::{Deserialize, Serialize};

use crate::core::actions::{ActionArgs, ActionKind, Refusal};
use crate::core::actions_progress::{self, Progress};

/// Where a job came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum Origin {
    Manual,
    Batch {
        batch: u64,
    },
    Schedule {
        schedule: String,
        slot: i64,
    },
    /// feat-platform-10: the final press of a form Claude drove
    /// (`homelab ui press confirm`), with the driving token's name.
    Claude {
        by: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    /// The host stood aside on purpose; nothing changed.
    Deferred,
    /// The dashboard refused before anything reached the host.
    Refused,
    /// The line dropped before the answer; History tells what happened.
    Unknown,
}

impl JobState {
    pub fn finished(self) -> bool {
        !matches!(self, JobState::Queued | JobState::Running)
    }
}

/// One job as the page shows it (the `action` event, `GET …/jobs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobView {
    pub job: u64,
    pub origin: Origin,
    pub stack: String,
    pub action: ActionKind,
    pub args: ActionArgs,
    pub state: JobState,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    /// The host's request ids, in the order they went out.
    pub reqs: Vec<u64>,
    /// The host's final message, or why the dashboard refused.
    pub message: Option<String>,
    /// feat-stacks-7: the same thing from a workstation.
    pub cli: Option<String>,
    pub progress: Option<Progress>,
    /// ask-kp11be-5: bytes moved so far in the transfer the job is in
    /// (restic backup and restore); None for a job with no byte counter or
    /// a host older than the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<actions_progress::JobBytes>,
    /// arch-self: this job restarts the dashboard; its end is read back
    /// after the restart.
    pub restarts_dashboard: bool,
    /// redesign-flows-6: the Update flow's own rows (`core::updateflow::
    /// FlowView`), for the page that follows this job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow: Option<serde_json::Value>,
    /// one-process-batch-2 (Kenny, 2026-10-23: "van begin tot einde moet dat
    /// 1 proces zijn, met een op voorhand bekend aantal stappen"): every
    /// step of the job, named before the first is sent, in the order they
    /// run ("kitchen · provision container" when the job spans stacks).
    /// Present only when every command of the job could be named exactly
    /// (`actions_progress::commands_plan_with`); the page then shows these
    /// names from the first frame instead of "Step N".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Vec<String>>,
}

pub(super) struct Job {
    pub(super) view: JobView,
}

/// Why a job stopped before the host answered.
pub(super) enum Stop {
    Refused(Refusal),
    Link(String),
}
