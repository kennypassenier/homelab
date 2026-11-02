//! The host's single push to kyu: duplicate suppression by kyu itself, the
//! curl arguments that ask for it, and the notice written when kyu does not
//! accept a push.
//!
//! Decision kyu-notify (Kenny, "Eenvoudig", 2026-10-30): the host sends one
//! POST and nothing more. Retries and the fallback to Home Assistant belong
//! to kyu (the subscription's `max_attempts` / `backoff_ms` in the
//! kyu-runner config); the 20 h damper that used to live in the host is
//! now `Kyu-Dedup-Key` + `Kyu-Dedup-Window` on the publish. Pure functions
//! only: the transport is `host/src/notify_send.rs`.

use sha2::{Digest, Sha256};

use crate::notify::{HostNotice, curl_args_pinned};

/// The window kyu suppresses a repeated failure for (`Kyu-Dedup-Window`).
/// The same 20 h the host's own damper used, so a failing nightly job
/// pushes once per night's repeat, not every night.
pub const DEDUP_WINDOW: &str = "20h";

/// kyu accepts 1 to 200 printable ASCII bytes as a key.
pub const DEDUP_KEY_MAX: usize = 200;

/// How much of a long key stays readable before the hash suffix.
const KEY_READABLE: usize = 170;

/// The dedup key of one push: `op|error text`, as the old damper keyed it.
/// None for a success (successes are never suppressed, so they carry no
/// key). A changed error text is a different key, so it passes through.
pub fn dedup_key(op: &str, ok: bool, error: Option<&str>) -> Option<String> {
    if ok {
        return None;
    }
    let raw = format!("{}|{}", op, error.unwrap_or(""));
    let printable: String = raw
        .chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
        .collect();
    let key = printable.trim().to_string();
    if key.len() <= DEDUP_KEY_MAX {
        return Some(key).filter(|k| !k.is_empty());
    }
    // Too long for the header: the readable head plus a hash of the whole,
    // so two long texts that differ only at the end stay distinct.
    let digest = Sha256::digest(raw.as_bytes());
    let hex: String = digest
        .iter()
        .take(8)
        .map(|b| format!("{:02x}", b))
        .collect();
    Some(format!("{}#{}", &key[..KEY_READABLE], hex))
}

/// [`curl_args_pinned`] plus the dedup headers, when there is a key.
pub fn curl_args_kyu(
    payload: &str,
    url: &str,
    header_file: Option<&str>,
    cacert_path: Option<&str>,
    dedup: Option<&str>,
) -> Vec<String> {
    let mut args = curl_args_pinned(payload, url, header_file, cacert_path);
    let Some(key) = dedup else {
        return args;
    };
    // `curl_args_pinned` ends with `-d <payload> <url>`; the dedup headers go
    // right before that tail. Found by the `-d` marker, not by a count.
    let at = args.iter().rposition(|a| a == "-d").unwrap_or(args.len());
    let mut out: Vec<String> = args.drain(..at).collect();
    out.extend([
        "-H".to_string(),
        format!("Kyu-Dedup-Key: {}", key),
        "-H".to_string(),
        format!("Kyu-Dedup-Window: {}", DEDUP_WINDOW),
    ]);
    out.extend(args);
    out
}

/// Whether curl's `-w` output says kyu answered `Kyu-Duplicate: true`
/// (a 200 instead of a 201: kyu already holds this alert).
pub fn was_duplicate(curl_out: &str) -> bool {
    curl_out
        .split_whitespace()
        .nth(1)
        .is_some_and(|w| w.eq_ignore_ascii_case("true"))
}

/// The push column text for a delivered push.
pub fn sent_text(curl_out: &str) -> &'static str {
    if was_duplicate(curl_out) {
        "sent: kyu already holds the same failure (within 20 h), not queued again"
    } else {
        "sent"
    }
}

/// The label of the undelivered notice for one destination (`route` is
/// [`crate::notify::route_for_log`]): the key the host collapses on.
pub fn undelivered_label(route: &str) -> String {
    format!("notification:{}", route)
}

const UNDELIVERED_TITLE: &str = "Notification could not be delivered";

/// The notice the host writes when kyu did not accept a push: the failure
/// is not queued and not retried here, so the Inbox is where it is seen.
/// Not urgent: pushing "the push failed" through the route that just
/// failed would only fail again. One per destination: see
/// [`fold_undelivered`].
pub fn undelivered_notice(
    seq: u64,
    at: u64,
    failed: &HostNotice,
    why: &str,
    route: &str,
) -> HostNotice {
    HostNotice {
        seq,
        at,
        since: at,
        op: "notify".into(),
        label: undelivered_label(route),
        ok: false,
        deferred: false,
        stack: None,
        title: UNDELIVERED_TITLE.into(),
        what: undelivered_what(1, &failed.title, why),
        consequence: "Nobody was pushed about it. The host keeps no queue and does not retry; \
                      this notice and the ones it counts are all that is left."
            .into(),
        remedy: "Check that kyu answers (its dashboard, CT 109) and that notify_webhook and \
                 notify_auth_bearer in host.toml are right."
            .into(),
        page: failed.page.clone(),
        urgent: false,
        routed: "not pushed: it reports a push that failed".into(),
        push: "centre only".into(),
        incident: None,
        req: None,
        by: None,
        findings: Vec::new(),
    }
}

fn undelivered_what(count: u64, latest_title: &str, why: &str) -> String {
    let noun = if count == 1 { "push" } else { "pushes" };
    format!(
        "kyu did not accept {} {} (latest: \"{}\"): {}",
        count, noun, latest_title, why
    )
}

/// Whether `n` is the open undelivered notice of destination `route`.
pub fn is_undelivered_for(n: &HostNotice, route: &str) -> bool {
    n.op == "notify" && n.title == UNDELIVERED_TITLE && n.label == undelivered_label(route)
}

/// How many pushes an undelivered notice counts (1 when unreadable).
pub fn undelivered_count(n: &HostNotice) -> u64 {
    n.what
        .strip_prefix("kyu did not accept ")
        .and_then(|r| r.split_whitespace().next())
        .and_then(|c| c.parse().ok())
        .unwrap_or(1)
}

/// The previous open undelivered notice and the new one as ONE notice: the
/// new one's seq and time (so the dashboard cursor sees it as news again),
/// the previous `since`, and the count raised by one.
pub fn fold_undelivered(prev: &HostNotice, new: HostNotice, why: &str, latest: &str) -> HostNotice {
    HostNotice {
        since: prev.since,
        what: undelivered_what(undelivered_count(prev) + 1, latest, why),
        ..new
    }
}
