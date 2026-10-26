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
use serde::{Deserialize, Serialize};

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

/// median-outlier-1 (Kenny, 2026-10-24: a deploy of the registry read
/// "5 days"): no operation here takes longer than this, so a longer run is a
/// job left open or a clock jump, not a duration.
pub const PLAUSIBLE_MAX_S: u64 = 12 * 3600;
/// With at least three durations, one this many times the median of them all
/// is left out as well.
const OUTLIER_FACTOR: u64 = 20;

/// The durations that can be real: none past [`PLAUSIBLE_MAX_S`], and, when
/// three or more are left, none past [`OUTLIER_FACTOR`] times their median.
pub fn plausible(values: &[u64]) -> Vec<u64> {
    let kept: Vec<u64> = values
        .iter()
        .copied()
        .filter(|v| *v <= PLAUSIBLE_MAX_S)
        .collect();
    match median(&kept) {
        Some(m) if kept.len() >= 3 => kept
            .into_iter()
            .filter(|v| *v <= m.max(1).saturating_mul(OUTLIER_FACTOR))
            .collect(),
        _ => kept,
    }
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
    /// feat-timing-1 (Kenny, 2026-10-09: "de mediaan ... en ook de minimum
    /// en maximum gemeten tijden"): the shortest and longest of those runs.
    pub total_min_s: Option<u64>,
    pub total_max_s: Option<u64>,
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
    // timebar-after-done-1: one command can run several host operations of
    // the same subject (a native stack fans out over its units, each unit one
    // history line under the same request). The time of "this operation" is
    // the whole command, first start to last end, never one unit's slice, so
    // the median compares like with like. Lines of one request and label are
    // folded into one run; a line without a request (the host's own work) is
    // a run by itself.
    struct Run<'a> {
        start: u64,
        end: u64,
        ok: bool,
        steps: Vec<&'a homelab_core::history::StepTiming>,
    }
    // Measured 2026-10-23 in the host's history.jsonl: the nightly round
    // writes "scheduled-release-update" lines under the same subject
    // ("release-update-kyu") that take 2 s when there is nothing to install,
    // so they set the median for a dashboard install of a minute. When the
    // subject has runs started by a person, the scheduled ones are left out.
    let by_person = history.iter().any(|e| {
        matches!(e, HistoryEntry::Op { subject: Some(s), label, .. }
            if s == subject && !label.starts_with("scheduled-"))
    });
    let mut runs_found: Vec<(Option<(u64, &str)>, Run)> = Vec::new();
    for e in history {
        let HistoryEntry::Op {
            start,
            end,
            label,
            subject: Some(s),
            req,
            ok,
            steps,
            ..
        } = e
        else {
            continue;
        };
        if s != subject || (by_person && label.starts_with("scheduled-")) {
            continue;
        }
        let key = req.map(|r| (r, label.as_str()));
        if let Some(key) = key
            && let Some((_, run)) = runs_found.iter_mut().find(|(k, _)| *k == Some(key))
        {
            run.start = run.start.min(*start);
            run.end = run.end.max(*end);
            run.ok &= *ok;
            run.steps.extend(steps.iter());
            continue;
        }
        runs_found.push((
            key,
            Run {
                start: *start,
                end: *end,
                ok: *ok,
                steps: steps.iter().collect(),
            },
        ));
    }
    let mut ok_runs: Vec<(u64, Vec<&homelab_core::history::StepTiming>, u64)> = Vec::new();
    let mut per_step: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    let mut runs = 0;
    for (_, r) in runs_found {
        runs += 1;
        for t in &r.steps {
            if t.end >= t.start && t.end != 0 {
                per_step.entry(&t.step).or_default().push(t.end - t.start);
            }
        }
        if r.ok && r.end >= r.start {
            ok_runs.push((r.start, r.steps, r.end - r.start));
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
                    median_s: per_step
                        .get(t.step.as_str())
                        .and_then(|v| median(&plausible(v))),
                })
                .fold(Vec::<ExpectedStep>::new(), |mut acc, s| {
                    if !acc.iter().any(|a| a.step == s.step) {
                        acc.push(s);
                    }
                    acc
                })
        })
        .unwrap_or_default();
    let totals: Vec<u64> = plausible(&ok_runs.iter().map(|r| r.2).collect::<Vec<_>>());
    Expectation {
        subject: subject.to_string(),
        steps: plan,
        total_s: median(&totals),
        total_min_s: totals.iter().copied().min(),
        total_max_s: totals.iter().copied().max(),
        runs,
    }
}

