//! Operations: step lists executed through the shared runner (AR3).

pub mod backup;
pub mod deploy;
pub mod destroy;
pub mod guards;
pub mod kea;
pub mod mirror;
pub mod patch;
pub mod resize;
pub mod selfupdate;
pub mod template;
pub mod update;
pub mod util;

use crate::error::CoreError;
use crate::executor::{pct_sh, CmdOutput, Executor};
use crate::runner::Journal;
use crate::safety::SafetyConfig;
use crate::sink::Sink;

/// A1/A2 guard shared by every mutating op that targets an existing
/// container: the vmid must not be on the no-touch list AND must carry the
/// expected `<vmid>-app-<stack>` hostname. Deploy/destroy embed this in
/// their pipelines; backup/restore/update call it up front.
pub(crate) async fn guard_target(
    exec: &dyn Executor,
    safety: &SafetyConfig,
    vmid: u16,
    expected_hostname: &str,
) -> Result<(), CoreError> {
    if safety.no_touch.contains(&vmid) {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} is on the no-touch list",
            vmid
        )));
    }
    let cfg = exec
        .run(&crate::executor::Cmd::new("pct", &["config", &vmid.to_string()], 30))
        .await?;
    if !cfg.success() {
        return Err(CoreError::Other(format!("vmid {} does not exist", vmid)));
    }
    let live = cfg
        .stdout
        .lines()
        .find(|l| l.starts_with("hostname:"))
        .map(|l| l.trim_start_matches("hostname:").trim().to_string())
        .unwrap_or_default();
    if live != expected_hostname {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} is '{}', expected '{}' — refusing",
            vmid, live, expected_hostname
        )));
    }
    Ok(())
}

/// Shared helper: run a shell script inside an LXC (re-exported for ops).
pub(crate) async fn util_pct_sh(
    exec: &dyn Executor,
    vmid: u16,
    script: &str,
    timeout_s: u64,
) -> Result<CmdOutput, CoreError> {
    pct_sh(exec, vmid, script, timeout_s).await
}

/// Everything an operation needs from the outside world. Constructed by the
/// host per request; constructed from mocks in tests.
pub struct OpCtx<'a> {
    pub exec: &'a dyn Executor,
    pub sink: &'a dyn Sink,
    pub journal: &'a dyn Journal,
    pub safety: SafetyConfig,
    /// e.g. /var/lib/homelab
    pub state_dir: String,
    /// Unix timestamp supplied by the caller — core never reads clocks.
    pub now_unix: u64,
    /// H2: OPNsense Kea reservation config; None = feature off.
    pub kea: Option<kea::KeaCfg>,
}
