//! The route-mounting steps of `mount`, in the order it runs them.

use std::sync::Arc;
use std::time::Duration;

use super::mount::Wiring;
use super::mount_setup::demo_home_address;
use super::{Actions, StackFiles};
use crate::shell::actions_notify;
use crate::shell::drive::{Driver, LiveTiming};
use crate::shell::edit::{EditCommitter, EditCtx};
use crate::shell::homeaddress::HomeAddress;
use crate::shell::hostlog::HostLog;
use crate::shell::releases::Releases;
use crate::shell::scheduler::Scheduler;
use crate::shell::workcopy::WorkingCopy;

impl Wiring<'_> {
    /// The stack editor, the Update flow's commit and undo records, the kept
    /// jobs, and the Backups and Retired pages' read-only routes.
    pub(super) fn mount_edit(&mut self, wc: &Arc<WorkingCopy>, actions: &Actions) -> EditCtx {
        let edit_ctx = EditCtx {
            wc: wc.clone(),
            actions: actions.clone(),
            host: self.host.clone(),
            shared: self.shared.clone(),
            publish: self.publish.clone(),
        };
        self.app
            .dashboard_routes(crate::shell::edit::router(edit_ctx.clone()));
        // redesign-flows-6: the Update flow commits through the stack editor
        // and keeps its undo records beside the notifications.
        actions.set_committer(Arc::new(EditCommitter(edit_ctx.clone())));
        actions.set_flow_store(self.db.clone());
        // jobs-kept-1: the kept jobs survive a restart of the dashboard.
        actions.set_job_store(self.db.clone());
        // feat-backup-1/3: the Backups page's read-only routes.
        self.app
            .dashboard_routes(crate::shell::backups::router(self.host.clone()));
        // feat-retired-1: the Retired page's one read-only route.
        self.app
            .dashboard_routes(crate::shell::retired::router(self.host.clone()));
        edit_ctx
    }

    /// feat-platform-10: the driver, its catch-up route and its relay; it
    /// drives the edit forms through the same editor.
    pub(super) fn mount_driver(&mut self, actions: &Actions, edit_ctx: &EditCtx) -> Driver {
        let driver = Driver::with_edit(
            actions.clone(),
            self.shared.clone(),
            self.publish.clone(),
            self.clock.clone(),
            Some(edit_ctx.clone()),
        );
        driver.set_timing(LiveTiming {
            announce: Duration::from_millis(self.cfg.live_announce_ms),
            max_pause: Duration::from_secs(self.cfg.live_max_pause_s),
        });
        driver.set_limits(self.cfg.drive_idle_s, self.cfg.drive_release_after_job_s);
        // liveview-final-button-1: nothing changes unless a tab watches.
        driver.require_follower();
        // fix-172: the queue now holds between two jobs while Live view is
        // paused; the driver is the only thing that knows about Live view, so
        // it registers itself once it exists (the queue was built first).
        actions.set_pause_gate(Arc::new(driver.clone()));
        self.app
            .dashboard_routes(crate::shell::drive::router(driver.clone()));
        // feat-secrets-1/2: the Secrets page (list, reveal, stage); the write
        // itself rides `ActionKind::ChangeSecret` through `edit_ctx.actions`.
        // redesign-3.71: after the driver, which tells Claude's clicks apart.
        self.app.dashboard_routes(crate::shell::secrets::router(
            crate::shell::secrets::SecretsCtx {
                edit: edit_ctx.clone(),
                driver: Some(driver.clone()),
                viewer_name: self.cfg.viewer_name.clone(),
            },
        ));
        #[cfg(feature = "demo-host")]
        if self.demo_host {
            self.app
                .dashboard_routes(crate::shell::drive::demo_router(driver.clone()));
        }
        driver
    }

    /// fix-231: the Fleet view's Update of a stale pin asks for its new
    /// reference here; the demo host's registry is made up, never asked.
    pub(super) fn mount_pin_updates(&mut self, wc: &Arc<WorkingCopy>) {
        #[cfg_attr(not(feature = "demo-host"), allow(unused_mut))]
        let mut resolve: crate::shell::pinupdate::Resolver =
            Arc::new(homelab_client::pinexists::resolve_digest);
        #[cfg(feature = "demo-host")]
        if self.demo_host {
            resolve = Arc::new(crate::shell::demo::demo_resolve);
        }
        self.app.dashboard_routes(crate::shell::pinupdate::router(
            crate::shell::pinupdate::PinCtx {
                wc: wc.clone(),
                resolve,
                host: self.host.clone(),
            },
        ));
    }

    /// TUI parity: releases (Update host, install-native, the badge), the
    /// host's log stream, and the read routes the TUI and CLI had alone.
    pub(super) fn mount_releases_and_parity(
        &mut self,
        actions: &Actions,
        files: Arc<dyn StackFiles>,
    ) -> (Arc<dyn Releases>, HostLog) {
        let releases: Arc<dyn Releases> = Arc::new(crate::shell::releases::GitHub::new());
        actions.set_releases(releases.clone());
        let hostlog = HostLog::with_ring(
            self.publish.clone(),
            self.clock.clone(),
            self.cfg.hostlog_ring,
        );
        self.app
            .dashboard_routes(crate::shell::hostlog::router(hostlog.clone()));
        self.app.dashboard_routes(crate::shell::parity::router(
            crate::shell::parity::ParityCtx::with_drift_reuse(
                self.host.clone(),
                self.shared.clone(),
                actions.clone(),
                files,
                self.cfg.repo.clone(),
                self.cfg.scratch_dir(),
                self.publish.clone(),
                self.cfg.drift_reuse_s,
            ),
        ));
        self.app.dashboard_routes(super::router(actions.clone()));
        (releases, hostlog)
    }

    /// eten-home-ip-1: the house's address against the Cloudflare bypass
    /// policies (the Home address section and the Inbox check). A demo run
    /// never calls out.
    pub(super) fn mount_home_address(
        &mut self,
        actions: &Actions,
    ) -> Result<Arc<HomeAddress>, String> {
        let home_address = if self.demo_host {
            demo_home_address(self.clock.clone())
        } else {
            HomeAddress::from_config(&self.cfg, self.clock.clone())?
        };
        actions.set_home_address(home_address.clone());
        self.app
            .dashboard_routes(crate::shell::homeaddress::router(home_address.clone()));
        Ok(home_address)
    }

    /// The Inbox, its per-row snoozes (ask-kp11be-7) and the scheduler.
    pub(super) fn mount_inbox(
        &mut self,
        scheduler: &Arc<Scheduler>,
    ) -> Arc<crate::shell::rowsnooze::RowSnoozes> {
        self.app
            .dashboard_routes(actions_notify::router(self.notify.clone()));
        let row_snoozes = crate::shell::rowsnooze::RowSnoozes::load(
            self.db.clone(),
            self.clock.clone(),
            self.publish.clone(),
        );
        self.app
            .dashboard_routes(crate::shell::rowsnooze::router(row_snoozes.clone()));
        self.app
            .dashboard_routes(crate::shell::scheduler::router(scheduler.clone()));
        row_snoozes
    }
}
