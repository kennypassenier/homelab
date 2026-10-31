//! The first steps of `mount`: the notification pusher, the app database,
//! the notification center and the demo host's stand-in address.

use std::sync::Arc;

use chassis::storage::AppDb;

use super::{Clock, Publish};
use crate::core::actions_config::ActConfig;
use crate::shell::actions_notify::{self, HookSlot, NotifyCenter};
use crate::shell::homeaddress::HomeAddress;

pub(super) fn demo_home_address(clock: Clock) -> Arc<HomeAddress> {
    HomeAddress::new(
        Arc::new(crate::shell::homeaddress::StaticAddress(
            std::net::IpAddr::from([203, 0, 113, 7]),
        )),
        None,
        crate::core::homeaddress::config_problem(None, None),
        "the demo host".into(),
        clock,
    )
}

pub(super) fn build_pusher(cfg: &ActConfig) -> Arc<dyn actions_notify::Pusher> {
    match &cfg.notify_url {
        // fix-126: the pin is read fresh here (not cached across restarts
        // of the dashboard's own process; the certificate file can be
        // rewritten by a regenerated hub certificate) and handed to the
        // client, which refuses to trust anything else over https.
        Some(url) => {
            let pem = cfg
                .notify_tls_cert
                .as_deref()
                .and_then(|p| std::fs::read_to_string(p).ok());
            Arc::new(actions_notify::KyuPusher::new_pinned(
                url.clone(),
                cfg.notify_token.clone(),
                pem.as_deref(),
                cfg.notify_tls_fingerprint.as_deref(),
            ))
        }
        None => Arc::new(actions_notify::NoPusher),
    }
}

/// feat-kpadopt-2 (4.0.0): the dashboard's data is one sqlite file through
/// chassis' storage layer. A notifications.json this build cannot read stops
/// the start, as it always did; the file's records move over once, in one
/// transaction (`store::import`).
pub(super) fn open_db(app: &mut chassis::App, cfg: &ActConfig) -> Result<AppDb, String> {
    crate::shell::store::precheck(&cfg.data_dir)?;
    let db = app
        .app_db(crate::shell::store::spec(cfg.data_dir.clone()))
        .map_err(|e| e.to_string())?;
    for kind in crate::shell::store::kinds() {
        app.storage_kind(kind);
    }
    // chassis-rs 3.12.0: the browser's choices follow the login, not the
    // device (declared after `app_db`, the table lives in that file).
    app.sync_browser_keys(crate::shell::store::browser_keys());
    Ok(db)
}

pub(super) fn load_notify(
    app: &mut chassis::App,
    cfg: &ActConfig,
    db: &AppDb,
    pusher: Arc<dyn actions_notify::Pusher>,
    publish: &Arc<dyn Publish>,
    clock: &Clock,
    hooks: &HookSlot,
) -> Result<Arc<NotifyCenter>, String> {
    let notify = NotifyCenter::load(db.clone(), pusher, publish.clone(), clock.clone())
        .map_err(|e| e.to_string())?;
    raise_storage_warnings(app, &notify);
    // Decision notify-routing / notify-detail: pushes link to this address,
    // and Alertmanager's hook now has somewhere to go.
    notify.set_base_url(&cfg.public_url);
    hooks.fill(notify.clone(), cfg.alerts_token.clone());
    Ok(notify)
}

/// The budget only warns; the warning is an Inbox notice that stays until
/// it is read (a restart refreshes it instead of adding a second).
pub(super) fn raise_storage_warnings(app: &mut chassis::App, notify: &Arc<NotifyCenter>) {
    let center = notify.clone();
    app.on_storage_warn(move |report| {
        let draft = crate::shell::store::warn_draft(report);
        let center = center.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(rt) => {
                rt.spawn(async move {
                    center.notify(draft).await;
                });
            }
            Err(_) => {
                tracing::warn!("the storage warning found no runtime to raise its notice")
            }
        }
    });
}
