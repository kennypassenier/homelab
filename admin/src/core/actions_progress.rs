//! feat-ops-6: "step 3/35, expected 40 s". The host marks every step's start
//! and end on its `Log` lines (`StepMark`, feat-platform-3) and keeps every
//! operation's step times in `history.jsonl` (arch-history). This module
//! turns the two into progress: which step of how many, what this step and
//! the rest are expected to take, from the median of the same operation's
//! past runs on the same stack. Pure: the caller passes the history it read
//! and the time each mark carries.

use std::collections::BTreeMap;

use homelab_core::history::HistoryEntry;
use homelab_proto::StepMark;
use serde::Serialize;

/// The median of whole seconds; the mean of the two middle values when the
/// count is even. None for no values.
pub fn median(values: &[u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let mid = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[mid]
    } else {
        (v[mid - 1] + v[mid]) / 2
    })
}

/// What past runs of one operation say about the next.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Expectation {
    /// The operation's name as its steps say it (`deploy-media`).
    pub subject: String,
    /// The steps of the newest successful run, in order, each with the
    /// median of its duration over every run that finished it.
    pub steps: Vec<ExpectedStep>,
    /// Median of whole-run durations over the successful runs.
    pub total_s: Option<u64>,
    /// How many past runs the numbers come from.
    pub runs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExpectedStep {
    pub step: String,
    pub median_s: Option<u64>,
}

/// The expectation for `subject` from the host's history. Successful runs
/// set the plan (the step list) and the total; every run that finished a
/// step contributes that step's time, so a run that failed late still says
/// how long its early steps took.
pub fn expectation(history: &[HistoryEntry], subject: &str) -> Expectation {
    let mut ok_runs: Vec<(u64, &Vec<homelab_core::history::StepTiming>, u64)> = Vec::new();
    let mut per_step: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    let mut runs = 0;
    for e in history {
        let HistoryEntry::Op {
            start,
            end,
            subject: Some(s),
            ok,
            steps,
            ..
        } = e
        else {
            continue;
        };
        if s != subject {
            continue;
        }
        runs += 1;
        for t in steps {
            if t.end >= t.start && t.end != 0 {
                per_step.entry(&t.step).or_default().push(t.end - t.start);
            }
        }
        if *ok && *end >= *start {
            ok_runs.push((*start, steps, end - start));
        }
    }
    ok_runs.sort_by_key(|r| r.0);
    let plan: Vec<ExpectedStep> = ok_runs
        .last()
        .map(|(_, steps, _)| {
            steps
                .iter()
                .map(|t| ExpectedStep {
                    step: t.step.clone(),
                    median_s: per_step.get(t.step.as_str()).and_then(|v| median(v)),
                })
                .collect()
        })
        .unwrap_or_default();
    let totals: Vec<u64> = ok_runs.iter().map(|r| r.2).collect();
    Expectation {
        subject: subject.to_string(),
        steps: plan,
        total_s: median(&totals),
        runs,
    }
}

/// One progress event, as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    /// The operation the steps belong to (`deploy-media`).
    pub op: String,
    pub step: String,
    /// 1-based position of this step (run or skipped) in this run.
    pub n: usize,
    /// Steps planned in all, as the operation(s) themselves announced
    /// before their first step (`Tracker::on_plan`/`set_total`) — never a
    /// guess from a past run. fix-171's first attempt held `m` at the
    /// newest successful run's OWN step count instead, which is still a
    /// guess about THIS run from a DIFFERENT one: correct until a run
    /// needed more steps than that one happened to, then "13/13" became
    /// "68/68" live (`plan.len().max(n)`) or, pinned, quietly wrong
    /// (`Some(13)` forever while `n` kept climbing past it). `m` is `None`
    /// only until the first plan arrives; after that it is this run's own
    /// real total, fixed, and it is reached exactly because a step this
    /// run's plan lists but does not take is marked "skipped" rather than
    /// left out.
    pub m: Option<usize>,
    pub finished: bool,
    pub changed: bool,
    /// Median duration of this step in past runs.
    pub expected_step_s: Option<u64>,
    /// Median duration of the whole operation in past runs.
    pub expected_total_s: Option<u64>,
    /// What is left by the medians: the rest of this step and every planned
    /// step not started yet.
    pub expected_remaining_s: Option<u64>,
    /// Seconds since the operation's first step started.
    pub elapsed_s: u64,
    /// How many past runs the expectation rests on.
    pub runs: usize,
}

