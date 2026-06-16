// Bootstrap orchestrator — drives the 13-step HOST bootstrap phase and
// emits a step-header log event before each named step.
//
// Each functional concern lives in a focused sub-module:
//   container.rs — pct primitives (stop/start/exec/wait)
//   configure.rs — storage, TUN/GPU passthrough, /appdata creation
//   install.rs   — secret injection, apt packages, Docker, latch CLI
//   git.rs       — sparse checkout + SSH key installation
//   daemon.rs    — LXC daemon binary + systemd service

pub mod container;
pub mod configure;
pub mod daemon;
pub mod git;
pub mod install;

use std::time::Duration;

use crate::provision::StackIntent;
use container::{pct_start, pct_stop, wait_for_ready};
use configure::{create_appdata_dir, setup_gpu_passthrough, setup_storage, setup_tun_device};
use daemon::install_lxc_daemon;
use git::{install_ssh_keys, setup_git_sparse_checkout};
use install::{inject_secrets, install_latch, install_system_deps};

#[derive(Debug, Clone)]
pub struct BootstrapResult {
    pub success: bool,
    pub duration: Duration,
    pub error: Option<String>,
}

/// Bootstrap a newly created LXC container.
///
/// `log` receives `(level, message)` pairs and is used both for normal
/// progress reporting and for pipeline step-header markers.
pub fn bootstrap_lxc(
    vmid: u32,
    intent: &StackIntent,
    log: &dyn Fn(&str, &str),
) -> Result<BootstrapResult, String> {
    let start = std::time::Instant::now();
    let total: u32 = 11; // HOST bootstrap steps (after LXC already created by provision.rs)
    let mut step = 0u32;

    macro_rules! step {
        ($label:expr, $body:expr) => {{
            step += 1;
            log("step", &format!("[STEP {:>2}/{}] {} — {} (HOST)", step, total, $label, intent.stack_name));
            log("info", &format!("[bootstrap] {}", $label));
            $body?;
        }};
    }

    step!("Stop LXC for pre-boot configuration",   pct_stop(vmid));
    step!("Configure host storage",                 setup_storage(vmid, intent));

    if intent.tun_device.unwrap_or(false) {
        step!("Configure TUN device passthrough",   setup_tun_device(vmid));
    } else {
        step += 1; // keep numbering stable
    }

    if intent.gpu_passthrough.unwrap_or(false) {
        step!("Configure GPU passthrough",          setup_gpu_passthrough(vmid));
    } else {
        step += 1;
    }

    step!("Start LXC container",                    pct_start(vmid));
    step!("Wait for LXC to become reachable",        wait_for_ready(vmid, Duration::from_secs(60)));
    step!("Create /appdata inside LXC",             create_appdata_dir(vmid));
    step!("Inject LATCH_* credentials",             inject_secrets(vmid));
    step!("Install system packages and Docker",     install_system_deps(vmid));
    step!("Install latch CLI binary",               install_latch(vmid));

    let github_user = std::env::var("GITHUB_USERNAME").unwrap_or_else(|_| "kennypassenier".to_string());
    step!("Configure sparse Git checkout",           setup_git_sparse_checkout(vmid, &intent.stack_name));
    step!("Install SSH keys from GitHub",            install_ssh_keys(vmid, &github_user));
    step!("Install and start LXC daemon service",   install_lxc_daemon(vmid, &intent.stack_name));

    let dur = start.elapsed();
    log("ok", &format!("[bootstrap] Bootstrap complete for LXC {} in {:.1}s", vmid, dur.as_secs_f64()));

    Ok(BootstrapResult { success: true, duration: dur, error: None })
}
