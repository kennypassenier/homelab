//! The operation runner (AR3): every operation is a sequence of named steps
//! executed through this one place, which uniformly provides transcripts
//! (F2), journal records (B5), fail-closed semantics (A3) and the report the
//! TUI renders.

use crate::error::{CoreError, OperatorError};
use crate::sink::{Level, PipelineEvent, Sink};

/// Journal hook (B5): phase-by-phase records of every operation, written
/// BEFORE each step runs so an interrupted operation is visible (AR13).
pub trait Journal: Send + Sync {
    fn record(&self, op: &str, step: &str, status: &str);
}

pub struct NullJournal;
impl Journal for NullJournal {
    fn record(&self, _op: &str, _step: &str, _status: &str) {}
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StepReport {
    pub name: String,
    pub changed: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OperationReport {
    pub op: String,
    pub steps: Vec<StepReport>,
    pub ok: bool,
    pub error: Option<OperatorError>,
    /// Set when the operation deliberately did not run (`CoreError::Deferred`).
    /// `ok` is false — nothing happened — but a caller that treats every
    /// `!ok` as a fault would be wrong here, so it can tell the two apart.
    /// `serde(default)` keeps an older client able to read a newer report.
    #[serde(default)]
    pub deferred: Option<String>,
}

/// Outcome of a step body: did it change anything? (Feeds B1's
/// "second run is quiet" property and the TUI's changed/unchanged display.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    Changed,
    Unchanged,
}

pub struct Runner<'a> {
    pub op: String,
    pub sink: &'a dyn Sink,
    pub journal: &'a dyn Journal,
    steps: Vec<StepReport>,
    /// fix-171 round 3 ("make it impossible to forget"): set by `plan()`,
    /// checked by `step()` and `skip()`. A mark emitted before the plan is
    /// announced is exactly the bug this whole fix exists to prevent — the
    /// client's `m` would start `None` and climb — so in debug/test builds
    /// it panics right here, at the mark that forgot to plan, rather than
    /// surviving to be caught later (if at all) by a test on the client
    /// side reading the wire. Release builds stay fail-open (a missed plan
    /// degrades the step counter; it must never crash the host).
    planned: bool,
    /// fix-step-plan-nested: the announced plan's own names, in order — kept
    /// so every later mark can be checked against it (a mark beyond `m`, or
    /// one whose name was never announced, is what the invariant below
    /// catches).
    plan_names: Vec<String>,
    /// How many marks (run or skip) this run has made so far — `n`, as the
    /// client sees it.
    marks: usize,
    /// fix-step-plan-nested (Kenny via the coordinator, 2026-10-02): a plan
    /// violation must never abort a running op — a mutating step that
    /// already ran cannot be half-undone by panicking on the NEXT step's
    /// mark. So in a release build this is recorded rather than panicked:
    /// the violation is logged at error level immediately (op, step, n/m),
    /// every remaining step still runs exactly as it would have, and
    /// `finish_ok` downgrades the final report to a failure carrying this
    /// text — the same `ok: false` shape every other op failure already
    /// takes, which is what gets this in front of a human (incident bundle,
    /// job status) without inventing a second failure channel. In a
    /// debug/test build it panics immediately instead: the bug is caught at
    /// the mark that caused it, in the test that exercises that op, rather
    /// than shipped to be found by a client months later the way the LIVE
    /// kyu counter was.
    plan_violation: Option<String>,
}

impl<'a> Runner<'a> {
    pub fn new(op: &str, sink: &'a dyn Sink, journal: &'a dyn Journal) -> Self {
        Self {
            op: op.to_string(),
            sink,
            journal,
            steps: Vec::new(),
            planned: false,
            plan_names: Vec::new(),
            marks: 0,
            plan_violation: None,
        }
    }

