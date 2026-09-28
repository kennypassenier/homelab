//! Which host release a feature needs, decided in one place.
//!
//! An older host drops a request it cannot parse without a word, so the
//! dashboard sends a command a host may not know only to a host at least as
//! new as the release that carries it (arch-host-link: "features are gated
//! on Hello `version`"). Every such gate names [`NEXT_RELEASE`].

use crate::core::actions::Refusal;

/// The release that carries the dashboard's host-side additions of the
/// read, edit and TUI parity rounds (host.toml over the line, the host's own
/// release download for install-native): the next one, 3.63.0.
pub const NEXT_RELEASE: (u64, u64, u64) = (3, 63, 0);

/// `3.62.2`, `v3.63.0-4-gabc` → the three numbers; anything else → None.
pub fn version_triple(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+', ' ']).next()?;
    let mut p = core.split('.').map(|x| x.parse::<u64>().ok());
    Some((p.next()??, p.next()??, p.next()??))
}

/// Ok when the host that said Hello with `host_version` is at least
/// `since`; a refusal that says what to do otherwise.
pub fn at_least(
    what: &str,
    host_version: Option<&str>,
    since: (u64, u64, u64),
    fix: &str,
) -> Result<(), Refusal> {
    match host_version.and_then(version_triple) {
        Some(t) if t >= since => Ok(()),
        Some(_) => Err(Refusal::new(
            what,
            format!(
                "the host runs {} and answers this only from {}.{}.{} on",
                host_version.unwrap_or_default(),
                since.0,
                since.1,
                since.2
            ),
            fix,
        )),
        None => Err(Refusal::new(
            what,
            "the dashboard has not heard the host yet",
            "wait for the link to the host; the top bar shows it",
        )),
    }
}

/// Whether the dashboard is older than the host it talks to: it may then
/// miss what the host added (a warning on every page, never a refusal).
pub fn dashboard_older(dashboard: &str, host: Option<&str>) -> bool {
    match (version_triple(dashboard), host.and_then(version_triple)) {
        (Some(d), Some(h)) => d < h,
        _ => false,
    }
}

/// Whether `latest` (a release tag) is newer than the host's version.
pub fn update_available(latest: Option<&str>, host: Option<&str>) -> bool {
    match (latest, host) {
        (Some(l), Some(h)) => homelab_client::release::version_newer(l, h),
        _ => false,
    }
}

/// What the pages show about versions: the host's, the dashboard's, the
/// newest release, and the two warnings (a host update is available; the
/// dashboard is older than the host).
pub fn release_view(latest: Option<&str>, host: Option<&str>) -> serde_json::Value {
    let dashboard = env!("CARGO_PKG_VERSION");
    serde_json::json!({
        "latest": latest,
        "host": host,
        "dashboard": dashboard,
        "update_available": update_available(latest, host),
        "dashboard_older": dashboard_older(dashboard, host),
    })
}
