//! Incident bundles (AR14): every failed operation captures its full context
//! into one directory — transcript, report, state snapshot, replayable
//! command script. The standing process: every bug becomes a MockExecutor
//! test scripted from a bundle.

use crate::error::CoreError;
use crate::executor::Executor;
use crate::runner::OperationReport;
use crate::sink::{PipelineEvent, Sink};

/// Tee: forwards every event to an inner sink AND records it for a possible
/// incident bundle.
pub struct RecordingSink<'a> {
    inner: &'a dyn Sink,
    events: std::sync::Mutex<Vec<PipelineEvent>>,
}

impl<'a> RecordingSink<'a> {
    pub fn new(inner: &'a dyn Sink) -> Self {
        Self {
            inner,
            events: std::sync::Mutex::new(Vec::new()),
        }
    }
    pub fn events(&self) -> Vec<PipelineEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl Sink for RecordingSink<'_> {
    fn emit(&self, event: PipelineEvent) {
        self.events.lock().unwrap().push(event.clone());
        self.inner.emit(event);
    }
}

/// Extract the literal executed commands ("[run ] …" transcript lines) as a
/// replayable shell script (AR16).
pub fn commands_script(events: &[PipelineEvent]) -> String {
    let mut out = String::from(
        "#!/bin/sh\n# Replay of the exact commands this operation ran (AR16).\n# Review before executing — this script mutates the host.\nset -x\n",
    );
    for ev in events {
        if let PipelineEvent::Line { msg, .. } = ev {
            if let Some(cmd) = msg.trim().strip_prefix("[run ] ") {
                out.push_str(cmd);
                out.push('\n');
            }
        }
    }
    out
}

/// Write a bundle under `<state_dir>/incidents/<ts>-<op>/`. Returns the
/// bundle directory path.
pub async fn write_bundle(
    exec: &dyn Executor,
    state_dir: &str,
    ts_unix: u64,
    report: &OperationReport,
    events: &[PipelineEvent],
    versions: &str,
) -> Result<String, CoreError> {
    let dir = format!("{}/incidents/{}-{}", state_dir, ts_unix, report.op);

    let report_json =
        serde_json::to_string_pretty(report).map_err(|e| CoreError::State(e.to_string()))?;
    exec.write_file(&format!("{}/report.json", dir), &report_json, 0o644)
        .await?;

    let mut events_jsonl = String::new();
    for ev in events {
        events_jsonl
            .push_str(&serde_json::to_string(ev).map_err(|e| CoreError::State(e.to_string()))?);
        events_jsonl.push('\n');
    }
    exec.write_file(&format!("{}/events.jsonl", dir), &events_jsonl, 0o644)
        .await?;

    exec.write_file(
        &format!("{}/commands.sh", dir),
        &commands_script(events),
        0o755,
    )
    .await?;

    if let Ok(state) = exec.read_file(&format!("{}/state.json", state_dir)).await {
        exec.write_file(&format!("{}/state-at-failure.json", dir), &state, 0o644)
            .await?;
    }
    if let Ok(journal) = exec
        .read_file(&format!("{}/journal.jsonl", state_dir))
        .await
    {
        let tail: String = journal
            .lines()
            .rev()
            .take(200)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        exec.write_file(&format!("{}/journal-tail.jsonl", dir), &tail, 0o644)
            .await?;
    }
    exec.write_file(&format!("{}/versions.txt", dir), versions, 0o644)
        .await?;
    Ok(dir)
}

// ── Interrupted-operation detection (AR13) ──────────────────────────────────

/// Parse journal JSONL content and report operations whose most recent record
/// is still "running" — i.e. the daemon died or was interrupted mid-step.
/// Re-running such an operation is always safe (B1 idempotency).
pub fn interrupted_ops(journal_content: &str) -> Vec<(String, String)> {
    use std::collections::BTreeMap;
    let mut last: BTreeMap<String, (String, String)> = BTreeMap::new();
    for line in journal_content.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let (Some(op), Some(step), Some(status)) = (
            v.get("op").and_then(|x| x.as_str()),
            v.get("step").and_then(|x| x.as_str()),
            v.get("status").and_then(|x| x.as_str()),
        ) else {
            continue;
        };
        last.insert(op.to_string(), (step.to_string(), status.to_string()));
    }
    last.into_iter()
        .filter(|(_, (_, status))| status == "running")
        .map(|(op, (step, _))| (op, step))
        .collect()
}