    /// fix-step-plan-nested: called by both `step` and `skip` before they
    /// emit their mark. See `plan_violation`'s own doc for why a release
    /// build records rather than panics.
    fn check_mark(&mut self, name: &str) {
        let in_plan = self.plan_names.iter().any(|s| s == name);
        let beyond = self.marks >= self.plan_names.len();
        if self.plan_violation.is_none() && (!in_plan || beyond) {
            let msg = format!(
                "plan violation in op '{}': mark '{}' at n={} m={} ({})",
                self.op,
                name,
                self.marks + 1,
                self.plan_names.len(),
                if !in_plan {
                    "this name was never in the announced plan"
                } else {
                    "this mark is beyond the announced plan's length"
                }
            );
            if cfg!(debug_assertions) {
                panic!("{msg}");
            }
            self.log(Level::Error, msg.clone());
            self.plan_violation = Some(msg);
        }
        self.marks += 1;
    }

    pub fn log(&self, level: Level, msg: impl Into<String>) {
        self.sink.emit(PipelineEvent::Line {
            level,
            source: "HOST".into(),
            msg: msg.into(),
        });
    }

    /// fix-171 round 2: announce the ordered, complete list of steps this
    /// run plans to mark — every one it might run OR skip — before the
    /// first one starts. A client's "step n/m" becomes `m` = this list's
    /// length, fixed from the very first mark, because it is the op's own
    /// plan for the run under way rather than a guess from a past run's
    /// count. A step named here that this run's inputs say will not run
    /// must still be marked, with `skip` (below), so `n` reaches `m`
    /// exactly when the operation finishes.
    pub fn plan<S: AsRef<str>>(&mut self, steps: &[S]) {
        self.planned = true;
        self.plan_names = steps.iter().map(|s| s.as_ref().to_string()).collect();
        self.sink.emit(PipelineEvent::Plan {
            op: self.op.clone(),
            steps: self.plan_names.clone(),
        });
    }

    /// fix-171 round 2: fill one slot of the announced plan with a skip —
    /// this step's precondition did not hold this run (no firewall
    /// declared, an app that failed its policy gate, a route nothing
    /// retires). One mark, not a start/finish pair, matching `plan`'s
    /// count 1-for-1 whether the step ran or not.
    pub fn skip(&mut self, name: &str) {
        debug_assert!(
            self.planned,
            "[{}] skip(\"{}\") before plan() — every step mark must be preceded by this op's \
             own announced plan (fix-171)",
            self.op, name
        );
        self.check_mark(name);
        self.journal.record(&self.op, name, "skipped");
        self.sink.emit(PipelineEvent::StepSkipped {
            op: self.op.clone(),
            step: name.to_string(),
        });
    }

    /// Run one named step. The journal sees "running" before the body starts
    /// and "done"/"failed" after — an interrupt leaves a visible "running"
    /// record behind (AR13).
    pub async fn step<F, Fut>(&mut self, name: &str, body: F) -> Result<StepOutcome, CoreError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<StepOutcome, CoreError>>,
    {
        debug_assert!(
            self.planned,
            "[{}] step(\"{}\") before plan() — every step mark must be preceded by this op's \
             own announced plan (fix-171)",
            self.op, name
        );
        self.check_mark(name);
        self.journal.record(&self.op, name, "running");
        self.sink.emit(PipelineEvent::StepStarted {
            op: self.op.clone(),
            step: name.to_string(),
        });
        match body().await {
            Ok(outcome) => {
                self.journal.record(&self.op, name, "done");
                self.sink.emit(PipelineEvent::StepFinished {
                    op: self.op.clone(),
                    step: name.to_string(),
                    changed: outcome == StepOutcome::Changed,
                });
                self.steps.push(StepReport {
                    name: name.to_string(),
                    changed: outcome == StepOutcome::Changed,
                });
                Ok(outcome)
            }
            Err(err) => {
                self.journal.record(&self.op, name, "failed");
                self.log(Level::Error, format!("[{}] {}", name, err));
                Err(err)
            }
        }
    }

