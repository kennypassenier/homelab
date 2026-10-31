//! The deploy operation (D1): validate → safety gates → storage → provision →
//! bootstrap → guards → intent to repo → push files/env → start apps →
//! verify → gateway route → record state. Every step runs through the shared
//! runner (AR3); every side effect through the Executor (AR2).
//!
//! One deploy is a `Run` (what it carries from step to step); `phases`
//! orders the steps and each step lives in a file named for what it does.

#[macro_use]
mod macros;

mod app_settings;
mod autorestore;
mod bootstrap;
mod checks;
mod checks_after;
mod commit;
mod compose;
mod firewall;
mod gate;
mod gc;
mod hardware;
mod health;
mod incomplete;
mod logshipper;
mod memory;
mod metrics;
mod natives;
mod natives_prep;
mod natives_start;
mod natives_vault;
mod orphans;
mod phases;
mod plan;
mod provision;
mod provision_attach;
mod provision_clone;
mod provision_create;
mod provision_mounts;
mod push;
mod push_secrets;
mod push_verify;
mod reconcile;
mod record;
mod registry_cache;
mod report;
mod restore_checks;
mod retire;
mod retire_native;
mod route;
mod run;
mod start;
mod start_pull;
mod start_restart;
mod storage;
mod storage_ownership;
mod vault;

pub use compose::compose_reports_created;
pub use memory::{MIN_MEMORY_AVAILABLE, memory_available_fraction};
pub use orphans::{generated_dirs, orphan_files, orphan_files_keeping};
pub use plan::{STEPS, plan_names};
pub(crate) use restore_checks::record_restore_checks;
pub use vault::vault_key;

use crate::executor::TracingExecutor;
use crate::manifest::DeploySpec;
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Runner};
use crate::sink::Level;

use run::Run;

pub async fn deploy(ctx: &OpCtx<'_>, spec: &DeploySpec) -> OperationReport {
    let m = &spec.manifest;
    let op = format!("deploy-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    runner.plan(&plan_names(spec));
    runner.log(
        Level::Info,
        format!("[sync][run ] deploy {} (vmid {})", m.stack_name, m.vmid),
    );
    // Every command flows through the tracing decorator, so transcripts are
    // both streamed (F2) and captured for incident replay (AR16).
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let mut run = Run::new(ctx, spec, &texec, op);
    match run.drive(&mut runner).await {
        Ok(()) => runner.finish_ok(),
        Err(f) => runner.finish_err(&f.step, &f.err),
    }
}