/// One progress event, as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// feat-timing-1: the shortest and longest past run.
    pub expected_total_min_s: Option<u64>,
    pub expected_total_max_s: Option<u64>,
    /// What is left by the medians: the rest of this step and every planned
    /// step not started yet.
    pub expected_remaining_s: Option<u64>,
    /// Seconds since the operation's first step started.
    pub elapsed_s: u64,
    /// How many past runs the expectation rests on.
    pub runs: usize,
}

/// ask-kp11be-5: how many bytes the operation has moved in its current
/// transfer (a restic backup or restore of one repository), as the job
/// panel's bar shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobBytes {
    /// What is moving: the repository's owning app.
    pub label: String,
    pub done: u64,
    /// None while restic is still scanning.
    pub total: Option<u64>,
    /// Bytes per second over the last stretch of samples; None until two
    /// samples are far enough apart to say.
    pub rate_bps: Option<u64>,
}

/// A rate needs this many seconds between two samples to mean anything.
const RATE_MIN_S: u64 = 3;
/// The rate's window slides after this long, so it follows the recent
/// speed, not the average of a night.
const RATE_WINDOW_S: u64 = 20;

/// Turns the host's byte counters into [`JobBytes`] with a rate. Pure: the
/// caller passes the time of each sample. Holds one anchor sample, so it
/// grows with nothing.
#[derive(Debug, Clone, Default)]
pub struct BytesRate {
    label: String,
    anchor: Option<(u64, u64)>,
    rate: Option<u64>,
}

impl BytesRate {
    pub fn on_sample(
        &mut self,
        label: &str,
        done: u64,
        total: Option<u64>,
        now_s: u64,
    ) -> JobBytes {
        // A new repository, or a counter that went backwards (a second
        // pass): start measuring again.
        if self.label != label || self.anchor.is_some_and(|(_, d)| done < d) {
            self.label = label.to_string();
            self.anchor = None;
            self.rate = None;
        }
        match self.anchor {
            None => self.anchor = Some((now_s, done)),
            Some((at, was)) => {
                let dt = now_s.saturating_sub(at);
                if dt >= RATE_MIN_S {
                    self.rate = Some((done - was) / dt);
                    if dt >= RATE_WINDOW_S {
                        self.anchor = Some((now_s, done));
                    }
                }
            }
        }
        JobBytes {
            label: label.to_string(),
            done,
            total,
            rate_bps: self.rate,
        }
    }
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
    /// timebar-after-done-1: the expectation of every operation this job has
    /// announced so far, in order. A job of several commands (a backup, then
    /// the install) is several operations in the host's history, each with
    /// its own whole-run median; the job's median is their sum, so it is
    /// never the median of one sub-operation alone.
    ops: Vec<Expectation>,
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
    /// Set by `expect_total`: the total was computed by the dashboard from
    /// the command's own inputs before anything was sent. The first plan the
    /// host announces replaces it (the host's own count is the truth should
    /// the dashboard's inputs ever have differed from the host's), instead of
    /// being added to it.
    total_expected: bool,
    /// ask-kp11be-5: the byte counter's rate.
    bytes: BytesRate,
}

impl Tracker {
    /// ask-kp11be-5: a byte counter of this job's operation.
    pub fn on_bytes(&mut self, label: &str, done: u64, total: Option<u64>, now_s: u64) -> JobBytes {
        self.bytes.on_sample(label, done, total, now_s)
    }

    pub fn new(history: Vec<HistoryEntry>) -> Self {
        Tracker {
            history,
            ..Default::default()
        }
    }

    /// Adds `op`'s expectation to the job's when it is new, and refreshes
    /// the merged `plan`: the first operation's steps (the per-step medians
    /// and the remaining time), and the totals summed over every operation
    /// seen so far.
    fn learn(&mut self, op: &str) {
        if self.ops.iter().any(|e| e.subject == op) {
            return;
        }
        self.ops.push(expectation(&self.history, op));
        let mut merged = self.ops[0].clone();
        let sum = |f: &dyn Fn(&Expectation) -> Option<u64>| -> Option<u64> {
            let all: Vec<u64> = self.ops.iter().filter_map(f).collect();
            (!all.is_empty()).then(|| all.iter().sum())
        };
        merged.total_s = sum(&|e| e.total_s);
        merged.total_min_s = sum(&|e| e.total_min_s);
        merged.total_max_s = sum(&|e| e.total_max_s);
        merged.runs = self.ops.iter().map(|e| e.runs).min().unwrap_or(0);
        for e in &self.ops[1..] {
            for st in &e.steps {
                if !merged.steps.iter().any(|m| m.step == st.step) {
                    merged.steps.push(st.clone());
                }
            }
        }
        self.plan = Some(merged);
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
        self.total_expected = false;
    }

