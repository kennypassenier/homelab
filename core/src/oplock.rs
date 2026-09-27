//! fix-104 (long-silences, 2026-09-27): who holds the host's operation lock.
//!
//! Mutations run strictly one at a time (AR12). A command typed while another
//! operation held the lock waited with no word: during the nightly round
//! that is the whole backup batch, so a command typed at 02:30 hung for an
//! unknown time. The host records what holds the lock, and a command that
//! has to wait is told at once.

/// What holds the operation lock right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    /// "the nightly backup", or the operation's label ("deploy").
    pub what: String,
    pub started_unix: u64,
    /// For a batch: how many of `total` are finished. Both 0 for a single
    /// operation.
    pub done: usize,
    pub total: usize,
}

/// A duration as minutes and seconds, hours past an hour.
fn ago(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{} s", s),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} h {} min", s / 3600, (s % 3600) / 60),
    }
}

/// The line a waiting command is sent the moment it finds the lock taken.
pub fn waiting_message(holder: Option<&Holder>, now_unix: u64) -> String {
    const TAIL: &str = "this command runs as soon as it is done";
    let Some(h) = holder else {
        return format!("waiting for another operation to finish; {}", TAIL);
    };
    let since = ago(now_unix.saturating_sub(h.started_unix));
    if h.total > 0 {
        format!(
            "waiting for {} (started {} ago, stack {} of {}); {}",
            h.what,
            since,
            (h.done + 1).min(h.total),
            h.total,
            TAIL
        )
    } else {
        format!("waiting for {} (started {} ago); {}", h.what, since, TAIL)
    }
}
