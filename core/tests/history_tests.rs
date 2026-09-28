//! arch-history: the host's history file.

use homelab_core::history::{parse, prune, select, HistoryEntry, StepTiming};

fn op(start: u64) -> HistoryEntry {
    HistoryEntry::Op {
        start,
        end: start + 30,
        label: "deploy".into(),
        subject: Some("deploy media".into()),
        req: Some(4),
        by: None,
        ok: true,
        deferred: None,
        error: None,
        steps: vec![StepTiming {
            step: "pull".into(),
            start,
            end: start + 20,
            changed: true,
        }],
    }
}

#[test]
fn arch_history_a_torn_line_is_skipped_not_fatal() {
    let text = format!(
        "{}{{\"kind\":\"op\",\"sta\n{}",
        op(1).to_line(),
        op(2).to_line()
    );
    let got = parse(&text);
    assert_eq!(got.len(), 2);
    assert_eq!(got[1].start(), 2);
}

#[test]
fn arch_history_select_keeps_the_newest_since_a_moment() {
    let all: Vec<HistoryEntry> = (1..=10).map(|i| op(i * 100)).collect();
    let got = select(all, 300, 3);
    assert_eq!(
        got.iter().map(|e| e.start()).collect::<Vec<_>>(),
        vec![800, 900, 1000]
    );
}

#[test]
fn arch_history_prune_drops_the_old_then_the_oldest_past_the_size() {
    let text: String = (1..=10).map(|i| op(i * 100).to_line()).collect();
    // age: nothing before 500
    let kept = prune(&text, 1000, 500, usize::MAX).unwrap();
    assert_eq!(parse(&kept).first().unwrap().start(), 500);
    // size: room for two lines
    let one = op(1000).to_line().len();
    let kept = prune(&text, 1000, 10_000, one * 2).unwrap();
    assert_eq!(parse(&kept).len(), 2);
    assert_eq!(parse(&kept)[1].start(), 1000);
    // nothing to change: None, so the file is not rewritten
    assert!(prune(&text, 1000, 10_000, usize::MAX).is_none());
}
