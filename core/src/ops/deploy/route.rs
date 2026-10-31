//! H1: the single allowed cross-stack write — the gateway's route files.

use crate::error::CoreError;
use crate::executor::pct_sh;
use crate::manifest::{self, GatewayRoute};
use crate::ops::util::push_content;
use crate::runner::StepOutcome;
use crate::safety;

use super::run::{Run, log_info};

impl<'a, 'c> Run<'a, 'c> {
    /// fix-91 (routes-outside-repo-unvalidated, 2026-09-27): the stack's
    /// `extra_routes` go the same way, under the name each already has on the
    /// gateway, so a file brought into the repository is rewritten with the
    /// bytes it already holds.
    pub(super) fn declared_routes(&self) -> Vec<&'a GatewayRoute> {
        let spec = self.spec;
        spec.gateway_route
            .iter()
            .chain(spec.extra_routes.iter())
            .collect()
    }

    pub(super) async fn gateway_route(
        &self,
        routes: &[&GatewayRoute],
    ) -> Result<StepOutcome, CoreError> {
        let (ctx, _, _, exec) = self.parts();
        let mut changed = false;
        for route in routes {
            let dest =
                safety::check_gateway_route(&ctx.safety, route.gateway_vmid, &route.filename)?;
            // H6 hardening: when the routes dir lives under /appdata/ it is a
            // HOST path bind-mounted into the gateway — write it host-side so
            // route fragments survive gateway recreation. The legacy /opt
            // path keeps the pct-push behavior until the platform migration.
            if ctx.safety.gateway_routes_dir.starts_with("/appdata/") {
                exec.write_file(&dest, &route.content, 0o644).await?;
                changed = true;
            } else if push_content(exec, route.gateway_vmid, &dest, &route.content, "644").await? {
                changed = true;
            }
            log_info(
                ctx,
                format!("[route] {} (file-provider watch reloads)", dest),
            );
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    /// step-22: a route the stack no longer declares goes. The file is
    /// named `<vmid>-app-<stack>.yml`, so dropping `gateway_route:` or moving
    /// to another vmid left the old one behind: still routing a hostname to a
    /// container that is gone or no longer this stack. Only when the stack had
    /// a record: a stack deployed for the first time has nothing of its own to
    /// take away.
    /// fix-41: only the route file a deploy of this stack recorded writing is
    /// retired, when the stack no longer declares it or now writes another
    /// name (a changed vmid). A route written by hand on the gateway is never
    /// this stack's to remove.
    /// fix-91: the recorded `extra_routes` files follow the same rule — every
    /// file this stack's last deploy recorded and this deploy does not write.
    /// fix-199: same field-keeping rule as "retire dropped" — an older
    /// client's spec may simply have no route this stack always declared,
    /// because its struct predates routes being removable at all; its
    /// absence must not read as "retire every route this stack ever wrote".
    pub(super) fn stale_routes(&self, routes: &[&GatewayRoute]) -> Vec<String> {
        let spec = self.spec;
        if !manifest::client_knows(spec.client_schema, "routes") {
            log_info(
                self.ctx,
                format!(
                    "[retire gateway route] skipped — the deploying client (schema {}) predates this \
                     declared-routes list; nothing was retired on its behalf",
                    spec.client_schema
                ),
            );
            return Vec::new();
        }
        let declared: Vec<&str> = routes.iter().map(|r| r.filename.as_str()).collect();
        self.prior
            .as_ref()
            .map(|p| {
                p.route_file
                    .iter()
                    .chain(p.extra_route_files.iter())
                    .filter(|f| !declared.contains(&f.as_str()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) async fn retire_gateway_route(
        &self,
        stale: &[String],
    ) -> Result<StepOutcome, CoreError> {
        let (ctx, _, _, exec) = self.parts();
        let mut changed = false;
        for f in stale {
            let dest = format!("{}/{}", ctx.safety.gateway_routes_dir, f);
            let q = crate::ops::util::shq(&dest);
            // Same machine and path destroy removes it from.
            let out = pct_sh(
                exec,
                ctx.safety.gateway_vmid,
                &format!("if [ -f {q} ]; then rm -f {q} && echo removed; fi"),
                30,
            )
            .await?;
            if out.stdout.contains("removed") {
                log_info(
                    ctx,
                    format!(
                        "[route] {} removed — the stack file no longer declares it",
                        dest
                    ),
                );
                changed = true;
            }
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }
}
