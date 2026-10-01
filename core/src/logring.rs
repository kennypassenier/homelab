//! fix-122 (AR15's JSONL ring, Kenny's go 2026-10-01, amending the
//! "journald only" amendment in `docs/ARCHITECTURE_DECISIONS.md`): a
//! size-capped ring of the host daemon's trace lines under
//! `<state_dir>/logs/host.jsonl` — a `tracing` sink of its own, readable
//! with `tail`/`jq` without a journalctl session, kept separate from the
//! operation journal (`journal.jsonl`, AR13, a record of steps, not a log)
//! and from journald itself.
//!
//! Rule 20 (nothing may balloon): the ring is cut back the same way
//! `journal.jsonl` is — oldest lines dropped once it grows past its cap,
//! kept to half of it so the cut does not happen again on the very next
//! line. Unlike `journal.jsonl`'s compaction (`incidents::compact_journal`),
//! nothing here is pinned: a log line carries no "this operation is still
//! running" meaning the way a journal entry does, so the oldest lines are
//! simply the ones to go.
//!
//! Zero I/O: this module only decides what to keep. The host does the
//! reading and writing (`RingWriter`, `compact_log_ring_file`).

/// Default cap of `logs/host.jsonl`, matching the operation journal's
/// default (`incidents::JOURNAL_MAX_BYTES`) — the two rings are unrelated,
/// but a daemon with no opinion either way should not keep one of them an
/// order of magnitude bigger than the other.
pub const LOG_RING_MAX_BYTES: usize = 4 * 1024 * 1024;

/// `content` (newline-separated JSONL lines) cut to at most half of
/// `max_bytes`, dropping the oldest lines first. `None` when it is already
/// within `max_bytes` — nothing to do, so the caller need not rewrite a file
/// that was already fine.
pub fn compact_ring(content: &str, max_bytes: usize) -> Option<String> {
    if content.len() <= max_bytes {
        return None;
    }
    let lines: Vec<&str> = content.lines().collect();
    let budget = max_bytes / 2;
    let mut used = 0usize;
    let mut start = lines.len();
    while start > 0 && used + lines[start - 1].len() < budget {
        used += lines[start - 1].len() + 1;
        start -= 1;
    }
    let mut out = String::new();
    for line in &lines[start..] {
        out.push_str(line);
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_122_within_the_cap_is_left_alone() {
        assert_eq!(compact_ring("one\ntwo\n", 1024), None);
    }

    #[test]
    fn fix_122_over_the_cap_drops_the_oldest_lines_first() {
        let lines: Vec<String> = (0..100).map(|i| format!("line-{:03}", i)).collect();
        let content = lines.join("\n") + "\n";
        let max_bytes = 200;
        let cut = compact_ring(&content, max_bytes).expect("over the cap");
        assert!(cut.len() <= max_bytes / 2, "{} bytes: {:?}", cut.len(), cut);
        // The newest lines survive; the oldest are gone.
        assert!(cut.contains("line-099"));
        assert!(!cut.contains("line-000"));
        // What is kept is a contiguous suffix, in order.
        let kept: Vec<&str> = cut.lines().collect();
        let first_kept: usize = kept[0].trim_start_matches("line-").parse().unwrap();
        for (i, l) in kept.iter().enumerate() {
            let n: usize = l.trim_start_matches("line-").parse().unwrap();
            assert_eq!(n, first_kept + i);
        }
        assert_eq!(*kept.last().unwrap(), "line-099");
    }

    #[test]
    fn fix_122_a_single_line_bigger_than_the_cap_still_terminates() {
        let huge = "x".repeat(10_000);
        let cut = compact_ring(&huge, 100).expect("over the cap");
        // Nothing fits the budget, so nothing is kept — not a panic, not an
        // infinite loop.
        assert_eq!(cut, "");
    }
}