    /// one-process-batch-2: the total of a job whose ONE command announces
    /// ONE plan, computed by the dashboard from that command's own inputs
    /// (`commands_plan_with`) before it is sent, so "step 0 of m" shows
    /// before the host has said a word. It is exact by construction, so the
    /// host's announcement normally equals it; should the dashboard's inputs
    /// ever have differed from the host's, the first announcement replaces it
    /// (never adds to it) and the host's count wins.
    pub fn expect_total(&mut self, total: usize) {
        self.total = Some(total);
        self.total_fixed = false;
        self.total_expected = true;
    }

    /// One operation's own announcement of its full step plan, arriving
    /// before its first step mark. For a single-command job this call
    /// alone sets `m`. For a job that runs several commands in sequence
    /// with no `set_total` call, each further announcement ADDS to the
    /// running total, so the total only reaches its true value once every
    /// command has started — which is why a multi-command job should call
    /// `set_total` instead, up front.
    pub fn on_plan(&mut self, op: &str, steps: &[String]) {
        self.learn(op);
        if self.total_expected {
            self.total_expected = false;
            self.total = Some(steps.len());
        } else if !self.total_fixed {
            *self.total.get_or_insert(0) += steps.len();
        }
    }

    /// steps-total-known-1 (Kenny, 2026-10-22: "we weten toch altijd hoeveel
    /// stappen iets moet doorlopen?"): the progress before the first step
    /// mark, once a total is known (`set_total`, or the first `on_plan`):
    /// step 0 of `m`. The page shows the total, and every step of the plan
    /// waiting, from the first frame instead of "0 of —". None while no
    /// total is known, or once a mark has arrived (the marks say it then).
    pub fn planned(&self, op: &str) -> Option<Progress> {
        if !self.started.is_empty() {
            return None;
        }
        let m = self.total?;
        let plan = self.plan.clone().unwrap_or_default();
        Some(Progress {
            op: op.to_string(),
            step: String::new(),
            n: 0,
            m: Some(m),
            finished: false,
            changed: false,
            expected_step_s: None,
            expected_total_s: plan.total_s,
            expected_total_min_s: plan.total_min_s,
            expected_total_max_s: plan.total_max_s,
            expected_remaining_s: plan.total_s,
            elapsed_s: 0,
            runs: plan.runs,
        })
    }

