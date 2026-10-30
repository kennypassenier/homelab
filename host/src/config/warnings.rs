//! What is said out loud when the config loads, without stopping the daemon.

use super::*;
use crate::{orphan_watchers, plaintext_bearer_routes};

/// Faults that are real but do not stop the start: a watcher that matches no
/// writer, a bearer over plain HTTP, a notification certificate that does not
/// match its pinned fingerprint.
pub(crate) fn warn_at_load(cfg: &Config) {
    // F259: a watcher whose path no writer targets watches nothing — it
    // reports "holds no files at all" forever while looking like a working
    // check. Said out loud at load, where every other config fault is said.
    for w in orphan_watchers(
        &cfg.watched_backups,
        &cfg.device_backups,
        &cfg.backup.restic_base,
    ) {
        tracing::warn!(
            "watched_backups entry {} matches no device_backups writer — it will report \
             'holds no files at all' whatever happens",
            w
        );
    }
    // fix-126: said at every start until the route moves to TLS.
    for route in plaintext_bearer_routes(
        cfg.initial_settings.notify_webhook.as_deref(),
        cfg.notify_auth_bearer.as_deref(),
        cfg.notify_fallback_webhook.as_deref(),
        cfg.notify_fallback_auth_bearer.as_deref(),
    ) {
        tracing::warn!(
            "notification route {} sends its bearer token over plain HTTP — anything on that \
             network segment can read it; an https:// route closes this",
            route
        );
    }
    // fix-126: an https:// route is already refused at start without a pin
    // (`startup_problems`); this is the one check that needs the file on
    // disk, so it runs here instead — a mismatch means the hub's
    // certificate changed (or notify_tls_cert points at the wrong file) and
    // is as loud as a client refusing a changed host certificate.
    if let (Some(path), Some(want)) = (&cfg.notify_tls_cert, &cfg.notify_tls_fingerprint) {
        match std::fs::read_to_string(path).map(|pem| homelab_core::notify::cert_fingerprint(&pem))
        {
            Ok(Ok(got)) if &got != want => {
                tracing::error!(
                    "notify_tls_cert {} does not match notify_tls_fingerprint ({}; the file is \
                     {}) — notifications over https will be refused until this is fixed: either \
                     the hub's certificate changed (read its fingerprint again and update \
                     notify_tls_fingerprint) or notify_tls_cert is the wrong file",
                    path,
                    want,
                    got
                );
            }
            Ok(Err(e)) => {
                tracing::error!("notify_tls_cert {} is not a valid certificate: {}", path, e);
            }
            Err(e) => {
                tracing::error!("notify_tls_cert {} could not be read: {}", path, e);
            }
            Ok(Ok(_)) => {}
        }
    }
}