/// Follows one operation's step marks.
///
/// fix-171 round 2: `m` (the total) no longer comes from history at all —
/// that was always a guess about THIS run from a DIFFERENT one. It comes
/// from the plan the operation(s) themselves announce before their first
/// step mark (`on_plan`), or, for a job that sends several commands in a
/// row (Apply, a batch item), from the caller's own precomputed sum of
/// each command's plan (`set_total`) — known before the first one is even
/// sent, so the dashboard's "step n/m" is correct from the very first mark
/// of the whole job, not just from the first mark of its first command.
#[derive(Debug, Clone, Default)]
pub struct Tracker {
    history: Vec<HistoryEntry>,
    /// Kept only for the step/total DURATION estimates (`expected_*_s`
    /// below) — a reasonable thing to estimate from past runs. The STEP
    /// COUNT is a different question, and no longer comes from here.
    plan: Option<Expectation>,
    /// Steps started or skipped so far, in order, with the time each
    /// started (a skip uses the time its one mark arrived).
    started: Vec<(String, u64)>,
    first_ts: Option<u64>,
    /// The real total this run (or job) announced for itself. `None` until
    /// the first plan arrives.
    total: Option<usize>,
    /// Set by `set_total`: the whole job's total is already fixed, so a
    /// later `on_plan` (a further command's own announcement) must not
    /// add to it again.
    total_fixed: bool,
}

impl Tracker {
    pub fn new(history: Vec<HistoryEntry>) -> Self {
        Tracker {
            history,
            ..Default::default()
        }
    }

    /// The expectation once the first mark named the operation (duration
    /// estimates only — see the struct doc for why the step count does not
    /// live here).
    pub fn expectation(&self) -> Option<&Expectation> {
        self.plan.as_ref()
    }

    /// fix-171 round 2: fix the WHOLE job's total before anything is sent —
    /// Apply and a batch's own command list already say, from the specs
    /// and stacks involved, exactly how many step-marks (run or skipped)
    /// every command in it will produce; the caller sums each command's
    /// own plan length and calls this once, before the first command goes
    /// out. Any further `on_plan` (each command's own announcement,
    /// arriving as it starts) is then a confirmation, not an addition.
    pub fn set_total(&mut self, total: usize) {
        self.total = Some(total);
        self.total_fixed = true;
    }

    /// One operation's own announcement of its full step plan, arriving
    /// before its first step mark. For a single-command job this call
    /// alone sets `m`. For a job that runs several commands in sequence
    /// with no `set_total` call, each further announcement ADDS to the
    /// running total, so the total only reaches its true value once every
    /// command has started — which is why a multi-command job should call
    /// `set_total` instead, up front.
    pub fn on_plan(&mut self, op: &str, steps: &[String]) {
        if self.plan.is_none() {
            self.plan = Some(expectation(&self.history, op));
        }
        if !self.total_fixed {
            *self.total.get_or_insert(0) += steps.len();
        }
    }

    /// A mark as it arrived, with the time the host put on its line.
    pub fn on_mark(&mut self, mark: &StepMark, ts: u64) -> Progress {
        if self.plan.is_none() {
            self.plan = Some(expectation(&self.history, &mark.op));
        }
        let first = *self.first_ts.get_or_insert(ts);
        // A skip is one mark, not a start/finish pair, but it still fills
        // one slot of the plan — counted here exactly like a start.
        if !mark.finished || mark.skipped {
            self.started.push((mark.step.clone(), ts));
        }
        let n = if mark.finished && !mark.skipped {
            self.started
                .iter()
                .rposition(|(s, _)| s == &mark.step)
                .map(|i| i + 1)
                .unwrap_or(self.started.len().max(1))
        } else {
            self.started.len()
        };
        let plan = self.plan.clone().unwrap_or_default();
        // fix-171 round 2: `m` is this run's own announced total, fixed —
        // never derived from a past run (see the struct doc).
        let m = self.total;
        let expected_step_s = plan
            .steps
            .iter()
            .find(|s| s.step == mark.step)
            .and_then(|s| s.median_s);
        // The rest: what is left of this step, then every planned step that
        // has not started yet (by name, so a step skipped this time does not
        // count twice).
        let step_started = self
            .started
            .iter()
            .rev()
            .find(|(s, _)| s == &mark.step)
            .map(|(_, t)| *t)
            .unwrap_or(ts);
        let left_of_this = if mark.finished {
            Some(0)
        } else {
            expected_step_s.map(|e| e.saturating_sub(ts.saturating_sub(step_started)))
        };
        let expected_remaining_s = if plan.steps.is_empty() {
            None
        } else {
            let not_started: u64 = plan
                .steps
                .iter()
                .filter(|p| !self.started.iter().any(|(s, _)| s == &p.step))
                .filter_map(|p| p.median_s)
                .sum();
            Some(left_of_this.unwrap_or(0) + not_started)
        };
        Progress {
            op: mark.op.clone(),
            step: mark.step.clone(),
            n,
            m,
            finished: mark.finished,
            changed: mark.changed,
            expected_step_s,
            expected_total_s: plan.total_s,
            expected_remaining_s,
            elapsed_s: ts.saturating_sub(first),
            runs: plan.runs,
        }
    }
}

