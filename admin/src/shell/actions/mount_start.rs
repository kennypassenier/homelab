//! What runs when the app starts serving: the working copy's first sync,
//! and every background task of the dashboard.

use std::sync::Arc;
use std::time::Duration;

use chassis::shell::live::Live;
use chassis::storage::AppDb;

use super::mount::StartPieces;
use super::{Clock, HostPort};
use crate::core::actions_config::{ActConfig, GitConfig};
use crate::shell::actions_notify::{self, NotifyCenter};
use crate::shell::host_link::Shared;
use crate::shell::workcopy::WorkingCopy;

/// Everything the start hook owns; `mount` hands it over in one piece.
pub(super) struct StartParts {
    pub(super) cfg: ActConfig,
    pub(super) demo_host: bool,
    pub(super) clock: Clock,
    pub(super) host: Arc<dyn HostPort>,
    pub(super) shared: Shared,
    pub(super) live: Live,
    pub(super) db: AppDb,
    pub(super) notify: Arc<NotifyCenter>,
    pub(super) pieces: StartPieces,
}

impl StartParts {
    pub(super) fn run(self) {
        refuse_failed_import(&self.db);
        provision_working_copy(self.cfg.git.clone(), self.pieces.wc.clone());
        self.spawn_workers();
        self.spawn_notice_tasks();
        self.spawn_watchers();
    }

    fn spawn_workers(&self) {
        let p = &self.pieces;
        p.scheduler.spawn(self.cfg.tick());
        p.driver.spawn_relay(self.host.clone());
        p.driver.spawn_release();
        p.hostlog.spawn(self.host.clone());
        // The demo host answers without the internet; the badge would not.
        if !self.demo_host {
            crate::shell::releases::spawn_watch(
                p.releases.clone(),
                self.shared.clone(),
                self.live.clone(),
                Duration::from_secs(self.cfg.releases_watch_every_s),
            );
        }
    }

    fn spawn_notice_tasks(&self) {
        let actions = self.pieces.actions.clone();
        let job_of: Arc<dyn Fn(u64) -> Option<u64> + Send + Sync> =
            Arc::new(move |req| actions.job_for_req(req));
        actions_notify::spawn_host_notice_poll(
            self.host.clone(),
            self.notify.clone(),
            job_of,
            Duration::from_secs(self.cfg.host_notices_poll_s),
        );
        actions_notify::spawn_digest(
            self.notify.clone(),
            today_reader(self.host.clone(), self.cfg.repo.clone()),
            Duration::from_secs(60),
        );
        crate::shell::rowsnooze::spawn(self.pieces.row_snoozes.clone(), self.notify.clone());
    }

    fn spawn_watchers(&self) {
        crate::shell::watch::spawn(
            self.host.clone(),
            self.shared.clone(),
            self.notify.clone(),
            self.pieces.watched.clone(),
            self.pieces.actions.clone(),
            self.clock.clone(),
        );
        // drive-reach review H1: the demo host's unread notice with a fix.
        #[cfg(feature = "demo-host")]
        if self.demo_host {
            let center = self.notify.clone();
            tokio::spawn(async move { crate::shell::demo::seed_unread_notice(&center).await });
        }
        actions_notify::spawn_incident_poll(
            self.host.clone(),
            self.notify.clone(),
            Duration::from_secs(self.cfg.incidents_poll_s),
        );
        if !self.demo_host {
            crate::shell::homeaddress::spawn_watch(
                self.pieces.home_address.clone(),
                Duration::from_secs(self.cfg.home_address_check_every_s),
            );
        }
    }
}

/// A failed import means the notices and snoozes were NOT moved into the
/// database; running on would start a second, empty history that the retried
/// import then collides with. The old file is untouched (the transaction
/// rolled back), so stopping loses nothing.
fn refuse_failed_import(db: &AppDb) {
    if let chassis::storage::ImportState::Failed { why } = db.import_state() {
        tracing::error!(why = %why, "the one-time import of the dashboard's files failed; stopping, nothing was changed");
        eprintln!("homelab-admin: the one-time import of the dashboard's files failed: {why}");
        std::process::exit(1);
    }
}

/// Clone or bring the working copy up to date, off the async threads.
fn provision_working_copy(git: GitConfig, wc: Arc<WorkingCopy>) {
    tokio::task::spawn_blocking(move || {
        // CT 120: the deploy key arrives from latch as an environment
        // variable; ssh wants a file (and GitHub's host keys).
        let key = std::env::var(crate::core::credentials::DEPLOY_KEY_ENV).ok();
        let p = crate::shell::workcopy::provision_credentials(&git, key.as_deref());
        if p.key_written {
            tracing::info!(path = %git.key.display(), "the deploy key file was written from the environment (mode 0600)");
        }
        if p.known_hosts_written {
            tracing::info!(path = %git.known_hosts.display(), "GitHub's pinned host keys were written");
        }
        for why in &p.problems {
            tracing::warn!(why = %why, "the working copy's credentials");
        }
        if let Err(r) = wc.sync() {
            tracing::warn!(why = %r.why, fix = %r.fix, "the working copy is not ready");
        }
    });
}

fn today_reader(host: Arc<dyn HostPort>, repo: std::path::PathBuf) -> actions_notify::TodayRead {
    Arc::new(move || {
        let host = host.clone();
        let repo = repo.clone();
        Box::pin(async move {
            crate::shell::parity::fetch_today(&host, &repo)
                .await
                .map(|t| crate::core::notify::today_lines(&t))
        })
    })
}
