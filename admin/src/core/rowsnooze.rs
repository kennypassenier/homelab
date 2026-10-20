//! ask-kp11be-7: per-row snooze and remind-me on the Inbox. A row is
//! snoozed by its stable key until a moment; it leaves the list and the
//! counter and comes back at that moment. A snooze made with "remind me"
//! also leaves a Reminder notice when it ends. Pure: no clock, no I/O.
//!
//! Bounded up front: at most [`MAX_ROWS`] snoozes, each at most a year ahead,
//! short keys and titles; an ended snooze is removed, so the file never
//! grows past what is waiting.

use serde::{Deserialize, Serialize};

pub const SCHEMA: u32 = 1;
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

impl File {
    /// Apply a request: set (or move) a snooze, or end it.
    pub fn apply(&mut self, r: Request, now: i64) -> Result<(), String> {
        let key = r.key.trim().to_string();
        if key.is_empty() || key.chars().count() > MAX_KEY || key.chars().any(char::is_control) {
            return Err("the row has no usable key".into());
        }
        let Some(until) = r.until else {
            self.rows.retain(|e| e.key != key);
            return Ok(());
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
        let entry = Entry {
            key: key.clone(),
            until,
            title: if title.trim().is_empty() {
                key.clone()
            } else {
                title
            },
            href,
            remind: r.remind,
            set_at: now,
        };
        if let Some(e) = self.rows.iter_mut().find(|e| e.key == key) {
            *e = entry;
        } else if self.rows.len() >= MAX_ROWS {
            return Err(format!(
                "{MAX_ROWS} rows are snoozed already: end some before snoozing more"
            ));
        } else {
            self.rows.push(entry);
        }
        Ok(())
    }

    /// Take out every snooze that has ended; the ones that asked to be
    /// reminded are handed back.
    pub fn take_ended(&mut self, now: i64) -> Vec<Entry> {
        let (ended, left): (Vec<_>, Vec<_>) = std::mem::take(&mut self.rows)
            .into_iter()
            .partition(|e| e.until <= now);
        self.rows = left;
        ended.into_iter().filter(|e| e.remind).collect()
    }

    /// The snoozes still running, soonest first.
    pub fn active(&self, now: i64) -> Vec<&Entry> {
        let mut v: Vec<&Entry> = self.rows.iter().filter(|e| e.until > now).collect();
        v.sort_by_key(|e| (e.until, e.key.clone()));
        v
    }
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
    fn a_snooze_is_set_moved_and_ended() {
        let mut f = File::default();
        f.apply(req("notice:1", Some(200), false), 100).unwrap();
        f.apply(req("notice:1", Some(300), false), 100).unwrap();
        assert_eq!(f.rows.len(), 1);
        assert_eq!(f.active(100)[0].until, 300);
        f.apply(req("notice:1", None, false), 100).unwrap();
        assert!(f.rows.is_empty());
    }

    #[test]
    fn bounds_hold() {
        let mut f = File::default();
        assert!(f.apply(req("k", Some(50), false), 100).is_err());
        assert!(
            f.apply(req("k", Some(100 + MAX_AHEAD_S + 1), false), 100)
                .is_err()
        );
        for i in 0..MAX_ROWS {
            f.apply(req(&format!("k{i}"), Some(200), false), 100)
                .unwrap();
        }
        assert!(f.apply(req("one-more", Some(200), false), 100).is_err());
        // Moving one that is there is not "one more".
        assert!(f.apply(req("k1", Some(400), false), 100).is_ok());
    }

    #[test]
    fn an_ended_snooze_goes_and_only_a_reminder_is_handed_back() {
        let mut f = File::default();
        f.apply(req("a", Some(150), true), 100).unwrap();
        f.apply(req("b", Some(150), false), 100).unwrap();
        f.apply(req("c", Some(900), true), 100).unwrap();
        let due = f.take_ended(200);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].key, "a");
        assert_eq!(f.rows.len(), 1);
    }

    #[test]
    fn a_link_must_be_a_dashboard_path() {
        let mut f = File::default();
        let mut r = req("a", Some(200), false);
        r.href = Some("https://evil.example".into());
        f.apply(r, 100).unwrap();
        assert_eq!(f.rows[0].href, None);
    }
}
