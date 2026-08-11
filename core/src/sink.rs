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
    StepStarted {
        op: String,
        step: String,
    },
    StepFinished {
        op: String,
        step: String,
        changed: bool,
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
