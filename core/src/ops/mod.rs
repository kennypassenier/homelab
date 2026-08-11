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
