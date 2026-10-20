//! arch-history (homelab-admin, 2026-09-28): what the host did, kept by the
//! host, because it does the work when the dashboard is down.
//!
//! One JSON object per line in `<state_dir>/history.jsonl`: every operation
//! (asked for over the line or started by the nightly round) with its steps
//! and their times, and every nightly phase. The dashboard draws the deploy
//! duration trend, the nightly timeline and the activity timeline from it.
//! Appending one line is the transaction; a line torn by a power cut is
//! skipped on read, never fatal.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepTiming {
    pub step: String,
    pub start: u64,
    /// 0 while the step runs; an operation that ended with a step still at 0
    /// was cut off inside it.
    pub end: u64,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryEntry {
    Op {
        start: u64,
        end: u64,
        /// The operation's label, e.g. "deploy", "scheduled-backup".
        label: String,
        /// What its steps said they were about, e.g. "deploy media".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<String>,
        /// The request that asked for it; None for the host's own work.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        req: Option<u64>,
        /// milestone act: the token name whose session asked for it; None
        /// for the host's own work and for entries written before the field.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<String>,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deferred: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        steps: Vec<StepTiming>,
    },
    Phase {
        start: u64,
        end: u64,
        /// e.g. "backup".
        name: String,
        /// How many stacks (or jobs) the phase covered.
        count: usize,
    },
}

impl HistoryEntry {
    pub fn start(&self) -> u64 {
        match self {
            HistoryEntry::Op { start, .. } | HistoryEntry::Phase { start, .. } => *start,
        }
    }

    /// One line, newline included.
    pub fn to_line(&self) -> String {
        let mut s = serde_json::to_string(self).unwrap_or_default();
        s.push('\n');
        s
    }
}

/// Every entry that parses, oldest first as written. A torn or foreign line
/// is skipped: history is for looking back, and one bad line must not hide
/// the rest.
pub fn parse(text: &str) -> Vec<HistoryEntry> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<HistoryEntry>(l).ok())
        .collect()
}

/// The entries that started at or after `since`, newest `limit` of them.
pub fn select(entries: Vec<HistoryEntry>, since: u64, limit: usize) -> Vec<HistoryEntry> {
    select_between(entries, since, None, limit)
}

/// ask-kp11be-8: [`select`] with an upper bound — the entries that started
/// at or after `since` and, when `until` is given, at or before it; the
/// newest `limit` of them. A calendar window in the past reads only its
/// own days instead of everything up to now.
pub fn select_between(
    entries: Vec<HistoryEntry>,
    since: u64,
    until: Option<u64>,
    limit: usize,
) -> Vec<HistoryEntry> {
    let mut kept: Vec<HistoryEntry> = entries
        .into_iter()
        .filter(|e| e.start() >= since && until.is_none_or(|u| e.start() <= u))
        .collect();
    if kept.len() > limit {
        kept.drain(..kept.len() - limit);
    }
    kept
}

/// What the file should hold after pruning: nothing older than `max_age_s`,
/// and when still over `max_bytes`, the oldest dropped until it fits. None
/// when nothing needs to change, so the caller rewrites the file only then.
pub fn prune(text: &str, now: u64, max_age_s: u64, max_bytes: usize) -> Option<String> {
    let cutoff = now.saturating_sub(max_age_s);
    let entries = parse(text);
    let mut lines: Vec<String> = entries
        .iter()
        .filter(|e| e.start() >= cutoff)
        .map(|e| e.to_line())
        .collect();
    let mut total: usize = lines.iter().map(|l| l.len()).sum();
    let mut first = 0;
    while total > max_bytes && first < lines.len() {
        total -= lines[first].len();
        first += 1;
    }
    lines.drain(..first);
    let out: String = lines.concat();
    (out != text).then_some(out)
}

#[cfg(test)]
mod bounded_read_tests {
    use super::*;

    fn phase(start: u64) -> HistoryEntry {
        HistoryEntry::Phase {
            start,
            end: start + 1,
            name: "backup".into(),
            count: 1,
        }
    }

    /// ask-kp11be-8: a window reads its own days, newest `limit` of them.
    #[test]
    fn select_between_keeps_only_the_window() {
        let all = vec![phase(100), phase(200), phase(300), phase(400)];
        let got = select_between(all.clone(), 150, Some(300), 10);
        assert_eq!(
            got.iter().map(|e| e.start()).collect::<Vec<_>>(),
            [200, 300]
        );
        assert_eq!(select_between(all.clone(), 150, None, 10).len(), 3);
        assert_eq!(select_between(all, 150, Some(300), 1)[0].start(), 300);
    }
}
