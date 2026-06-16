// Pipeline orchestration for CLIENT-driven stack deployment.
//
// Each sub-module is deliberately small (<150 lines) so that any individual
// file can be read and understood at a glance.
//
// Sub-module responsibilities:
//   step.rs      — PipelineStep and StepRecord types, StepStatus enum
//   state.rs     — deploy-state.json read/write (one file per stack)
//   stages.rs    — Canonical 20-step deployment pipeline definition
//   runner.rs    — Drives the pipeline: dispatches steps, tracks results
pub mod runner;
pub mod stages;
pub mod state;
pub mod step;

pub use runner::PipelineRunner;
pub use stages::DeployPipeline;
pub use state::{DeployState, load_deploy_state, save_deploy_state};
pub use step::{PipelineStep, StepExecutor, StepRecord, StepStatus};
