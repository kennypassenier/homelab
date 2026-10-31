//! Small values of the settings file: who a token names, the watched-backup entry and the defaults and sanity rules.

/// One externally-made backup to keep an eye on.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct WatchedBackup {
    /// What the nightly report calls it.
    pub(crate) name: String,
    /// An rclone path, e.g. `gdrive:homelab-backups/OPNSense-backups`.
    pub(crate) rclone_path: String,
    /// Older than this and it becomes a finding. OPNsense runs its backup
    /// cron at 01:00 with up to an hour of jitter, so 26 leaves room for a
    /// late night without crying wolf.
    #[serde(default = "default_watched_max_age_hours")]
    pub(crate) max_age_hours: u64,
}

pub(crate) fn default_watched_max_age_hours() -> u64 {
    26
}

/// Who a session belongs to, decided once when it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) name: String,
    pub(crate) scope: homelab_proto::Scope,
}

/// fix-122: `tracing_subscriber::EnvFilter::try_new` almost never returns
/// `Err` — its directive grammar is deliberately forgiving, so garbage like
/// "not a directive!!" parses as a bare directive and is accepted rather
/// than refused (measured 2026-10-01: `try_new` on exactly that string is
/// `Ok`). A log level a person can type wrong needs a check that actually
/// rejects the wrong ones, so this re-checks the grammar `EnvFilter` itself
/// documents: comma-separated directives, each either a bare level keyword
/// or `target[span]=level`.
pub(crate) fn log_level_is_valid(s: &str) -> bool {
    fn is_level(s: &str) -> bool {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "trace" | "debug" | "info" | "warn" | "error" | "off"
        ) || matches!(s, "0" | "1" | "2" | "3" | "4" | "5")
    }
    fn is_target(s: &str) -> bool {
        // A span suffix, `target[span]`, narrows further; only the target
        // itself (a crate/module path) needs validating here.
        let target = s.split('[').next().unwrap_or(s);
        !target.is_empty()
            && target
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && target
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '-'))
    }
    let s = s.trim();
    !s.is_empty()
        && s.split(',').map(str::trim).all(|d| {
            if d.is_empty() {
                return false;
            }
            match d.split_once('=') {
                Some((target, level)) => is_target(target) && is_level(level),
                None => is_level(d),
            }
        })
}

/// Y1: how many backups actually run at once, given what the config says.
///
/// Zero is the case worth guarding. `backup_concurrency = 0` in host.toml
/// reads like "no limit" to the person typing it and means "run none" to the
/// stream that consumes it — so a typo meant to go faster would silently
/// produce a night with no backups at all, and the only sign would be
/// `last_backup` never moving. Clamped to at least one: slow is recoverable,
/// silent is not.
pub(crate) fn effective_concurrency(configured: usize) -> usize {
    configured.max(1)
}

/// fix-240: ten minutes (`homelab_core::ask::DEFAULT_WAIT_S`). Two minutes
/// ran out before the person the question was for had seen it.
pub(crate) fn default_ask_timeout_s() -> u64 {
    homelab_core::ask::DEFAULT_WAIT_S
}

/// Kenny's choice (form Y4, 2026-09-02): three at a time. About a third of
/// the wait, and never more than three services briefly quiet at 04:00.
pub(crate) fn default_backup_concurrency() -> usize {
    3
}

/// A day. The coverage question is asked nightly and on demand, so a stack
/// that has shipped nothing in twenty-four hours has genuinely stopped —
/// while an hour of quiet is normal for a service nobody browsed.
pub(crate) fn default_logs_window() -> String {
    "24h".into()
}

/// Only `<digits><unit>` reaches the query string. The window is pasted into
/// a hand-built URL, and a value out of a config file is not a value to trust
/// with that: anything else falls back to the default rather than producing a
/// query that silently means something other than it says.
pub(crate) fn sane_window(w: &str) -> String {
    let ok = w.len() >= 2
        && w.chars().next().is_some_and(|c| c.is_ascii_digit())
        && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit())
        && matches!(w.chars().last(), Some('s' | 'm' | 'h' | 'd' | 'w'));
    if ok {
        w.to_string()
    } else {
        default_logs_window()
    }
}
