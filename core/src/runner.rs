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

    pub fn finish_ok(self) -> OperationReport {
        self.journal.record(&self.op, "-", "complete");
        OperationReport {
            op: self.op,
            steps: self.steps,
            ok: true,
            error: None,
        }
    }

    pub fn finish_err(self, step: &str, err: &CoreError) -> OperationReport {
        self.journal.record(&self.op, "-", "failed");
        OperationReport {
            op: self.op,
            steps: self.steps,
            ok: false,
            error: Some(OperatorError::from_core(step, err)),
        }
    }
}
