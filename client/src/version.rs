//! Is the host we are talking to older than this client?
//!
//! A client newer than the host loses whatever the host does not know about.
//! Serde drops an unknown field without a word, so a deploy succeeds and
//! quietly does less than it was asked to. On 2026-08-31 a host one release
//! behind ignored the `data_mounts` block: the downloader came up without its
//! disks, 73 torrents went to `missingFiles`, and roughly 7 GB of partial
//! downloads had to be fetched again. Nothing in the transcript said a word.

use homelab_proto::Command;

/// Does this command change anything on the host? Read-only commands stay
/// usable against an older host precisely so a mismatch can be diagnosed;
/// everything else, including anything added later, counts as mutating.
pub fn mutates(c: &Command) -> bool {
    !matches!(
        c,
        Command::Ping | Command::Status | Command::Doctor | Command::Incidents | Command::GetState
    )
}

/// Is `host` an older release than `client`? Compares major/minor/patch;
/// anything unparseable is treated as NOT older, because refusing to work on
/// a version string we failed to read would be worse than the problem this
/// guards against.
pub fn older(host: &str, client: &str) -> bool {
    fn parts(v: &str) -> Option<(u32, u32, u32)> {
        let mut it = v.trim().trim_start_matches('v').split('.');
        let a = it.next()?.parse().ok()?;
        let b = it.next()?.parse().ok()?;
        let c = it.next()?.split(['-', '+']).next()?.parse().ok()?;
        Some((a, b, c))
    }
    match (parts(host), parts(client)) {
        (Some(h), Some(c)) => h < c,
        _ => false,
    }
}
