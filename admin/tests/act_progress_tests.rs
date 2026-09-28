//! feat-ops-6: "step n/m, expected x" from step marks and the host's history.

use homelab_admin::core::actions_progress::{expectation, median, Tracker};
use homelab_core::history::{HistoryEntry, StepTiming};
use homelab_proto::StepMark;

fn run(subject: &str, start: u64, steps: &[(&str, u64)], ok: bool) -> HistoryEntry {
    let mut t = start;
    let timings = steps
        .iter()
        .map(|(s, d)| {
            let st = StepTiming {
                step: s.to_string(),
                start: t,
                end: t + d,
                changed: true,
            };
            t += d;
            st
        })
        .collect();
    HistoryEntry::Op {
        start,
        end: t,
        label: "deploy".into(),
        subject: Some(subject.into()),
        req: None,
        by: None,
        ok,
        deferred: None,
        error: None,
        steps: timings,
    }
}

fn mark(step: &str, finished: bool) -> StepMark {
    StepMark {
        op: "deploy-media".into(),
        step: step.into(),
        finished,
        changed: false,
    }
}

#[test]
fn feat_ops_6_median_of_odd_even_and_none() {
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[7]), Some(7));
    assert_eq!(median(&[30, 10, 20]), Some(20));
    assert_eq!(median(&[10, 40, 20, 30]), Some(25));
}

#[test]
fn feat_ops_6_the_expectation_comes_from_the_same_op_on_the_same_stack() {
    let h = vec![
        run("deploy-media", 100, &[("pull", 10), ("up", 5)], true),
        run(
            "deploy-media",
            1_000,
            &[("pull", 30), ("up", 7), ("verify", 3)],
            true,
        ),
        run(
            "deploy-media",
            2_000,
            &[("pull", 20), ("up", 6), ("verify", 5)],
            true,
        ),
        // A failed run adds its step times, not its plan or total.
        run("deploy-media", 3_000, &[("pull", 40)], false),
        // Another stack and another op do not count.
        run("deploy-home", 4_000, &[("pull", 999)], true),
        run("backup-media", 5_000, &[("pull", 999)], true),
    ];
    let e = expectation(&h, "deploy-media");
    assert_eq!(e.runs, 4);
    let plan: Vec<(&str, Option<u64>)> = e
        .steps
        .iter()
        .map(|s| (s.step.as_str(), s.median_s))
        .collect();
    // The newest successful run's steps, each with its median.
    assert_eq!(
        plan,
        vec![("pull", Some(25)), ("up", Some(6)), ("verify", Some(4))]
    );
    assert_eq!(e.total_s, Some(31), "median of 15, 40 and 31");
    let none = expectation(&h, "deploy-nothing");
    assert!(none.steps.is_empty() && none.total_s.is_none() && none.runs == 0);
}

#[test]
fn feat_ops_6_step_n_of_m_with_the_expected_rest() {
    let h = vec![
        run(
            "deploy-media",
            100,
            &[("pull", 20), ("up", 10), ("verify", 4)],
            true,
        ),
        run(
            "deploy-media",
            500,
            &[("pull", 40), ("up", 10), ("verify", 6)],
            true,
        ),
    ];
    let mut t = Tracker::new(h);
    let p = t.on_mark(&mark("pull", false), 10_000);
    assert_eq!((p.n, p.m), (1, Some(3)));
    assert_eq!(p.expected_step_s, Some(30));
    assert_eq!(p.expected_total_s, Some(45));
    assert_eq!(p.expected_remaining_s, Some(30 + 10 + 5));
    assert_eq!(p.elapsed_s, 0);
    assert_eq!(p.runs, 2);
    // 12 s into pull: 18 of it left, then up and verify.
    let p = t.on_mark(&mark("pull", true), 10_012);
    assert_eq!(p.n, 1);
    assert!(p.finished);
    assert_eq!(p.expected_remaining_s, Some(15));
    let p = t.on_mark(&mark("up", false), 10_012);
    assert_eq!((p.n, p.m), (2, Some(3)));
    assert_eq!(p.expected_remaining_s, Some(10 + 5));
    assert_eq!(p.elapsed_s, 12);
    // A step the plan never had: counted, m grows with it, no expectation.
    let p = t.on_mark(&mark("migrate", false), 10_030);
    assert_eq!((p.n, p.m), (3, Some(3)));
    let p = t.on_mark(&mark("verify", false), 10_040);
    assert_eq!((p.n, p.m), (4, Some(4)));
    assert_eq!(p.expected_step_s, Some(5));
}

#[test]
fn feat_ops_6_without_history_it_counts_but_does_not_guess() {
    let mut t = Tracker::new(Vec::new());
    let p = t.on_mark(&mark("pull", false), 5);
    assert_eq!((p.n, p.m), (1, None));
    assert_eq!(
        (
            p.expected_step_s,
            p.expected_total_s,
            p.expected_remaining_s
        ),
        (None, None, None)
    );
    let json = serde_json::to_value(&p).unwrap();
    for k in [
        "op",
        "step",
        "n",
        "m",
        "finished",
        "expected_step_s",
        "expected_remaining_s",
        "elapsed_s",
        "runs",
    ] {
        assert!(json.get(k).is_some(), "{k} in {json}");
    }
}
