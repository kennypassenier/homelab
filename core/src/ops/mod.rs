//! Operations: step lists executed through the shared runner (AR3).

pub mod deploy;
pub mod guards;
pub mod util;

use crate::executor::Executor;
use crate::runner::Journal;
use crate::safety::SafetyConfig;
use crate::sink::Sink;

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
}