/// arch-self: a job whose action restarts the dashboard
/// (`JobView::restarts_dashboard`) loses the dashboard's own memory of how
/// it ended; the host's history still has it. A few seconds of clock skew
/// are allowed between the job's `started_at` and the host's recorded
/// start, the two clocks not being the same one.
pub const OUTCOME_SKEW_S: i64 = 5;

/// What the host's history says a job that restarted the dashboard came to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Outcome {
    pub found: bool,
    pub ok: bool,
    /// The steps the finished run recorded; the page's final "step n/m".
    pub steps: usize,
    pub end: i64,
}

/// The host history entry for `subject` (a job's stack-action, e.g.
/// `install-native-admin`) whose start is at/after `since` (the job's
/// `started_at`, a few seconds of skew allowed). Entries are read oldest
/// first (arch-history), so the earliest match is the run the job itself
/// started, not one pressed again afterwards.
pub fn outcome_since(history: &[HistoryEntry], subject: &str, since: i64) -> Outcome {
    let hit = history.iter().find(|e| match e {
        HistoryEntry::Op {
            start,
            subject: Some(s),
            ..
        } => s == subject && *start as i64 >= since - OUTCOME_SKEW_S,
        _ => false,
    });
    match hit {
        Some(HistoryEntry::Op { end, ok, steps, .. }) => Outcome {
            found: true,
            ok: *ok,
            steps: steps.len(),
            end: *end as i64,
        },
        _ => Outcome::default(),
    }
}

#[cfg(test)]
mod outcome_tests {
    use super::*;

    fn op(subject: &str, start: u64, end: u64, ok: bool, n_steps: usize) -> HistoryEntry {
        HistoryEntry::Op {
            start,
            end,
            label: "install-native".into(),
            subject: Some(subject.into()),
            req: None,
            by: None,
            ok,
            deferred: None,
            error: None,
            steps: (0..n_steps)
                .map(|i| homelab_core::history::StepTiming {
                    step: format!("step-{i}"),
                    start,
                    end: start + 1,
                    changed: true,
                })
                .collect(),
        }
    }

    #[test]
    fn outcome_since_finds_the_run_that_started_at_or_after_the_job() {
        let history = vec![
            op("install-native-admin", 1_000, 1_010, true, 3),
            op("install-native-admin", 2_000, 2_022, true, 22),
            op("deploy-media", 2_005, 2_030, true, 10),
        ];
        let out = outcome_since(&history, "install-native-admin", 2_000);
        assert_eq!(
            out,
            Outcome {
                found: true,
                ok: true,
                steps: 22,
                end: 2_022,
            }
        );
    }

    #[test]
    fn outcome_since_allows_a_few_seconds_of_clock_skew() {
        let history = vec![op("install-native-admin", 1_998, 2_020, false, 5)];
        // The job's started_at (2_000) is 2 s after the host's own start;
        // within OUTCOME_SKEW_S this still counts as the same run.
        let out = outcome_since(&history, "install-native-admin", 2_000);
        assert!(out.found);
        assert!(!out.ok);
        assert_eq!(out.steps, 5);
    }

    #[test]
    fn outcome_since_nothing_before_the_skew_window_is_unfound() {
        let history = vec![op("install-native-admin", 1_000, 1_010, true, 3)];
        let out = outcome_since(&history, "install-native-admin", 2_000);
        assert_eq!(out, Outcome::default());
        assert!(!out.found);
    }

    #[test]
    fn outcome_since_a_different_subject_does_not_match() {
        let history = vec![op("deploy-media", 2_000, 2_010, true, 5)];
        let out = outcome_since(&history, "install-native-admin", 2_000);
        assert!(!out.found);
    }
}

#[cfg(test)]
mod tracker_m_tests {
    use super::*;

    fn mark(op: &str, step: &str, finished: bool) -> StepMark {
        StepMark {
            op: op.into(),
            step: step.into(),
            finished,
            changed: finished,
            skipped: false,
        }
    }

    fn skip(op: &str, step: &str) -> StepMark {
        StepMark {
            op: op.into(),
            step: step.into(),
            finished: true,
            changed: false,
            skipped: true,
        }
    }

