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
    /// 1-based position of this step in this run.
    pub n: usize,
    /// Steps expected in all: the newest successful run's own count, held
    /// fixed. None when the operation never succeeded before, or once this
    /// run has taken more steps than that plan knew about (fix-171: the
    /// total used to be `plan.len().max(n)`, "at least n", so once a run
    /// outgrew the plan it read `m == n` on every mark and the pair climbed
    /// together — "13/13" became "68/68" while BOTH numbers rose. An
    /// honest "step n" beats a total that is simply wrong).
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
#[derive(Debug, Clone, Default)]
pub struct Tracker {
    history: Vec<HistoryEntry>,
    plan: Option<Expectation>,
    /// Steps started so far with their start time, in order.
    started: Vec<(String, u64)>,
    first_ts: Option<u64>,
}

impl Tracker {
    pub fn new(history: Vec<HistoryEntry>) -> Self {
        Tracker {
            history,
            ..Default::default()
        }
    }

    /// The expectation once the first mark named the operation.
    pub fn expectation(&self) -> Option<&Expectation> {
        self.plan.as_ref()
    }

    /// A mark as it arrived, with the time the host put on its line.
    pub fn on_mark(&mut self, mark: &StepMark, ts: u64) -> Progress {
        if self.plan.is_none() {
            self.plan = Some(expectation(&self.history, &mark.op));
        }
        let first = *self.first_ts.get_or_insert(ts);
        if !mark.finished {
            self.started.push((mark.step.clone(), ts));
        }
        let n = if mark.finished {
            self.started
                .iter()
                .rposition(|(s, _)| s == &mark.step)
                .map(|i| i + 1)
                .unwrap_or(self.started.len().max(1))
        } else {
            self.started.len()
        };
        let plan = self.plan.clone().unwrap_or_default();
        // fix-171: `m` is the plan's own length, fixed — never raised to
        // keep up with `n`. Once `n` outgrows it the plan no longer
        // describes this run, so the total goes unknown rather than lying.
        let m = (!plan.steps.is_empty() && n <= plan.steps.len()).then_some(plan.steps.len());
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

    fn mark(op: &str, step: &str, finished: bool) -> StepMark {
        StepMark {
            op: op.into(),
            step: step.into(),
            finished,
            changed: finished,
        }
    }

    /// fix-171 (Kenny, 2026-10-02 06:55): a batch "Install newest release"
    /// ran on two stacks; the live job panel went "13/13" to "68/68" with
    /// BOTH numbers climbing during the one run. The plan's last successful
    /// run had 13 steps; this run needed more. `m` used to be
    /// `plan.len().max(n)`, so it tracked `n` exactly once `n` passed the
    /// plan — the fix holds `m` at the plan's own length and reports
    /// `None` (not a new, equally wrong total) once the run outgrows it.
    #[test]
    fn m_stays_fixed_at_the_plans_length_and_never_grows_with_n() {
        let subject = "release-update-native-almanac";
        let history = vec![op(subject, 1_000, 1_013, true, 13)];
        let mut t = Tracker::new(history);

        // Steps 1..=13 match the plan: m is the plan's own fixed length.
        // `finished: false` (a step's start mark) is what makes `n` count
        // up one per mark (`Tracker::on_mark`'s `started` list).
        for i in 0..13u64 {
            let p = t.on_mark(&mark(subject, &format!("step-{i}"), false), 2_000 + i);
            assert_eq!(p.n, i as usize + 1);
            assert_eq!(p.m, Some(13), "m must stay 13, not grow with n");
        }

        // Step 14 outgrows the plan: n keeps counting, m goes unknown
        // rather than becoming 14 (which the old `max(n)` logic did).
        let p14 = t.on_mark(&mark(subject, "step-13", false), 2_020);
        assert_eq!(p14.n, 14);
        assert_eq!(
            p14.m, None,
            "once n exceeds the plan, m must not silently become n"
        );

        // The run keeps going well past the plan (the historical "68"):
        // m must never reappear as a number that merely mirrors n.
        for i in 14..68u64 {
            let p = t.on_mark(&mark(subject, &format!("step-{i}"), false), 2_020 + i);
            assert_eq!(p.n, i as usize + 1);
            assert_ne!(
                p.m,
                Some(p.n),
                "m must never equal n once the plan is outgrown (the 13/13 -> 68/68 bug)"
            );
            assert_eq!(p.m, None);
        }
    }
}
