//! feat-ops-6: "step n/m, expected x" from step marks and the host's history.

use homelab_admin::core::actions_progress::{Tracker, expectation, median};
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
        skipped: false,
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
    // fix-171 round 2: `m` comes from this run's own announced plan, not
    // from history — the op sends it before its first step mark.
    t.on_plan(
        "deploy-media",
        &["pull".into(), "up".into(), "verify".into()],
    );
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
    let p = t.on_mark(&mark("verify", false), 10_040);
    assert_eq!((p.n, p.m), (3, Some(3)), "m stays the announced total");
    assert_eq!(p.expected_step_s, Some(5));
}

/// fix-171 round 2: with no plan announced, `m` stays `None` throughout —
/// it is never derived from history, and never drifts into mirroring `n`
/// (the shape of the original "13/13" -> "68/68" bug).
#[test]
fn feat_ops_6_without_an_announced_plan_m_stays_none_and_never_mirrors_n() {
    let h = vec![run(
        "deploy-media",
        100,
        &[("pull", 20), ("up", 10), ("verify", 4)],
        true,
    )];
    let mut t = Tracker::new(h);
    for (step, ts) in [("pull", 10_000), ("up", 10_010), ("verify", 10_020)] {
        let p = t.on_mark(&mark(step, false), ts);
        assert_eq!(p.m, None, "no plan was ever announced");
        assert_ne!(Some(p.n), p.m);
    }
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

/// ask-kp11be-5: the byte counter's rate follows the recent samples and
/// starts over for a new repository.
#[test]
fn bytes_rate_needs_two_samples_apart_and_restarts_per_repository() {
    let mut t = Tracker::new(Vec::new());
    let a = t.on_bytes("media", 0, None, 100);
    assert_eq!((a.done, a.total, a.rate_bps), (0, None, None));
    // Too soon to say.
    assert_eq!(t.on_bytes("media", 1_000, Some(10_000), 101).rate_bps, None);
    let b = t.on_bytes("media", 50_000, Some(100_000), 105);
    assert_eq!(b.rate_bps, Some(10_000));
    // A new repository starts measuring again.
    let c = t.on_bytes("home", 10, Some(20), 106);
    assert_eq!((c.label.as_str(), c.rate_bps), ("home", None));
}

// ---- one-process-batch-1: a batch is one process with a known step total ----

mod batch_plan {
    use homelab_admin::core::actions_progress::{
        MemberPlan, batch_total, commands_plan, member_plan,
    };
    use homelab_core::history::HistoryEntry;
    use homelab_proto::Command;

    fn destroy(stack: &str) -> Command {
        Command::DestroyRecorded {
            stack: stack.into(),
            confirm: stack.into(),
            skip_backup: false,
        }
    }

    #[test]
    fn a_commands_plan_is_the_destroy_plan_names_in_order() {
        let got = commands_plan(&[destroy("kitchen"), Command::Status, destroy("gateway")]);
        let mut want = homelab_core::ops::destroy::destroy_plan_names("kitchen");
        want.extend(homelab_core::ops::destroy::destroy_plan_names("gateway"));
        assert_eq!(got, want);
    }

    #[test]
    fn a_command_without_a_plan_adds_no_steps() {
        assert!(commands_plan(&[Command::Status, Command::Ping]).is_empty());
    }

    #[test]
    fn a_member_with_announced_commands_is_exact() {
        let m = member_plan(
            7,
            "kitchen",
            Some(&[destroy("kitchen")]),
            &[],
            "destroy-kitchen",
        );
        assert!(m.exact);
        assert_eq!(
            m.steps.len(),
            homelab_core::ops::destroy::destroy_plan_names("kitchen").len()
        );
        assert_eq!((m.job, m.stack.as_str()), (7, "kitchen"));
    }

    #[test]
    fn a_member_whose_commands_run_no_steps_is_exact_with_none() {
        // one-process-batch-2: a command with no steps (a read) is not an
        // estimate; it is known to add nothing.
        let m = member_plan(
            8,
            "gateway",
            Some(&[Command::Status]),
            &[],
            "status-gateway",
        );
        assert!(m.exact);
        assert!(m.steps.is_empty());
    }

    #[test]
    fn a_member_whose_inputs_were_not_read_is_estimated_from_history_and_not_exact() {
        // zfs-replicate's steps depend on host.toml's jobs, which `member_plan`
        // (no facts read) cannot know.
        let history: Vec<HistoryEntry> = Vec::new();
        let m = member_plan(
            8,
            "_host",
            Some(&[Command::ZfsReplicate]),
            &history,
            "zfs-replicate-_host",
        );
        assert!(!m.exact);
        assert!(m.steps.is_empty());
        assert_eq!(m.estimated_because.len(), 1);
    }

    #[test]
    fn the_batch_total_is_the_sum_and_exact_only_when_every_member_is() {
        let a = MemberPlan {
            job: 1,
            stack: "a".into(),
            steps: vec!["x".into(), "y".into()],
            exact: true,
            estimated_because: Vec::new(),
        };
        let b = MemberPlan {
            job: 2,
            stack: "b".into(),
            steps: vec!["x".into(), "y".into(), "z".into()],
            exact: true,
            estimated_because: Vec::new(),
        };
        assert_eq!(batch_total(&[a.clone(), b.clone()]), (5, true));
        let c = MemberPlan { exact: false, ..b };
        assert_eq!(batch_total(&[a, c]), (5, false));
        assert_eq!(batch_total(&[]), (0, true));
    }
}

fn unit_run(subject: &str, req: u64, start: u64, steps: &[(&str, u64)], ok: bool) -> HistoryEntry {
    let HistoryEntry::Op {
        end,
        label,
        subject,
        steps,
        ..
    } = run(subject, start, steps, ok)
    else {
        unreachable!()
    };
    HistoryEntry::Op {
        start,
        end,
        label,
        subject,
        req: Some(req),
        by: None,
        ok,
        deferred: None,
        error: None,
        steps,
    }
}

/// timebar-after-done-1: a command that ran one operation per unit leaves
/// one history line per unit under one request; the median is the whole
/// command's time, not one unit's slice of it.
#[test]
fn timebar_after_done_1_the_median_is_of_whole_commands_not_of_one_unit() {
    let h = vec![
        unit_run("update-kyu", 7, 100, &[("pull", 2)], true),
        unit_run("update-kyu", 7, 102, &[("pull", 50), ("up", 12)], true),
        unit_run("update-kyu", 9, 1_000, &[("pull", 2)], true),
        unit_run("update-kyu", 9, 1_002, &[("pull", 46), ("up", 16)], true),
    ];
    let e = expectation(&h, "update-kyu");
    assert_eq!(e.runs, 2, "two commands, not four lines");
    assert_eq!(e.total_s, Some(64), "2 + 62 s per command");
    let names: Vec<&str> = e.steps.iter().map(|s| s.step.as_str()).collect();
    assert_eq!(
        names,
        ["pull", "up"],
        "a step name appears once in the plan"
    );
}

/// timebar-after-done-1: a job of two commands carries the sum of both
/// operations' whole-command medians once both have announced themselves.
#[test]
fn timebar_after_done_1_a_two_command_job_sums_its_operations() {
    let h = vec![
        run("backup-kyu", 100, &[("dump", 6)], true),
        run("update-kyu", 200, &[("pull", 50), ("up", 8)], true),
    ];
    let mut t = Tracker::new(h);
    let mark = |op: &str, step: &str, finished: bool| homelab_proto::StepMark {
        op: op.into(),
        step: step.into(),
        finished,
        changed: false,
        skipped: false,
    };
    let first = t.on_mark(&mark("backup-kyu", "dump", false), 1_000);
    assert_eq!(first.expected_total_s, Some(6));
    t.on_mark(&mark("backup-kyu", "dump", true), 1_006);
    let second = t.on_mark(&mark("update-kyu", "pull", false), 1_007);
    assert_eq!(second.expected_total_s, Some(64), "6 + 58");
    assert_eq!(second.expected_step_s, Some(50));
}

/// timebar-after-done-1, measured 2026-10-23 in history.jsonl: nightly
/// "scheduled-release-update" lines of 2 s under the same subject set the
/// median for a dashboard install of a minute. A person's runs exist, so the
/// scheduled ones are left out.
#[test]
fn timebar_after_done_1_scheduled_no_op_runs_do_not_set_the_median() {
    let scheduled = |start: u64| {
        let HistoryEntry::Op {
            end,
            subject,
            steps,
            ok,
            ..
        } = run("release-update-kyu", start, &[("ask GitHub", 2)], true)
        else {
            unreachable!()
        };
        HistoryEntry::Op {
            start,
            end,
            label: "scheduled-release-update".into(),
            subject,
            req: None,
            by: None,
            ok,
            deferred: None,
            error: None,
            steps,
        }
    };
    let h = vec![
        scheduled(10),
        scheduled(20),
        scheduled(30),
        unit_run(
            "release-update-kyu",
            7,
            100,
            &[("ask GitHub", 2), ("install", 26)],
            true,
        ),
    ];
    let e = expectation(&h, "release-update-kyu");
    assert_eq!(e.runs, 1, "only the run a person started");
    assert_eq!(e.total_s, Some(28));
}