    fn plan_names(n: usize, prefix: &str) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}-{i}")).collect()
    }

    /// fix-171 round 2 (Kenny, 2026-10-02): the FIRST attempt held `m` at
    /// the newest successful run's OWN step count from history — still a
    /// guess about this run from a different one, rejected because a run
    /// that genuinely needs more steps than that history-guess allowed for
    /// still has no honest total. `m` now comes only from the op's own
    /// announced plan (`on_plan`), computed from this run's real inputs,
    /// never from a past run — so even a run with MORE steps than any
    /// prior one gets a correct, fixed `m` from its own first mark.
    #[test]
    fn m_comes_from_the_announced_plan_not_from_a_past_runs_count() {
        let subject = "release-update-native-almanac";
        // History says the last successful run took 13 steps; THIS run's
        // own plan says 68 — bigger than any run on record. `m` must
        // follow the announcement, not the history.
        let mut t = Tracker::new(Vec::new());
        t.on_plan(subject, &plan_names(68, "step"));

        for i in 0..68u64 {
            let p = t.on_mark(&mark(subject, &format!("step-{i}"), false), 2_000 + i);
            assert_eq!(p.n, i as usize + 1);
            assert_eq!(p.m, Some(68), "m is this run's own announced total");
        }
    }

    /// Before any plan has arrived, `m` is `None` — never guessed, and
    /// never equal to `n` either (the shape of the original bug).
    #[test]
    fn m_is_none_until_a_plan_arrives() {
        let subject = "deploy-media";
        let mut t = Tracker::new(Vec::new());
        for i in 0..5u64 {
            let p = t.on_mark(&mark(subject, &format!("step-{i}"), false), 2_000 + i);
            assert_eq!(p.m, None);
            assert_ne!(Some(p.n), p.m);
        }
    }

    /// A step the plan lists but this run does not take still fills one
    /// slot: `n` reaches `m` exactly, because the skip is one mark, not a
    /// missing one.
    #[test]
    fn a_skipped_step_still_advances_n_to_meet_the_announced_total() {
        let subject = "deploy-media";
        let mut t = Tracker::new(Vec::new());
        t.on_plan(subject, &plan_names(3, "step"));

        let p1 = t.on_mark(&mark(subject, "step-0", false), 10);
        assert_eq!((p1.n, p1.m), (1, Some(3)));
        let p1b = t.on_mark(&mark(subject, "step-0", true), 11);
        assert_eq!((p1b.n, p1b.m), (1, Some(3)));

        // step-1's precondition did not hold this run: one skip mark.
        let p2 = t.on_mark(&skip(subject, "step-1"), 12);
        assert_eq!((p2.n, p2.m), (2, Some(3)));
        assert!(p2.finished);

        let p3 = t.on_mark(&mark(subject, "step-2", false), 13);
        assert_eq!((p3.n, p3.m), (3, Some(3)));
        let p3b = t.on_mark(&mark(subject, "step-2", true), 14);
        assert_eq!((p3b.n, p3b.m), (3, Some(3)), "n reaches m exactly");
    }

    /// fix-171 round 2: a job that sends several commands in a row (Apply,
    /// a batch item) fixes the WHOLE total up front with `set_total`,
    /// computed by the caller from every command's own plan before the
    /// first one is even sent — so `m` is correct from the job's very
    /// first mark, not just from its first command's first mark. Each
    /// command's own announcement, arriving as it starts, then confirms
    /// rather than adds to the total.
    #[test]
    fn set_total_fixes_a_multi_command_jobs_total_before_the_first_mark() {
        let mut t = Tracker::new(Vec::new());
        // Two stacks, 28 steps each (deploy's own fixed plan length) —
        // known before either deploy is sent.
        t.set_total(56);

        let p = t.on_mark(&mark("deploy-media", "validate", false), 1);
        assert_eq!(
            (p.n, p.m),
            (1, Some(56)),
            "m is already the job's whole total"
        );

        // The first stack announces its own plan as it starts: this must
        // NOT add to the total that was already fixed.
        t.on_plan("deploy-media", &plan_names(28, "step"));
        let p = t.on_mark(&mark("deploy-media", "safety gates", false), 2);
        assert_eq!(
            p.m,
            Some(56),
            "a sub-op's own plan does not add to a fixed total"
        );

        // Running the first stack's 28 marks, then the second stack's own
        // plan arriving: still no change to the fixed total.
        for i in 0..26u64 {
            t.on_mark(&mark("deploy-media", &format!("s{i}"), false), 3 + i);
        }
        t.on_plan("deploy-drill", &plan_names(28, "step"));
        let p = t.on_mark(&mark("deploy-drill", "validate", false), 100);
        assert_eq!(p.n, 29, "n keeps counting across the job's commands");
        assert_eq!(p.m, Some(56));
    }
}