    /// A mark as it arrived, with the time the host put on its line.
    pub fn on_mark(&mut self, mark: &StepMark, ts: u64) -> Progress {
        self.learn(&mark.op);
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
            expected_total_min_s: plan.total_min_s,
            expected_total_max_s: plan.total_max_s,
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

/// one-process-batch-2 (Kenny, 2026-10-23): what the dashboard has to read
/// besides the commands themselves to name the steps of the commands whose
/// plan depends on something the command does not carry. Each is what the
/// host will itself use at run time (its stored native manifests are what
/// the stack's files declare, its patch targets are its managed stacks, its
/// jobs and devices come from host.toml), read once by the shell before the
/// plan is made. A fact that was not read leaves its commands estimated, and
/// they are named so.
#[derive(Debug, Clone, Default)]
pub struct PlanFacts {
    /// Per stack: the native services its files declare, in the order the
    /// host walks them (BackupNative, UpdateNative, ReleaseUpdateNative,
    /// RestoreNative, VerifyRestore of a native stack).
    pub natives: BTreeMap<String, Vec<homelab_proto::NativeServiceManifest>>,
    /// Patch targets: the host's managed stacks, from the newest fleet
    /// reading.
    pub fleet_stacks: Option<Vec<String>>,
    /// `zfs_jobs` of host.toml.
    pub zfs_jobs: Option<Vec<homelab_core::ops::zfs::ZfsJob>>,
    /// The names of the `device_backups` host.toml lists, in order.
    pub device_backups: Option<Vec<String>>,
    /// Per stack: every restic repository it keeps (owner groups of a
    /// compose stack's manifest, backed-up units of a native one), for
    /// VerifyRestore.
    pub repositories: BTreeMap<String, Vec<String>>,
}

/// Which facts a list of commands needs read: the stacks whose native
/// services, the stacks whose repositories, and whether the fleet, the
/// zfs jobs and the device list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactsNeeded {
    pub natives: Vec<String>,
    pub repositories: Vec<String>,
    pub fleet: bool,
    pub zfs_jobs: bool,
    pub devices: bool,
}

/// What `commands` need read before their plan can be named exactly.
pub fn facts_needed(commands: &[homelab_proto::Command]) -> FactsNeeded {
    use homelab_proto::Command;
    let mut n = FactsNeeded::default();
    let push = |v: &mut Vec<String>, s: &str| {
        if !v.iter().any(|x| x == s) {
            v.push(s.to_string());
        }
    };
    for c in commands {
        match c {
            Command::BackupNative { stack }
            | Command::UpdateNative { stack }
            | Command::ReleaseUpdateNative { stack }
            | Command::RestoreNative { stack, .. } => push(&mut n.natives, stack),
            Command::VerifyRestore { stack, .. } => push(&mut n.repositories, stack),
            Command::PatchFleet => n.fleet = true,
            Command::ZfsReplicate => n.zfs_jobs = true,
            Command::BackupDevices => n.devices = true,
            _ => {}
        }
    }
    n
}

/// One command's steps: the stack they belong to and, for a command the host
/// fans out over a stack's native services, the unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPart {
    pub stack: String,
    pub unit: Option<String>,
    pub names: Vec<String>,
}

/// The step plan of a list of commands: its parts in the order they run, and
/// the commands whose steps could NOT be named up front (each with the
/// reason), so a caller can say exactly what stays an estimate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandsPlan {
    pub parts: Vec<PlanPart>,
    pub estimated: Vec<String>,
}

impl CommandsPlan {
    /// The step names, in order, unqualified.
    pub fn names(&self) -> Vec<String> {
        self.parts
            .iter()
            .flat_map(|p| p.names.iter().cloned())
            .collect()
    }

    /// Every step named, no command left to estimate.
    pub fn exact(&self) -> bool {
        self.estimated.is_empty()
    }

    /// Whether the host will announce more than one plan for these commands
    /// (several commands, or one fanned out over a stack's native units):
    /// then the total is the sum, fixed, rather than the one announcement.
    pub fn multi(&self) -> bool {
        self.parts.len() > 1
    }

    /// The names for a page that shows one list for the whole job: each step
    /// carries the stack it belongs to ("kitchen · provision container") when
    /// the job holds steps of more than one stack, and the unit when a
    /// stack's services each run the same steps.
    pub fn labeled(&self) -> Vec<String> {
        let stacks: std::collections::BTreeSet<&str> =
            self.parts.iter().map(|p| p.stack.as_str()).collect();
        let by_stack = stacks.len() > 1;
        self.parts
            .iter()
            .flat_map(|p| {
                let unit_of_many = p.unit.is_some()
                    && self
                        .parts
                        .iter()
                        .filter(|q| q.stack == p.stack && q.unit.is_some())
                        .count()
                        > 1;
                let by_stack = by_stack && !p.stack.is_empty();
                let unit_of_many = unit_of_many
                    || (p.stack.is_empty()
                        && p.unit.is_some()
                        && self.parts.iter().filter(|q| q.stack.is_empty()).count() > 1);
                let prefix = match (by_stack, unit_of_many) {
                    (true, true) => format!("{} · {} · ", p.stack, p.unit.as_deref().unwrap_or("")),
                    (true, false) => format!("{} · ", p.stack),
                    (false, true) => format!("{} · ", p.unit.as_deref().unwrap_or("")),
                    (false, false) => String::new(),
                };
                p.names.iter().map(move |n| format!("{prefix}{n}"))
            })
            .collect()
    }
}

