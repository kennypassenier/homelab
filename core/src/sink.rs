//! Pipeline event sink: how operations talk to the outside world (F2 live
//! transcripts, G6 byte counters) without knowing who listens.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PipelineEvent {
    /// fix-171 round 2: the operation's own announcement of every step it
    /// plans to mark (run or skipped), emitted once, before its first
    /// `StepStarted` — so a listener's total is known and fixed from the
    /// very first step rather than guessed from a past run.
    Plan {
        op: String,
        steps: Vec<String>,
    },
    StepStarted {
        op: String,
        step: String,
    },
    StepFinished {
        op: String,
        step: String,
        changed: bool,
    },
    /// fix-171 round 2: a step the plan listed that this run did not take —
    /// its precondition did not hold. One event, not a start/finish pair,
    /// so it still fills exactly one slot of the announced plan.
    StepSkipped {
        op: String,
        step: String,
    },
    Line {
        level: Level,
        source: String,
        msg: String,
    },
    /// Real byte counters for G6 transfer visuals — never fake progress.
    Bytes {
        op: String,
        label: String,
        done: u64,
        total: Option<u64>,
    },
}

pub trait Sink: Send + Sync {
    fn emit(&self, event: PipelineEvent);
}

/// Discards everything (some CLI paths, tests that don't care).
pub struct NullSink;
impl Sink for NullSink {
    fn emit(&self, _event: PipelineEvent) {}
}

/// Collects everything (tests, incident bundles).
#[derive(Default)]
pub struct VecSink(std::sync::Mutex<Vec<PipelineEvent>>);

impl VecSink {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn events(&self) -> Vec<PipelineEvent> {
        self.0.lock().unwrap().clone()
    }
    pub fn lines(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| match e {
                PipelineEvent::Line { msg, .. } => Some(msg),
                _ => None,
            })
            .collect()
    }
}

impl Sink for VecSink {
    fn emit(&self, event: PipelineEvent) {
        self.0.lock().unwrap().push(event);
    }
}
