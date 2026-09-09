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
        Command::Ping
            | Command::Status
            | Command::Doctor
            | Command::Incidents
            | Command::GetState
            // The remedy the refusal names. Blocking it turns the guard into
            // a trap: the first live run of this check refused
            // `homelab release-update` against exactly the old host it was
            // supposed to replace, and told the operator to run the command
            // it had just refused.
            | Command::SelfUpdateHost { .. }
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

/// The largest single message the CLIENT↔HOST link carries. The host sets the
/// same ceiling; this copy exists so a payload that cannot arrive is refused
/// before it is sent, with a sentence that says what happened.
///
/// It was found the way these things are always found: the host binary grew
/// 132 KB past the old 16 MiB default between two releases, and
/// `homelab release-update` answered "Connection reset by peer". That names
/// the network. The limit had never been written down anywhere, so there was
/// nothing to read.
pub const MAX_WS_FRAME: usize = 256 * 1024 * 1024;

// Raised from 64 MiB on 2026-09-09 (F303). The old figure was "five times the
// current binary" — reasoning about the HOST binary, which was the only large
// payload the link had at the time. A stack deploy carries every native
// service's program in one message, so the ceiling has to scale with the
// STACK, not with one program: CT 109 alone ships kyu, kyu-runner and
// http-switchboard, 71 MiB of binaries and 94.7 MiB once base64-encoded.
//
// Headroom, not a solution. The honest fix is to send those programs one at a
// time instead of in one message — recorded as T85. This number only buys the
// room to get there without a wall in the middle.

/// Refuse a payload the far side cannot accept, and say why.
pub fn too_large(len: usize) -> Option<String> {
    (len > MAX_WS_FRAME).then(|| {
        format!(
            "payload is {} MiB and the link carries at most {} MiB :: this is a limit, not a \
             network fault — retrying sends the same bytes and gets the same reset. What has \
             outgrown the transport is the message itself: a host binary, or a stack deploy \
             carrying every native service's program at once. It has to be split or shrunk",
            len / 1024 / 1024,
            MAX_WS_FRAME / 1024 / 1024
        )
    })
}
