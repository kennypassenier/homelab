//! Sending one notification over the wire (webhook), with the pinned certificate when one is configured.

use crate::*;

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

/// Writes the bearer to the 0600 header file curl reads (fix-35).
async fn bearer_header_file(
    state: &AppState,
    exec: &RealExecutor,
) -> Result<Option<String>, String> {
    let Some(token) = state.config.notify_auth_bearer.clone() else {
        return Ok(None);
    };
    let path = homelab_core::notify::header_file_path(&state.config.state_dir, 0);
    exec.write_file(
        &path,
        &homelab_core::notify::header_file_content(&token),
        0o600,
    )
    .await
    .map_err(|e| format!("cannot write the header file {}: {}", path, e))?;
    Ok(Some(path))
}

/// One POST to kyu (`notify_webhook`), nothing more.
///
/// Decision kyu-notify (Kenny, "Eenvoudig"): no retry loop, no queue and no
/// second route here. Retries and the fallback to Home Assistant belong to
/// kyu; `dedup` (see `homelab_core::notify_kyu::dedup_key`) lets kyu drop a
/// repeated failure. Returns what the push column should say, or why kyu
/// did not accept the push. The outcome is recorded in state (G16, F222) so
/// an unreachable notification path becomes a finding instead of a silence.
pub(crate) async fn notify_raw(
    state: &AppState,
    exec: &RealExecutor,
    payload: String,
    dedup: Option<String>,
) -> Result<String, String> {
    let url = state
        .settings
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .notify_webhook
        .clone()
        .ok_or_else(|| "no notification route is configured (notify_webhook)".to_string());
    let result = match url {
        Ok(url) => post_to_kyu(state, exec, &url, &payload, dedup.as_deref()).await,
        Err(e) => Err(e),
    };
    match &result {
        Ok(_) => record_notify_outcome(state, exec, true, "").await,
        Err(why) => record_notify_outcome(state, exec, false, why).await,
    }
    result
}

async fn post_to_kyu(
    state: &AppState,
    exec: &RealExecutor,
    url: &str,
    payload: &str,
    dedup: Option<&str>,
) -> Result<String, String> {
    let header_file = bearer_header_file(state, exec).await?;
    // fix-126: the pin is re-checked here, not only at boot — the
    // certificate file can be rewritten without the host restarting.
    let cacert = pinned_cacert(&state.config, url).inspect_err(|e| {
        tracing::warn!(
            "notification route {} refused: {}",
            homelab_core::notify::route_for_log(url),
            e
        )
    })?;
    let owned = homelab_core::notify_kyu::curl_args_kyu(
        payload,
        url,
        header_file.as_deref(),
        cacert.as_deref(),
        dedup,
    );
    let args: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    let out = exec.run(&Cmd::new("curl", &args, 10)).await;
    let (ran, code) = match &out {
        Ok(o) => (true, o.stdout.clone()),
        Err(_) => (false, String::new()),
    };
    match homelab_core::notify::verdict(ran, &code) {
        homelab_core::notify::Delivery::Delivered => {
            Ok(homelab_core::notify_kyu::sent_text(&code).to_string())
        }
        homelab_core::notify::Delivery::Failed(why) => {
            // fix-123: the route's host only; a webhook path is its id.
            tracing::warn!(
                "notification route {} failed: {}",
                homelab_core::notify::route_for_log(url),
                why
            );
            Err(why)
        }
    }
}