    /// S2 · a step that checks its own work.
    ///
    /// `body` does the thing; `verify` then reads the world back and answers
    /// whether it is actually so. A verify that says no fails the step, with
    /// wording that names the real problem — the command succeeded and the
    /// change is not there.
    ///
    /// This exists because that combination is the single most common shape
    /// of defect in this project. Three from one evening: the host dropped a
    /// manifest field it did not recognise and reported a clean deploy while
    /// the container came up with no disks; a file was written over a running
    /// program's own binary and the transcript said "pushed"; and promtail
    /// ran for months shipping nothing while every check called it healthy.
    /// An exit code of zero answers "did the command run", which is a
    /// different question from "is it now true".
    pub async fn step_verified<F, Fut, V, VFut>(
        &mut self,
        name: &str,
        body: F,
        verify: V,
    ) -> Result<StepOutcome, CoreError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<StepOutcome, CoreError>>,
        V: FnOnce() -> VFut,
        VFut: std::future::Future<Output = Result<(), String>>,
    {
        let outcome = self.step(name, body).await?;
        // Nothing changed means nothing to check: re-reading the world to
        // confirm an absence of work costs a command per idempotent step,
        // and every deploy is mostly idempotent steps.
        if outcome == StepOutcome::Unchanged {
            return Ok(outcome);
        }
        match verify().await {
            Ok(()) => Ok(outcome),
            Err(why) => {
                self.journal.record(&self.op, name, "unverified");
                let err = CoreError::Command {
                    rendered: format!("verify {}", name),
                    detail: format!(
                        "the step reported success but the change is not there: {}",
                        why
                    ),
                };
                self.log(Level::Error, format!("[{}] {}", name, err));
                Err(err)
            }
        }
    }

    pub fn finish_ok(self) -> OperationReport {
        self.journal.record(&self.op, "-", "complete");
        // fix-step-plan-nested: every step this run marked already executed
        // (or was legitimately skipped) exactly as it would have — a plan
        // violation never aborted it mid-flight. What changes here is only
        // whether the FINISHED report counts as a success: a violation means
        // the client's step counter cannot be trusted for this run, which is
        // a bug worth a human's attention the same way any other op failure
        // is (incident bundle, job status) — "ran fine" would hide it.
        if let Some(violation) = self.plan_violation {
            return OperationReport {
                op: self.op,
                steps: self.steps,
                ok: false,
                error: Some(OperatorError {
                    what: "plan violation".to_string(),
                    why: violation,
                    remedy: "every mutating step already ran to completion; this is a counting \
                             bug in the op's own announced plan, not damage — file it against \
                             the op named above"
                        .to_string(),
                }),
                deferred: None,
            };
        }
        OperationReport {
            op: self.op,
            steps: self.steps,
            ok: true,
            error: None,
            deferred: None,
        }
    }

    pub fn finish_err(self, step: &str, err: &CoreError) -> OperationReport {
        self.journal.record(
            &self.op,
            "-",
            match err {
                CoreError::Deferred(_) => "deferred",
                _ => "failed",
            },
        );
        OperationReport {
            op: self.op,
            steps: self.steps,
            ok: false,
            error: Some(OperatorError::from_core(step, err)),
            deferred: match err {
                CoreError::Deferred(why) => Some(why.clone()),
                _ => None,
            },
        }
    }
}

