//! Refuse before anything mutates: validation, safety gates, the backup
//! taken first, and the record as it stood before this deploy.

use crate::error::CoreError;
use crate::manifest;
use crate::runner::{Runner, StepOutcome};
use crate::safety;
use crate::state::{StackState, StateStore};

use super::run::{Failure, Run, log_info};

impl<'c> Run<'_, 'c> {
    /// D10: never trust the client — validate host-side too.
    pub(super) async fn validate(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, spec, m, _) = self.parts();
        manifest::validate(spec)?;
        // fix-88: only the host knows which container is the gateway, so
        // the port-80 rule (traefik-lan-host-header-bypass) is checked here,
        // still before anything changes.
        if let Some(fw) = m.firewall.as_ref().filter(|f| f.enabled)
            && m.vmid == ctx.safety.gateway_vmid
        {
            // tile-watch: the derived rule counts too — a gateway tile
            // that opens port 80 on itself is exactly what
            // traefik-lan-host-header-bypass forbids, derived or not.
            let effective = crate::firewall::with_tile_watch(
                fw,
                &m.network.ip,
                &m.tiles,
                ctx.tile_watch_source.as_deref().unwrap_or(""),
                &ctx.tile_watch_targets,
                // trusted-sources: the port-80 guard holds the declaration, not the
                // owner's own machine (`firewall::trusted_rules`).
                &[],
            );
            let p = crate::firewall::gateway_problems(&effective);
            if !p.is_empty() {
                return Err(CoreError::Validation(p.join("; ")));
            }
        }
        Ok(StepOutcome::Unchanged)
    }

    /// A1 + A2: refuse before anything mutates.
    pub(super) async fn safety_gates(&mut self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        self.exists = safety::check_deploy_target(exec, &ctx.safety, m).await?;
        Ok(StepOutcome::Unchanged)
    }

    /// redesign-stacks-6: the backup the dashboard promises ("each stack
    /// is backed up first"), before the first step that changes anything.
    /// Nested through this runner (fix-step-plan-nested), so its names are
    /// part of the one plan announced above. A failed backup ends the
    /// deploy here, with the stack exactly as it was: nothing before this
    /// point mutates, so there is nothing to mark incomplete.
    pub(super) async fn backup_first(&self, runner: &mut Runner<'c>) -> Result<(), Failure> {
        let (ctx, _, m, _) = self.parts();
        let names = crate::ops::backup::backup_plan_names(&m.stack_name);
        if !self.exists || m.storage.is_empty() {
            self.skip_backup(runner, &names);
            return Ok(());
        }
        let cfg = crate::ops::backup::BackupCfg {
            trigger: crate::ops::backup::BackupTrigger::PreDeploy,
            ..ctx.backup.clone()
        };
        let mut scope = crate::runner::Scope::Nested {
            parent: runner,
            prefix: format!("backup-{}", m.stack_name),
        };
        let backed = crate::ops::backup::backup_impl(ctx, m, &cfg, &mut scope).await;
        drop(scope);
        backed.map_err(|f| Failure {
            err: CoreError::SafetyAbort(format!(
                "refusing to deploy '{}': the backup taken first did not succeed :: {} \
                 :: the stack is unchanged; fix the backup and deploy again",
                m.stack_name,
                crate::error::OperatorError::from_core(&f.step, &f.err).why
            )),
            step: f.step,
        })
    }

    fn skip_backup(&self, runner: &mut Runner<'_>, names: &[String]) {
        let (ctx, _, m, _) = self.parts();
        log_info(
            ctx,
            format!(
                "[deploy] {}: nothing to back up first ({})",
                m.stack_name,
                if self.exists {
                    "the stack declares no storage"
                } else {
                    "the container does not exist yet"
                }
            ),
        );
        for n in names {
            runner.skip(n);
        }
    }

    /// step-22 / ask-8: the record as it stood before this deploy. What the
    /// stack file used to declare is the only way to know what has left it —
    /// a mount, a native unit, a route — and so what to take away. Read after
    /// the safety gates: a refused target reads nothing.
    pub(super) async fn load_prior(&mut self) {
        let (ctx, _, m, exec) = self.parts();
        let prior: Option<StackState> = StateStore::new(exec, &ctx.state_dir)
            .load()
            .await
            .ok()
            .and_then(|s| s.stacks.get(&m.stack_name).cloned());
        self.prior_manifest = prior.as_ref().and_then(|p| p.manifest.clone());
        self.prior = prior;
        self.dropped_natives = self.compute_dropped_natives();
    }

    /// ask-8 (`Stoppen, data bewaren`): only a unit the stack FILE used to
    /// declare is retired. One registered by `homelab adopt` that no stack
    /// file ever named is not the deploy's to take away.
    ///
    /// fix-186: a unit this deploy still declares as a native is never
    /// dropped — and neither is one that moved from `natives:` to `apps:`
    /// under the same name (a docker-to-native or native-to-docker
    /// conversion is not a departure). Only a name declared NOWHERE in the
    /// new manifest left the stack.
    fn compute_dropped_natives(&self) -> Vec<String> {
        let m = &self.spec.manifest;
        self.prior_manifest
            .as_ref()
            .map(|pm| {
                pm.natives
                    .iter()
                    .filter(|u| !m.natives.contains(u) && !m.apps.contains(u))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}
