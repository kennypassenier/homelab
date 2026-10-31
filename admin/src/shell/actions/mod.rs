//! milestone act: the dashboard acts. feat-stacks-4 (every action per
//! stack), feat-stacks-5 (several stacks at once), feat-stacks-6 (roll
//! back), feat-stacks-7 (copy as CLI command) and feat-ops-6 (step progress
//! with the expected duration).
//!
//! A press becomes a job in ONE queue, run one at a time in the order they
//! came, which is the order the host's own queue would run them in anyway
//! (AR12). The route answers at once with the job id; what happens next goes
//! over the live channel:
//!
//! * `action`: the job, whenever its state changes (queued, running, done,
//!   failed, deferred, refused, unknown);
//! * `action_log`: every host line of the job's request;
//! * `action_progress`: "step n/m, expected x" (feat-ops-6);
//! * `action_batch`: a batch's jobs and totals, after each of its jobs.
//!
//! The pieces that touch the world are traits ([`HostPort`], [`Publish`],
//! [`StackFiles`]) so the act tests drive the real queue against a mock
//! host, a recording channel and stack files in memory.
//!
//! Layout (one responsibility per file):
//!
//! * `ports`, `stackfiles`, `repofiles`, `repo_git`, `repo_stacks`: the
//!   seams to the world and the working copy behind [`StackFiles`];
//! * `job`, `state`, `settings`, `registry`, `secret_stage`, `history`,
//!   `fleet`: the queue's data and the small reads and writes around it;
//! * `press`, `preview`, `batches`, `cancel`, `plan`: what a press does
//!   before and after a job is queued;
//! * `material`, `diffs`, `options`: what a job reads before it runs;
//! * `run`, `execute`, `follow`, `host_wait`: the worker that runs a job;
//! * `routes`, `routes_jobs`: the HTTP surface; `mount`, `mount_start`: the
//!   wiring into the app.

mod apply;
mod batches;
mod cancel;
mod diffs;
mod execute;
mod fleet;
mod follow;
mod history;
mod host_wait;
mod job;
mod material;
mod mount;
mod mount_routes;
mod mount_setup;
mod mount_start;
mod options;
mod plan;
mod ports;
mod press;
mod preview;
mod preview_route;
mod registry;
mod repo_git;
mod repo_stacks;
mod repofiles;
mod routes;
mod routes_jobs;
mod run;
mod secret_stage;
mod settings;
mod stackfiles;
mod state;

#[path = "../actions_flow.rs"]
mod flow;
#[path = "../actions_home.rs"]
mod home;
#[path = "../actions_jobs.rs"]
mod jobs_store;
pub use flow::ImageCommitter;
pub(crate) use flow::stacks_cell;
#[cfg(test)]
pub(crate) use flow::{put_record as put_test_record, records_since as records_since_test};

pub use job::{JobState, JobView, Origin};
pub use mount::mount;
pub use ports::{HostPort, PauseGate, Publish, SettledFn};
pub use routes::router;
pub use stackfiles::{CommitInfo, LocalStacks, RepoFiles, StackFiles, is_head};
pub use state::{Actions, ActionsDeps, CANCEL_EXEC_SINCE, JOB_LOG_MAX};

use job::{Job, Stop};
use state::{HISTORY_WINDOW_S, KEEP_JOBS, SECRET_STAGE_TTL};

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Unix seconds, injected so tests can hold time still.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn system_clock() -> Clock {
    // redesign-final: the dashboard's one clock (injected in a demo run).
    Arc::new(|| crate::shell::clock::now_s() as i64)
}