/// fix-step-plan-nested (round 4 of fix-171; LIVE: 2026-10-02, "step x/6
/// with x past 10; kyu 28/18 and higher"): an op whose own steps call
/// ANOTHER op's function used to let that other op start its own `Runner`
/// and announce its own plan — `release_update` (6 steps) called
/// `install_native` (9 steps) called `adopt` (8 steps), each one a separate
/// `PipelineEvent::Plan` under its own op id, so the client's running total
/// for one unit climbed from 6 to 23 mid-flight and the admin batch sum
/// (`RELEASE_UPDATE_STEPS.len() * units`) never matched what actually ran.
///
/// `Scope` is what a nestable op takes instead of owning a `Runner`
/// outright. The outermost call is `Scope::Top` — it owns the `Runner`,
/// announces the plan and produces the `OperationReport`. A call made FROM
/// inside another op's step is `Scope::Nested` — every mark it makes lands
/// on the PARENT's `Runner`, qualified with this op's own name (e.g.
/// `"install-kyu :: activate"`), so the parent's single announced plan
/// already lists these names and the shared `n` never resets or restarts.
/// Nesting is flattened, not compounded: `Scope::child` always reaches
/// through to the one underlying `Runner`, so a 3-deep call chain produces
/// sibling-qualified names (`"install-kyu :: …"`, `"adopt-kyu :: …"`), never
/// `"install-kyu :: adopt-kyu :: …"`.
pub enum Scope<'r, 'a> {
    Top(Runner<'a>),
    Nested {
        parent: &'r mut Runner<'a>,
        prefix: String,
    },
}

/// A step's failure, carrying the ALREADY-QUALIFIED name it should be
/// reported under (prefixed when it happened inside a nested scope) — the
/// only thing an `_impl` function hands back on error, since only the
/// outermost `Scope::Top` owns a `Runner` to finish a report with.
#[derive(Debug)]
pub struct StepFailure {
    pub step: String,
    pub err: CoreError,
}

impl<'r, 'a> Scope<'r, 'a> {
    /// Start a fresh, top-level scope: this call owns its own `Runner` and
    /// will announce its own plan via `plan_if_top`. `'r` is never used by
    /// `Top` — it is only ever constrained by a later `.child()` call, so
    /// leaving it generic here lets the same function produce whatever
    /// `Scope<'r, 'a>` the caller's context needs.
    pub fn top(op: &str, sink: &'a dyn Sink, journal: &'a dyn Journal) -> Self {
        Scope::Top(Runner::new(op, sink, journal))
    }

    fn runner_mut(&mut self) -> &mut Runner<'a> {
        match self {
            Scope::Top(r) => r,
            Scope::Nested { parent, .. } => parent,
        }
    }

    /// A nested scope for a call this op makes to another op's `_impl`,
    /// sharing the SAME underlying `Runner` this scope already uses
    /// (flattened — see the type's own doc comment).
    pub fn child(&mut self, prefix: impl Into<String>) -> Scope<'_, 'a> {
        Scope::Nested {
            parent: self.runner_mut(),
            prefix: prefix.into(),
        }
    }

    /// The name a mark through this scope is actually recorded under:
    /// unchanged at the top, `"<prefix> :: <name>"` when nested.
    pub fn qualify(&self, name: &str) -> String {
        match self {
            Scope::Top(_) => name.to_string(),
            Scope::Nested { prefix, .. } => format!("{} :: {}", prefix, name),
        }
    }

    /// Announce the plan — only when this scope is the outermost call. A
    /// nested scope must never call this: its names are already part of the
    /// parent's own announced plan (composed by the parent before its first
    /// step runs).
    pub fn plan_if_top<S: AsRef<str>>(&mut self, steps: &[S]) {
        if let Scope::Top(r) = self {
            r.plan(steps);
        }
    }

    pub fn log(&self, level: Level, msg: impl Into<String>) {
        match self {
            Scope::Top(r) => r.log(level, msg),
            Scope::Nested { parent, .. } => parent.log(level, msg),
        }
    }

    pub fn skip(&mut self, name: &str) {
        let qualified = self.qualify(name);
        self.runner_mut().skip(&qualified);
    }

    pub async fn step<F, Fut>(&mut self, name: &str, body: F) -> Result<StepOutcome, CoreError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<StepOutcome, CoreError>>,
    {
        let qualified = self.qualify(name);
        self.runner_mut().step(&qualified, body).await
    }