/// The stack a command works on, where it names one.
pub fn command_stack(c: &homelab_proto::Command) -> Option<String> {
    use homelab_proto::Command::*;
    match c {
        DeployStack(spec) => Some(spec.manifest.stack_name.clone()),
        DestroyStack { manifest, .. }
        | RestoreStack { manifest, .. }
        | UpdateStack { manifest, .. } => Some(manifest.stack_name.clone()),
        BackupStack(m) | ApplyResources(m) => Some(m.stack_name.clone()),
        AdoptService(m) => Some(m.stack_name.clone()),
        InstallNative { manifest, .. } | InstallNativeRelease { manifest, .. } => {
            Some(manifest.stack_name.clone())
        }
        DestroyRecorded { stack, .. }
        | ForgetStack { stack }
        | BackupNative { stack }
        | UpdateNative { stack }
        | ReleaseUpdateNative { stack }
        | RollbackNative { stack, .. }
        | RestoreNative { stack, .. }
        | VerifyRestore { stack, .. }
        | SetStackEnabled { stack, .. }
        | SealEnv { stack } => Some(stack.clone()),
        WipeRetired { name, .. } => Some(name.clone()),
        _ => None,
    }
}

/// The steps one command reports, named from its own inputs and `facts`;
/// `Err(why)` when they depend on something that was not read. A command that
/// runs no steps at all (a read, an exec, a secret) is `Ok` with no parts.
fn command_parts(c: &homelab_proto::Command, facts: &PlanFacts) -> Result<Vec<PlanPart>, String> {
    use homelab_core::ops;
    use homelab_proto::Command;
    let own = |names: Vec<String>| -> Vec<PlanPart> {
        vec![PlanPart {
            stack: command_stack(c).unwrap_or_default(),
            unit: None,
            names,
        }]
    };
    let strs = |v: &[&str]| -> Vec<String> { v.iter().map(|s| s.to_string()).collect() };
    // The stack's native services as its files declare them, per unit.
    let natives =
        |stack: &str, what: &str| -> Result<&Vec<homelab_proto::NativeServiceManifest>, String> {
            facts.natives.get(stack).ok_or_else(|| {
                format!("{what} {stack}: the stack's native services were not read from its files")
            })
        };
    let per_unit = |stack: &str,
                    ms: &[homelab_proto::NativeServiceManifest],
                    f: &dyn Fn(&homelab_proto::NativeServiceManifest) -> Vec<String>|
     -> Vec<PlanPart> {
        ms.iter()
            .filter_map(|m| {
                let names = f(m);
                (!names.is_empty()).then(|| PlanPart {
                    stack: stack.to_string(),
                    unit: Some(m.unit.clone()),
                    names,
                })
            })
            .collect()
    };
    Ok(match c {
        Command::DeployStack(spec) => own(ops::deploy::plan_names(spec)),
        Command::DestroyStack { manifest, .. } => {
            own(ops::destroy::destroy_plan_names(&manifest.stack_name))
        }
        Command::DestroyRecorded { stack, .. } => own(ops::destroy::destroy_plan_names(stack)),
        Command::ForgetStack { .. } => own(ops::destroy::forget_plan_names()),
        Command::BackupStack(_) => own(strs(ops::backup::BACKUP_STEPS)),
        Command::RestoreStack {
            skip_safety_copy, ..
        } => own(ops::backup::restore_plan_names(!skip_safety_copy)),
        Command::UpdateStack { manifest, app } => own(ops::update::update_plan_names(
            manifest,
            app.as_deref(),
            false,
        )),
        Command::ApplyResources(_) => own(strs(ops::resize::RESIZE_STEPS)),
        Command::SetStackEnabled { .. } => own(strs(ops::enable::SET_ENABLED_STEPS)),
        Command::SealEnv { .. } => own(strs(ops::sealenv::SEAL_ENV_STEPS)),
        Command::BackupHostMeta => own(strs(ops::backup::HOST_META_BACKUP_STEPS)),
        Command::AdoptService(_) => own(strs(ops::native::ADOPT_STEPS)),
        Command::InstallNative { manifest, .. } => {
            own(ops::native::install_plan_names(manifest, None))
        }
        Command::InstallNativeRelease { manifest, tag, .. } => {
            // `install_release`: a pinned tag, the unit file sent with it.
            own(ops::native::release_install_plan_names(
                manifest,
                Some(tag),
                Some(""),
            ))
        }
        Command::RollbackNative { .. } => own(strs(ops::native::ROLLBACK_NATIVE_STEPS)),
        Command::BackupNative { stack } => per_unit(
            stack,
            natives(stack, "backup-native")?,
            &ops::native::backup_native_plan_names,
        ),
        Command::UpdateNative { stack } => per_unit(
            stack,
            natives(stack, "update-native")?,
            &ops::native::update_native_plan_names,
        ),
        Command::ReleaseUpdateNative { stack } => {
            per_unit(stack, natives(stack, "release-update-native")?, &|m| {
                ops::native::release_install_plan_names(m, None, None)
            })
        }
        Command::RestoreNative { stack, unit, .. } => {
            let ms = natives(stack, "restore-native")?;
            let picked: Vec<_> = ms
                .iter()
                .filter(|m| unit.as_deref().is_none_or(|u| u == m.unit))
                .cloned()
                .collect();
            per_unit(stack, &picked, &ops::native::restore_native_plan_names)
        }
        Command::VerifyRestore {
            stack,
            app,
            snapshot,
            ..
        } => {
            let repos = facts.repositories.get(stack).ok_or_else(|| {
                format!("verify-restore {stack}: the stack's repositories were not read")
            })?;
            // A request the host refuses reports no steps at all.
            let owners = ops::restoredrill::select_verify_owners(
                repos.clone(),
                stack,
                app.as_deref(),
                snapshot,
            )
            .unwrap_or_default();
            if owners.is_empty() {
                Vec::new()
            } else {
                own(ops::restoredrill::verify_restore_plan_names(&owners))
            }
        }
        Command::PatchFleet => {
            let stacks = facts
                .fleet_stacks
                .as_ref()
                .ok_or("patch: the host's managed stacks were not read")?;
            if stacks.is_empty() {
                Vec::new()
            } else {
                own(ops::patch::patch_plan_names(stacks))
            }
        }
        Command::ZfsReplicate => {
            let jobs = facts
                .zfs_jobs
                .as_ref()
                .ok_or("zfs-replicate: the host's zfs_jobs were not read")?;
            own(ops::zfs::replicate_plan_names(jobs))
        }
        Command::BackupDevices => {
            let devices = facts
                .device_backups
                .as_ref()
                .ok_or("backup-devices: the host's device_backups were not read")?;
            // The host runs one `device-backup` operation per device, each
            // announcing its own plan.
            devices
                .iter()
                .map(|d| PlanPart {
                    stack: String::new(),
                    unit: Some(d.clone()),
                    names: ops::devicebackup::backup_devices_plan_names(1),
                })
                .collect()
        }
        Command::ApplyGuards { .. } => own(strs(ops::guards::APPLY_GUARDS_STEPS)),
        Command::WipeRetired {
            confirm: Some(_), ..
        } => own(strs(ops::retired::WIPE_STEPS)),
        Command::BuildTemplate { .. } => own(strs(ops::template::TEMPLATE_BUILD_STEPS)),
        Command::SelfUpdateHost { .. } => own(strs(ops::selfupdate::SELF_UPDATE_STEPS)),
        Command::RestartHost => own(strs(ops::restarthost::RESTART_HOST_STEPS)),
        // Everything else runs no `Runner` steps: reads, an exec, a secret,
        // a staged binary, a prune of orphan files, an answer.
        _ => Vec::new(),
    })
}

