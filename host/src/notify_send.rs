//! Sending one notification over the wire (webhook), with the pinned certificate when one is configured.

use crate::*;

/// Lower-level webhook POST used by notify() and the boot notification.
///
/// G16 (F222, see REGISTER.md): reads the status, falls back to the second
/// route when the first fails, and records the outcome in state so an
/// unreachable notification path becomes a finding instead of a silence.
/// fix-126 (TLS to the message hub, owner decision 2026-10-01): the
/// `--cacert` path for `url`, or the reason it is refused. `Ok(None)` for a
/// plain `http://` route, which needs no pin (migration is stepwise).
/// Re-checked at every send, not only at boot: the certificate file can be
/// rewritten — a regenerated hub certificate — without the host restarting,
/// and a stale pin must stop the send, not weaken into trusting whatever is
/// on disk now.
pub(crate) fn pinned_cacert(cfg: &Config, url: &str) -> Result<Option<String>, String> {
    if !url.starts_with("https://") {
        return Ok(None);
    }
    let path = cfg
        .notify_tls_cert
        .as_deref()
        .ok_or("https:// route with no notify_tls_cert configured")?;
    let want = cfg
        .notify_tls_fingerprint
        .as_deref()
        .ok_or("https:// route with no notify_tls_fingerprint configured")?;
    let pem =
        std::fs::read_to_string(path).map_err(|e| format!("notify_tls_cert {}: {}", path, e))?;
    let got = homelab_core::notify::cert_fingerprint(&pem)
        .map_err(|e| format!("notify_tls_cert {}: {}", path, e))?;
    if got != want {
        return Err(format!(
            "notify_tls_cert {} does not match notify_tls_fingerprint ({} on disk, {} \
             configured) — the hub's certificate changed, or this is the wrong file",
            path, got, want
        ));
    }
    Ok(Some(path.to_string()))
}

pub(crate) async fn notify_raw(
    state: &AppState,
    exec: &RealExecutor,
    payload: String,
) -> Result<(), String> {
    let primary = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .notify_webhook
        .clone();
    let fallback = state.config.notify_fallback_webhook.clone();
    let urls = homelab_core::notify::route(primary.as_deref(), fallback.as_deref());
    if urls.is_empty() {
        return Err("no notification route is configured (notify_webhook)".into());
    }
    let mut last = String::new();
    let mut delivered = false;
    for (i, url) in urls.iter().enumerate() {
        // Each route carries its own credential: kyu takes a bearer token,
        // Home Assistant's webhook takes none.
        let bearer = if i == 0 {
            state.config.notify_auth_bearer.clone()
        } else {
            state.config.notify_fallback_auth_bearer.clone()
        };
        // fix-35: the token goes to curl through a 0600 header file, never
        // through argv (see homelab_core::notify::curl_args).
        let header_file = match bearer {
            Some(t) => {
                let path = homelab_core::notify::header_file_path(&state.config.state_dir, i);
                match exec
                    .write_file(&path, &homelab_core::notify::header_file_content(&t), 0o600)
                    .await
                {
                    Ok(()) => Some(path),
                    Err(e) => {
                        last = format!("cannot write the header file {}: {}", path, e);
                        continue;
                    }
                }
            }
            None => None,
        };
        // fix-126: the pin is re-checked here, not only at boot — the
        // certificate file can be rewritten (a regenerated hub certificate)
        // without the host restarting.
        let cacert = match pinned_cacert(&state.config, url) {
            Ok(c) => c,
            Err(e) => {
                last = e;
                tracing::warn!(
                    "notification route {} refused: {}",
                    homelab_core::notify::route_for_log(url),
                    last
                );
                continue;
            }
        };
        let owned = homelab_core::notify::curl_args_pinned(
            &payload,
            url,
            header_file.as_deref(),
            cacert.as_deref(),
        );
        let args: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
        let out = exec.run(&Cmd::new("curl", &args, 10)).await;
        let (ran, code) = match &out {
            Ok(o) => (true, o.stdout.clone()),
            Err(_) => (false, String::new()),
        };
        match homelab_core::notify::verdict(ran, &code) {
            homelab_core::notify::Delivery::Delivered => {
                if i > 0 {
                    tracing::warn!(
                        "notification took the fallback route: the primary said {}",
                        last
                    );
                }
                delivered = true;
                break;
            }
            homelab_core::notify::Delivery::Failed(why) => {
                last = why;
                // fix-123: the route's host only; a webhook path is its id.
                tracing::warn!(
                    "notification route {} failed: {}",
                    homelab_core::notify::route_for_log(url),
                    last
                );
            }
        }
    }
    record_notify_outcome(state, exec, delivered, &last).await;
    if delivered { Ok(()) } else { Err(last) }
}
