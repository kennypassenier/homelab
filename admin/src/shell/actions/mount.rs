//! milestone act, wired into the app: the action queue, the notification
//! center and the scheduler, their routes, and (on the serving path only)
//! the scheduler's tick and the incident poll. Settings come from
//! `HOMELAB_ADMIN_*` (`core::actions_config`).
//!
//! [`mount`] runs the steps in a fixed order; each step is one method of
//! [`Wiring`]. What the start hook needs afterwards is handed over as
//! [`StartParts`] (`mount_start`).

use std::sync::Arc;

use chassis::shell::live::Live;
use chassis::storage::AppDb;

use super::mount_setup::{build_pusher, load_notify, open_db};
use super::mount_start::StartParts;
use super::{Actions, ActionsDeps, Clock, HostPort, Publish, RepoFiles, StackFiles, system_clock};
use crate::core::actions_config::{self, ActConfig};
use crate::shell::actions_notify::{self, HookSlot, NotifyCenter};
use crate::shell::drive::Driver;
use crate::shell::homeaddress::HomeAddress;
use crate::shell::host_link::{HostClient, Shared};
use crate::shell::hostlog::HostLog;
use crate::shell::releases::Releases;
use crate::shell::scheduler::Scheduler;
use crate::shell::workcopy::WorkingCopy;

/// milestone act, wired into the app: the action queue, the notification
/// center and the scheduler, their routes, and (on the serving path only)
/// the scheduler's tick and the incident poll. Settings come from
/// `HOMELAB_ADMIN_*` (`core::actions_config`).
pub fn mount(
    app: &mut chassis::App,
    host: HostClient,
    live: Live,
    shared: Shared,
    demo_host: bool,
    hooks: HookSlot,
) -> Result<AppDb, String> {
    let cfg = actions_config::from_env(&|k| std::env::var(k).ok())?;
    let mut w = Wiring::open(app, cfg, (host, live, shared), demo_host, &hooks)?;
    let wc = w.working_copy();
    let files = w.stack_files(&wc);
    let watched = w.mount_watch();
    let actions = w.start_actions(&files);
    let scheduler = w.load_scheduler(&actions);
    let edit_ctx = w.mount_edit(&wc, &actions);
    let driver = w.mount_driver(&actions, &edit_ctx);
    w.mount_pin_updates(&wc);
    let (releases, hostlog) = w.mount_releases_and_parity(&actions, files);
    let home_address = w.mount_home_address(&actions)?;
    let row_snoozes = w.mount_inbox(&scheduler);
    let db = w.db.clone();
    let parts = w.into_start_parts(StartPieces {
        wc,
        scheduler,
        driver,
        hostlog,
        releases,
        row_snoozes,
        watched,
        actions,
        home_address,
    });
    app.on_start(move || parts.run());
    // The trend of the fleet (host_link) lives in the same database.
    Ok(db)
}

/// What `mount` built that only the start hook uses.
pub(super) struct StartPieces {
    pub(super) wc: Arc<WorkingCopy>,
    pub(super) scheduler: Arc<Scheduler>,
    pub(super) driver: Driver,
    pub(super) hostlog: HostLog,
    pub(super) releases: Arc<dyn Releases>,
    pub(super) row_snoozes: Arc<crate::shell::rowsnooze::RowSnoozes>,
    pub(super) watched: crate::shell::watch::Watched,
    pub(super) actions: Actions,
    pub(super) home_address: Arc<HomeAddress>,
}

/// The pieces every step shares.
pub(super) struct Wiring<'a> {
    pub(super) app: &'a mut chassis::App,
    pub(super) cfg: ActConfig,
    pub(super) demo_host: bool,
    pub(super) clock: Clock,
    pub(super) live: Live,
    pub(super) publish: Arc<dyn Publish>,
    pub(super) host: Arc<dyn HostPort>,
    pub(super) shared: Shared,
    pub(super) db: AppDb,
    pub(super) notify: Arc<NotifyCenter>,
}

impl<'a> Wiring<'a> {
    fn open(
        app: &'a mut chassis::App,
        cfg: ActConfig,
        (host, live, shared): (HostClient, Live, Shared),
        demo_host: bool,
        hooks: &HookSlot,
    ) -> Result<Self, String> {
        let clock = system_clock();
        let publish: Arc<dyn Publish> = Arc::new(live.clone());
        let host: Arc<dyn HostPort> = Arc::new(host);
        let pusher = build_pusher(&cfg);
        let db = open_db(app, &cfg)?;
        let notify = load_notify(app, &cfg, &db, pusher, &publish, &clock, hooks)?;
        Ok(Wiring {
            app,
            cfg,
            demo_host,
            clock,
            live,
            publish,
            host,
            shared,
            db,
            notify,
        })
    }

    /// milestone edit (arch-edit-txn): the working copy, cloned at start.
    fn working_copy(&self) -> Arc<WorkingCopy> {
        Arc::new(WorkingCopy::new(
            self.cfg.repo.clone(),
            self.cfg.git.clone(),
            self.cfg.scratch_dir(),
            self.clock.clone(),
        ))
    }

    fn stack_files(&self, wc: &Arc<WorkingCopy>) -> Arc<dyn StackFiles> {
        Arc::new(RepoFiles {
            repo: self.cfg.repo.clone(),
            scratch: self.cfg.scratch_dir(),
            wc: Some(wc.clone()),
        })
    }

    /// replace-kuma: the minute watch, and what it holds for the start page.
    fn mount_watch(&mut self) -> crate::shell::watch::Watched {
        let watched: crate::shell::watch::Watched = Default::default();
        // ask-kp11be-2: the watch/ping history survives a restart (its last 90
        // days are read back from the app database).
        if let Ok(mut w) = watched.try_lock() {
            crate::shell::watch::load_history(&mut w, self.db.clone(), (self.clock)().max(0));
        }
        self.app
            .dashboard_routes(crate::shell::watch::router(watched.clone()));
        watched
    }

    fn start_actions(&self, files: &Arc<dyn StackFiles>) -> Actions {
        let actions = Actions::start(ActionsDeps {
            host: self.host.clone(),
            publish: self.publish.clone(),
            files: files.clone(),
            shared: self.shared.clone(),
            notify: self.notify.clone(),
            clock: self.clock.clone(),
            timeout: self.cfg.action_timeout(),
        });
        actions.set_limits(self.cfg.keep_jobs, self.cfg.history_window_s);
        self.notify.set_limits(actions_notify::NotifyLimits {
            keep: self.cfg.notify_keep,
            snooze_max_s: self.cfg.notify_snooze_max_s,
            digest_late_s: self.cfg.notify_digest_late_s,
        });
        actions
    }

    fn load_scheduler(&self, actions: &Actions) -> Arc<Scheduler> {
        Scheduler::load(
            self.db.clone(),
            actions.clone(),
            self.notify.clone(),
            self.publish.clone(),
            self.clock.clone(),
            self.cfg.schedule_grace_s,
        )
    }

    fn into_start_parts(self, p: StartPieces) -> StartParts {
        StartParts {
            cfg: self.cfg,
            demo_host: self.demo_host,
            clock: self.clock,
            host: self.host,
            shared: self.shared,
            live: self.live,
            db: self.db,
            notify: self.notify,
            pieces: p,
        }
    }
}
