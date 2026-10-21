//! ask-kp11be-7: per-row snooze and remind-me on the Inbox. A row is
//! snoozed by its stable key until a moment; it leaves the list and the
//! counter and comes back at that moment. A snooze made with "remind me"
//! also leaves a Reminder notice when it ends. Pure: no clock, no I/O.
//!
//! Bounded up front: at most [`MAX_ROWS`] snoozes, each at most a year ahead,
//! short keys and titles; an ended snooze is finished by definition and is
//! removed (`shell::rowsnooze` when it ends, the kit's daily clean-up for
//! one that ended while the dashboard was off), so the table never grows
//! past what is waiting.

use serde::{Deserialize, Serialize};

pub const MAX_ROWS: usize = 300;
/// The furthest a snooze may reach.
pub const MAX_AHEAD_S: i64 = 366 * 86_400;
const MAX_KEY: usize = 200;
const MAX_TITLE: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The Inbox row's stable key (`notice:12`, `ask:…`, `update:…`).
    pub key: String,
    /// Unix seconds the row comes back.
    pub until: i64,
    /// What the row said, for the Snoozed list and the reminder.
    pub title: String,
    /// The dashboard page the row opens (a path), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    /// Leave a Reminder notice when the snooze ends.
    #[serde(default)]
    pub remind: bool,
    pub set_at: i64,
}

/// The pre-4.0.0 `inbox-snoozes.json`: the shape the one-time import reads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub rows: Vec<Entry>,
}

/// What a request asks for, before it is checked.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub key: String,
    /// None ends the snooze.
    pub until: Option<i64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub remind: bool,
}

/// What a checked request does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// End the snooze of this key.
    End(String),
    /// Set (or move) a snooze.
    Set(Entry),
}

/// Check a request at `now`: the key is usable, the end is in the future
/// and at most a year away, the link is a dashboard path, the title is
/// short. The "at most [`MAX_ROWS`]" bound needs the store and is checked
/// where the row is written.
pub fn plan(r: Request, now: i64) -> Result<Op, String> {
    let key = r.key.trim().to_string();
    if key.is_empty() || key.chars().count() > MAX_KEY || key.chars().any(char::is_control) {
        return Err("the row has no usable key".into());
    }
    let Some(until) = r.until else {
        return Ok(Op::End(key));
    };
    if until <= now {
        return Err("a snooze ends in the future".into());
    }
    if until > now + MAX_AHEAD_S {
        return Err("a snooze lasts a year at most".into());
    }
    let href = r
        .href
        .filter(|h| h.starts_with('/') && !h.starts_with("//") && h.len() <= 300)
        .filter(|h| !h.chars().any(char::is_control));
    let title: String = r
        .title
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TITLE)
        .collect();
    Ok(Op::Set(Entry {
        key: key.clone(),
        until,
        title: if title.trim().is_empty() { key } else { title },
        href,
        remind: r.remind,
        set_at: now,
    }))
}

/// The message for a full list.
pub fn full_message() -> String {
    format!("{MAX_ROWS} rows are snoozed already: end some before snoozing more")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(key: &str, until: Option<i64>, remind: bool) -> Request {
        Request {
            key: key.into(),
            until,
            title: "t".into(),
            href: Some("/inbox".into()),
            remind,
        }
    }

    #[test]
    fn a_request_sets_moves_or_ends_a_snooze() {
        let Op::Set(e) = plan(req(" notice:1 ", Some(200), true), 100).unwrap() else {
            panic!("a future end sets a snooze");
        };
        assert_eq!(
            (e.key.as_str(), e.until, e.remind, e.set_at),
            ("notice:1", 200, true, 100)
        );
        assert_eq!(
            plan(req("notice:1", None, false), 100).unwrap(),
            Op::End("notice:1".into())
        );
    }

    #[test]
    fn bounds_hold() {
        assert!(plan(req("k", Some(50), false), 100).is_err());
        assert!(plan(req("k", Some(100 + MAX_AHEAD_S + 1), false), 100).is_err());
        assert!(plan(req("k", Some(100 + MAX_AHEAD_S), false), 100).is_ok());
        assert!(plan(req("   ", Some(200), false), 100).is_err());
        assert!(plan(req(&"k".repeat(MAX_KEY + 1), Some(200), false), 100).is_err());
    }

    #[test]
    fn an_empty_title_falls_back_to_the_key() {
        let mut r = req("a", Some(200), false);
        r.title = " \u{7} ".into();
        let Op::Set(e) = plan(r, 100).unwrap() else {
            panic!()
        };
        assert_eq!(e.title, "a");
    }

    #[test]
    fn a_link_must_be_a_dashboard_path() {
        let mut r = req("a", Some(200), false);
        r.href = Some("https://evil.example".into());
        let Op::Set(e) = plan(r, 100).unwrap() else {
            panic!()
        };
        assert_eq!(e.href, None);
    }
}
