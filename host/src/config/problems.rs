//! The faults that must stop a start or refuse a change, said before anything is written.

use super::*;
use std::net::SocketAddr;

/// feat-settings-1: the start-up validation's refusals for a file, without
/// the process exit, so a settings change is held to the same checks
/// before it is written. `load_config_from` exits on the same list.
pub(crate) fn startup_problems(file: &FileConfig) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(t) = &file.token
        && t.len() < 16
    {
        out.push("token must be at least 16 characters".into());
    }
    if file.status_interval_s.is_some_and(|s| s < 10) {
        out.push(
            "status_interval_s must be at least 10 (each reading runs one probe per container)"
                .into(),
        );
    }
    if let Err(e) = validate_tokens(file.tokens.as_deref().unwrap_or_default()) {
        out.push(e);
    }
    for (name, spec) in file.tools.iter().flatten() {
        let named = homelab_core::tools::spec::ToolSpec {
            name: name.clone(),
            ..spec.clone()
        };
        if let Err(e) = named.validate() {
            out.push(e);
        }
    }
    if let Some(l) = &file.listen
        && l.parse::<SocketAddr>().is_err()
    {
        out.push(format!("listen {:?} must be host:port", l));
    }
    if file.backup_hour.is_some_and(|h| h > 23) {
        out.push("backup_hour must be 0-23".into());
    }
    if file.retention.as_ref().is_some_and(|r| r.is_empty()) {
        out.push("retention needs at least one tier".into());
    }
    if let Some(level) = &file.log_level
        && !level.trim().is_empty()
        && !log_level_is_valid(level)
    {
        out.push(format!(
            "log_level {:?} is not a tracing/EnvFilter directive (e.g. \"info\", \"debug\")",
            level
        ));
    }
    for job in file.zfs_jobs.iter().flatten() {
        if let Some(p) = homelab_core::ops::zfs::job_problems(job) {
            out.push(format!("zfs_jobs {} → {}: {}", job.source, job.target, p));
        }
    }
    // fix-126 (TLS to the message hub, owner decision 2026-10-01): an
    // `https://` notify route with no pin would fall back to curl's system
    // CA bundle, which a LAN self-signed certificate never passes — refused
    // here, at the config, rather than discovered as "every notification
    // fails" at 04:00. `http://` stays accepted unconditionally (migration
    // is stepwise); the actual fingerprint-on-disk match is a boot-time
    // warning (`load_config_from`), not refused here, since it needs to
    // read the file.
    let https_route = [
        file.notify_webhook.as_deref(),
        file.notify_fallback_webhook.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|u| u.starts_with("https://"));
    if https_route && (file.notify_tls_cert.is_none() || file.notify_tls_fingerprint.is_none()) {
        out.push(
            "an https:// notify route needs notify_tls_cert and notify_tls_fingerprint — the \
             LAN hub's certificate is self-signed, so curl trusts nothing unless it is pinned"
                .into(),
        );
    }
    out
}
