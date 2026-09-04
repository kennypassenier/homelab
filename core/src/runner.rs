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
}

impl<'a> Runner<'a> {
    pub fn new(op: &str, sink: &'a dyn Sink, journal: &'a dyn Journal) -> Self {
        Self {
            op: op.to_string(),
            sink,
            journal,
            steps: Vec::new(),
        }
    }

    pub fn log(&self, level: Level, msg: impl Into<String>) {
        self.sink.emit(PipelineEvent::Line {
            level,
            source: "HOST".into(),
            msg: msg.into(),
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