    /// Only valid on `Scope::Top`: consumes it into the finished report.
    /// Every nested call instead returns its `Result<(), StepFailure>` to
    /// its own caller, up to whichever scope IS the top.
    pub fn finish(self, result: Result<(), StepFailure>) -> OperationReport {
        match self {
            Scope::Top(r) => match result {
                Ok(()) => r.finish_ok(),
                Err(f) => r.finish_err(&f.step, &f.err),
            },
            Scope::Nested { .. } => {
                unreachable!("Scope::finish called on a Nested scope — only Top produces a report")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::VecSink;

    /// fix-171 round 3 ("make it impossible to forget"): a step mark before
    /// the plan is announced panics in a debug/test build — the guard that
    /// backs every op's own test in `core/tests/step_plan_tests.rs` and the
    /// per-op tests across `core/tests/*`, so a NEW op that forgets to call
    /// `plan()` fails the moment its own test runs it, rather than shipping
    /// a step counter that starts at `None` and climbs.
    #[test]
    #[should_panic(expected = "before plan()")]
    fn a_step_mark_before_plan_panics() {
        let sink = VecSink::new();
        let journal = NullJournal;
        let mut r = Runner::new("forgot-the-plan", &sink, &journal);
        r.skip("whatever");
    }

    /// The positive twin: a plan announced first never panics.
    #[test]
    fn a_step_mark_after_plan_does_not_panic() {
        let sink = VecSink::new();
        let journal = NullJournal;
        let mut r = Runner::new("remembered-the-plan", &sink, &journal);
        r.plan(&["whatever"]);
        r.skip("whatever");
    }

    /// fix-step-plan-nested (LIVE 2026-10-02, "step x/6 with x past 10"): a
    /// mark beyond the announced plan's length panics in a debug/test build
    /// — this is the invariant that would have caught the kyu fault inside
    /// its own test, rather than letting `n` climb past `m` on the wire.
    #[test]
    #[should_panic(expected = "plan violation")]
    fn a_mark_beyond_the_plan_panics_in_debug() {
        let sink = VecSink::new();
        let journal = NullJournal;
        let mut r = Runner::new("over-marked", &sink, &journal);
        r.plan(&["only-step"]);
        r.skip("only-step");
        r.skip("a-second-mark-nobody-announced");
    }

    /// The twin for a mark whose name the plan never listed at all (even
    /// as mark 1 of 1) — also a violation, not just running out of slots.
    #[test]
    #[should_panic(expected = "plan violation")]
    fn a_mark_whose_name_was_never_planned_panics_in_debug() {
        let sink = VecSink::new();
        let journal = NullJournal;
        let mut r = Runner::new("wrong-name", &sink, &journal);
        r.plan(&["only-step"]);
        r.skip("a-name-that-was-never-planned");
    }

    /// fix-step-plan-nested: debug/test builds panic (above); this test
    /// proves the NON-panicking shape the coordinator decided on for a
    /// release build directly against the production-path method, by
    /// checking what `finish_ok` would do once a violation is recorded —
    /// `cfg!(debug_assertions)` makes the panic branch untestable here, so
    /// this drives `plan_violation` by hand the way a release build's
    /// `check_mark` would have set it, and asserts the SAME downgrade:
    /// `ok: false`, a `plan violation` error, no panic, no aborted op.
    #[test]
    fn finish_ok_downgrades_to_failed_when_a_violation_was_recorded() {
        let sink = VecSink::new();
        let journal = NullJournal;
        let mut r = Runner::new("release-shaped", &sink, &journal);
        r.plan(&["only-step"]);
        r.plan_violation = Some("plan violation in op 'release-shaped': test-injected".into());
        let report = r.finish_ok();
        assert!(!report.ok, "a recorded violation must never report success");
        let err = report.error.expect("a violation produces an error");
        assert_eq!(err.what, "plan violation");
        assert!(err.why.contains("test-injected"));
    }
}
