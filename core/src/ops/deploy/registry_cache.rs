//! D60: which upstreams does the registry cache answer for, right now?

use crate::error::CoreError;
use crate::executor::pct_sh;
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    /// Asked from inside the container that is about to pull, because that is
    /// the only place the answer means anything. A cache that does not answer
    /// is not an error: the image keeps naming its own origin and the pull
    /// goes out to the internet exactly as it did before there was a cache.
    pub(super) async fn registry_cache(&mut self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let Some(cache) = ctx.registry_cache.as_ref() else {
            return Ok(StepOutcome::Unchanged);
        };
        if m.native_only {
            return Ok(StepOutcome::Unchanged);
        }
        for up in &cache.upstreams {
            let probe = pct_sh(
                exec,
                m.vmid,
                &format!(
                    "curl -fsS -m 3 -o /dev/null http://{}:{}/v2/ && echo UP",
                    cache.host, up.port
                ),
                30,
            )
            .await;
            if matches!(&probe, Ok(o) if o.stdout.contains("UP")) {
                self.cache_up.push(up.registry.clone());
            }
        }
        Ok(StepOutcome::Unchanged)
    }

    /// Says which registries the pulls will go through.
    pub(super) fn log_cache(&self) {
        let (ctx, _, m, _) = self.parts();
        let Some(cache) = ctx.registry_cache.as_ref().filter(|_| !m.native_only) else {
            return;
        };
        if self.cache_up.is_empty() {
            log_info(
                ctx,
                format!(
                    "[cache] {} answered for nothing — every image keeps its own registry",
                    cache.host
                ),
            );
        } else {
            log_info(
                ctx,
                format!(
                    "[cache] pulling via {} for: {}",
                    cache.host,
                    self.cache_up.join(", ")
                ),
            );
        }
    }
}