/// The step plan of `commands`, named exactly wherever it can be, and which
/// commands cannot be (`estimated`).
pub fn commands_plan_with(commands: &[homelab_proto::Command], facts: &PlanFacts) -> CommandsPlan {
    let mut plan = CommandsPlan::default();
    for c in commands {
        match command_parts(c, facts) {
            Ok(parts) => plan
                .parts
                .extend(parts.into_iter().filter(|p| !p.names.is_empty())),
            Err(why) => plan.estimated.push(why),
        }
    }
    plan
}

/// one-process-batch-1 (Kenny, 2026-10-23: "van begin tot einde moet dat 1
/// proces zijn, met een op voorhand bekend aantal stappen"): the step names
/// the commands of ONE action announce, in order, as far as the commands
/// alone say (no facts read). The one source `execute`'s multi-command total
/// and a batch's up-front plan both sum, so the two cannot drift.
pub fn commands_plan(commands: &[homelab_proto::Command]) -> Vec<String> {
    commands_plan_with(commands, &PlanFacts::default()).names()
}

/// One member of a batch in the batch's single step plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemberPlan {
    pub job: u64,
    pub stack: String,
    /// The member's steps in order, unqualified (the page prefixes the
    /// stack: "kitchen · provision container").
    pub steps: Vec<String>,
    /// True when `steps` is what the member's commands announce, exactly;
    /// false when it is an estimate from the newest successful run of the
    /// same operation (or empty when there is none): the member's own count
    /// is only certain once the host announces it. one-process-batch-2: only
    /// a command whose inputs could not be read is still estimated, and
    /// `estimated_because` says which and why.
    pub exact: bool,
    /// Why the member is an estimate; empty when it is exact.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub estimated_because: Vec<String>,
}

