//! The order of a deploy: each phase runs its steps through the runner and
//! stops at the first one that fails.

use crate::firewall::TileWatcher;
use crate::runner::Runner;

use super::run::{Failure, Run};

type Phase = Result<(), Failure>;

impl<'a, 'c> Run<'a, 'c> {
    /// Every phase in order, then what the deploy leaves behind.
    pub(super) async fn drive(&mut self, runner: &mut Runner<'c>) -> Phase {
        self.phase_gate(runner).await?;
        self.phase_measure(runner).await?;
        self.phase_provision(runner).await?;
        self.phase_bootstrap(runner).await?;
        self.phase_deliver(runner).await?;
        self.phase_start(runner).await?;
        self.phase_routes(runner).await?;
        self.phase_collect(runner).await?;
        self.phase_services(runner).await?;
        self.phase_prove(runner).await?;
        self.report(runner).await;
        Ok(())
    }

    /// D10, A1 + A2, the backup taken first, the record from before.
    async fn phase_gate(&mut self, runner: &mut Runner<'c>) -> Phase {
        step!(runner, self, "validate", self.validate().await);
        step!(runner, self, "safety gates", self.safety_gates().await);
        if self.spec.backup_first {
            self.backup_first(runner).await?;
        }
        self.load_prior().await;
        Ok(())
    }

    /// Readings and refusals before anything is created.
    async fn phase_measure(&mut self, runner: &mut Runner<'c>) -> Phase {
        step!(runner, self, "baseline", self.baseline().await);
        self.log_baseline();
        step!(runner, self, "registry cache", self.registry_cache().await);
        self.log_cache();
        step!(
            runner,
            self,
            "hardware readiness",
            self.hardware_readiness().await
        );
        self.log_gpu();
        step!(runner, self, "host storage", self.host_storage().await);
        step!(
            runner,
            self,
            "auto-restore check",
            self.auto_restore_check().await
        );
        step!(
            runner,
            self,
            "metrics discovery",
            self.metrics_discovery().await
        );
        Ok(())
    }

    /// The container's firewall and the container itself.
    async fn phase_provision(&mut self, runner: &mut Runner<'c>) -> Phase {
        let (_, _, m, _) = self.parts();
        match m.firewall.as_ref() {
            Some(fw) => {
                step!(runner, self, "firewall", self.firewall(fw).await);
            }
            None => runner.skip("firewall"),
        }
        step!(
            runner,
            self,
            "provision container",
            self.provision_container().await
        );
        match m.firewall.as_ref().filter(|f| f.enabled) {
            Some(fw) if !self.exists => {
                let name = "firewall after create";
                step!(runner, self, name, self.firewall_after_create(fw).await);
            }
            _ => runner.skip("firewall after create"),
        }
        match self.watcher_to_refresh() {
            Some((watcher, source)) => {
                let name = "refresh the tile watcher's firewall";
                step!(
                    runner,
                    self,
                    name,
                    self.refresh_watcher_firewall(watcher, source).await
                );
            }
            None => runner.skip("refresh the tile watcher's firewall"),
        }
        Ok(())
    }

    /// Skipped when there is no watcher, or this deploy IS the watcher (its
    /// own "firewall" step already derived its OUT rule from the same fleet
    /// targets).
    fn watcher_to_refresh(&self) -> Option<(&'a TileWatcher, &'a str)> {
        let (ctx, _, m, _) = self.parts();
        let watcher = ctx.tile_watch_watcher.as_ref()?;
        let source = ctx
            .tile_watch_source
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        (watcher.stack != m.stack_name).then_some((watcher, source))
    }

    /// systemd up, docker in, guards and rotation.
    async fn phase_bootstrap(&mut self, runner: &mut Runner<'c>) -> Phase {
        step!(
            runner,
            self,
            "wait for systemd",
            self.wait_for_systemd().await
        );
        step!(
            runner,
            self,
            "bootstrap docker",
            self.bootstrap_docker().await
        );
        step!(runner, self, "runaway guards", self.runaway_guards().await);
        step!(runner, self, "log rotation", self.log_rotation().await);
        Ok(())
    }

    /// What left the files goes; what is declared goes in.
    async fn phase_deliver(&mut self, runner: &mut Runner<'c>) -> Phase {
        if self.files_removal_known() {
            step!(runner, self, "retire dropped", self.retire_dropped().await);
        } else {
            runner.skip("retire dropped");
        }
        step!(runner, self, "commit intent", self.commit_intent().await);
        let check = self.push_check();
        stepv!(
            runner,
            self,
            "push files",
            self.push_files().await,
            check.run().await
        );
        Ok(())
    }

    async fn phase_start(&mut self, runner: &mut Runner<'c>) -> Phase {
        step!(runner, self, "start apps", self.start_apps().await);
        step!(
            runner,
            self,
            "storage ownership",
            self.storage_ownership().await
        );
        step!(runner, self, "verify health", self.verify_health().await);
        Ok(())
    }

    /// H1 and step-22: the gateway's route files.
    async fn phase_routes(&mut self, runner: &mut Runner<'c>) -> Phase {
        let routes = self.declared_routes();
        if routes.is_empty() {
            runner.skip("gateway route");
        } else {
            step!(
                runner,
                self,
                "gateway route",
                self.gateway_route(&routes).await
            );
        }
        let stale = self.stale_routes(&routes);
        if stale.is_empty() {
            runner.skip("retire gateway route");
        } else {
            step!(
                runner,
                self,
                "retire gateway route",
                self.retire_gateway_route(&stale).await
            );
        }
        Ok(())
    }

    /// D3: files and apps the stack no longer declares.
    async fn phase_collect(&mut self, runner: &mut Runner<'c>) -> Phase {
        self.apps_removal = self.apps_removal_known();
        step!(runner, self, "orphan files", self.orphan_files().await);
        step!(
            runner,
            self,
            "garbage collect",
            self.garbage_collect().await
        );
        Ok(())
    }

    /// Log shipper, native units, the state record.
    async fn phase_services(&mut self, runner: &mut Runner<'c>) -> Phase {
        match self.ctx.loki_url.as_deref() {
            Some(loki) => {
                step!(runner, self, "log shipper", self.log_shipper(loki).await);
            }
            None => runner.skip("log shipper"),
        }
        step!(runner, self, "native units", self.native_units().await);
        step!(runner, self, "record state", self.record_state().await);
        Ok(())
    }

    /// S2 and J1: the finished whole is asked, and the service checks judged.
    async fn phase_prove(&mut self, runner: &mut Runner<'c>) -> Phase {
        step!(runner, self, "reconcile", self.reconcile().await);
        step!(runner, self, "app settings", self.app_settings().await);
        step!(runner, self, "service checks", self.service_checks().await);
        Ok(())
    }
}
