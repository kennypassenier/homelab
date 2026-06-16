// Canonical pipeline definitions.
//
// Each pipeline is a list of PipelineStep values.
// To change the order or add/remove steps, edit this file only.
//
// Current pipelines:
//   stack_deploy_full  — provision a brand-new stack (20 steps)
//   stack_deploy_sync  — re-sync an already-running stack (7 steps)

use super::step::{PipelineStep, StepExecutor};

pub const PIPELINE_FULL: &str = "stack_deploy_full";
pub const PIPELINE_SYNC: &str = "stack_deploy_sync";

/// A named pipeline is just a sorted Vec<PipelineStep>.
pub struct DeployPipeline {
    pub name: &'static str,
    pub steps: Vec<PipelineStep>,
}

impl DeployPipeline {
    /// Full 20-step pipeline used when a stack has never been provisioned.
    pub fn full() -> Self {
        let defs: &[(&str, &str, StepExecutor)] = &[
            // ── HOST PHASE — LXC creation and bootstrap ──────────────────────
            ("pre_flight",          "Validate stack configuration",            StepExecutor::Client),
            ("lxc_create",         "Create Proxmox LXC container",             StepExecutor::Host),
            ("lxc_configure",      "Apply CPU / RAM / disk settings",          StepExecutor::Host),
            ("storage_init",       "Initialise /opt/appdata/<stack> on host",  StepExecutor::Host),
            ("lxc_start",          "Start LXC container",                      StepExecutor::Host),
            ("lxc_wait_ready",     "Wait for LXC to become reachable",         StepExecutor::Host),
            ("appdata_mkdir",      "Create /appdata inside LXC",               StepExecutor::Host),
            ("inject_secrets",     "Inject LATCH_* credentials",               StepExecutor::Host),
            ("install_system",     "Install apt packages and Docker",           StepExecutor::Host),
            ("install_latch",      "Install latch CLI binary",                  StepExecutor::Host),
            ("configure_git",      "Sparse git checkout init",                 StepExecutor::Host),
            ("install_ssh",        "Install SSH keys from GitHub",              StepExecutor::Host),
            ("install_daemon",     "Install and start LXC daemon service",      StepExecutor::Host),
            // ── LXC PHASE — after the LXC daemon WebSocket connects ──────────
            ("lxc_connect",        "Establish WebSocket to LXC daemon",        StepExecutor::Client),
            ("git_fetch",          "git fetch + reset --hard origin/main",     StepExecutor::Lxc),
            ("latch_pull",         "Pull secrets via latch",                   StepExecutor::Lxc),
            ("compose_prep",       "Prepare bind-mounted files from compose", StepExecutor::Lxc),
            ("compose_pull",       "docker compose pull for each app",          StepExecutor::Lxc),
            ("compose_up",         "docker compose up --remove-orphans",       StepExecutor::Lxc),
            ("orphan_gc",          "Garbage collect removed apps",             StepExecutor::Lxc),
        ];
        Self::build(PIPELINE_FULL, defs)
    }

    /// Sync-only 7-step pipeline used when the container is already running.
    pub fn sync() -> Self {
        let defs: &[(&str, &str, StepExecutor)] = &[
            ("lxc_connect",    "Verify LXC daemon WebSocket connection",   StepExecutor::Client),
            ("git_fetch",      "git fetch + reset --hard origin/main",     StepExecutor::Lxc),
            ("latch_pull",     "Pull secrets via latch",                   StepExecutor::Lxc),
            ("compose_prep",   "Prepare bind-mounted files from compose", StepExecutor::Lxc),
            ("compose_pull",   "docker compose pull for each app",          StepExecutor::Lxc),
            ("compose_up",     "docker compose up --remove-orphans",       StepExecutor::Lxc),
            ("orphan_gc",      "Garbage collect removed apps",             StepExecutor::Lxc),
        ];
        Self::build(PIPELINE_SYNC, defs)
    }

    fn build(name: &'static str, defs: &[(&'static str, &'static str, StepExecutor)]) -> Self {
        let total = defs.len() as u32;
        let steps = defs
            .iter()
            .enumerate()
            .map(|(i, (id, label, executor))| PipelineStep {
                index: (i + 1) as u32,
                total,
                id,
                label,
                executor: executor.clone(),
            })
            .collect();
        Self { name, steps }
    }

    /// Return the step at a given 1-based index, or None.
    pub fn step(&self, index: u32) -> Option<&PipelineStep> {
        self.steps.get((index as usize).saturating_sub(1))
    }

    pub fn total(&self) -> u32 {
        self.steps.len() as u32
    }
}