/// A member's plan: from its commands when they can be named exactly, else
/// from the history of the same operation (`op`, e.g. `update-kitchen`),
/// else empty.
pub fn member_plan(
    job: u64,
    stack: &str,
    commands: Option<&[homelab_proto::Command]>,
    history: &[HistoryEntry],
    op: &str,
) -> MemberPlan {
    member_plan_with(job, stack, commands, &PlanFacts::default(), history, op)
}

/// [`member_plan`] with the facts the shell read (`facts_needed`).
pub fn member_plan_with(
    job: u64,
    stack: &str,
    commands: Option<&[homelab_proto::Command]>,
    facts: &PlanFacts,
    history: &[HistoryEntry],
    op: &str,
) -> MemberPlan {
    let Some(commands) = commands else {
        return estimated_member(
            job,
            stack,
            history,
            op,
            vec!["the stack's files could not be read for its commands".to_string()],
        );
    };
    let plan = commands_plan_with(commands, facts);
    if plan.exact() {
        return MemberPlan {
            job,
            stack: stack.to_string(),
            steps: plan.names(),
            exact: true,
            estimated_because: Vec::new(),
        };
    }
    estimated_member(job, stack, history, op, plan.estimated)
}

fn estimated_member(
    job: u64,
    stack: &str,
    history: &[HistoryEntry],
    op: &str,
    because: Vec<String>,
) -> MemberPlan {
    let guess = expectation(history, op)
        .steps
        .into_iter()
        .map(|s| s.step)
        .collect();
    MemberPlan {
        job,
        stack: stack.to_string(),
        steps: guess,
        exact: false,
        estimated_because: because,
    }
}

/// The batch's step total: the sum of every member's plan, and whether all
/// of them are exact (false: the page says "about N steps").
pub fn batch_total(members: &[MemberPlan]) -> (usize, bool) {
    (
        members.iter().map(|m| m.steps.len()).sum(),
        members.iter().all(|m| m.exact),
    )
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

    /// steps-total-known-1: before the first mark the tracker already says
    /// "step 0 of m", for a fixed total and for a single command's plan.
    #[test]
    fn planned_gives_step_zero_of_the_total_before_the_first_mark() {
        let mut t = Tracker::new(Vec::new());
        assert!(t.planned("deploy-media").is_none(), "no total yet");
        t.set_total(56);
        let p = t.planned("deploy-media").expect("a fixed total is known");
        assert_eq!((p.n, p.m, p.finished), (0, Some(56), false));
        assert_eq!(p.step, "");
        // A single command's own plan sets the total the same way.
        let mut one = Tracker::new(Vec::new());
        one.on_plan("deploy-media", &plan_names(28, "step"));
        assert_eq!(one.planned("deploy-media").map(|p| p.m), Some(Some(28)));
        // Once a mark arrived the marks say it.
        one.on_mark(&mark("deploy-media", "step-0", false), 1);
        assert!(one.planned("deploy-media").is_none());
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

#[cfg(test)]
mod plausible_tests {
    use super::{PLAUSIBLE_MAX_S, median, plausible};

    #[test]
    fn median_outlier_1_five_days_among_five_second_deploys_is_left_out() {
        let five_days = 5 * 86_400;
        let kept = plausible(&[5, 6, 5, five_days]);
        assert_eq!(kept, vec![5, 6, 5]);
        assert_eq!(median(&kept), Some(5));
    }

    #[test]
    fn median_outlier_1_a_slow_run_among_a_few_stays_unless_far_out() {
        assert_eq!(plausible(&[60, 80, 70, 300]), vec![60, 80, 70, 300]);
        assert_eq!(plausible(&[5, 6, 7, 6 * 20 + 1]), vec![5, 6, 7]);
    }

    #[test]
    fn median_outlier_1_one_long_run_alone_is_not_a_duration() {
        assert!(plausible(&[PLAUSIBLE_MAX_S + 1]).is_empty());
        assert_eq!(plausible(&[90]), vec![90]);
    }
}
