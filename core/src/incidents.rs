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
        "#!/bin/sh\n# Replay of the exact commands this operation ran.\n# Review before executing — this script mutates the host.\nset -x\n",
    );
    for ev in events {
        if let PipelineEvent::Line { msg, .. } = ev
            && let Some(cmd) = msg.trim().strip_prefix("[run ] ")
        {
            out.push_str(cmd);
            out.push('\n');
        }
    }
    out
}

/// fix-125: bundle files hold transcripts that can carry secrets, so they are
/// private rather than the default mode. Story: `docs/deployment/REGISTER.md`.
pub const PRIVATE: u32 = 0o600;

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
    exec.write_file(&format!("{}/report.json", dir), &report_json, PRIVATE)
        .await?;

    let mut events_jsonl = String::new();
    for ev in events {
        events_jsonl
            .push_str(&serde_json::to_string(ev).map_err(|e| CoreError::State(e.to_string()))?);
        events_jsonl.push('\n');
    }
    exec.write_file(&format!("{}/events.jsonl", dir), &events_jsonl, PRIVATE)
        .await?;

    exec.write_file(
        &format!("{}/commands.sh", dir),
        &commands_script(events),
        0o700,
    )
    .await?;

    if let Ok(state) = exec.read_file(&format!("{}/state.json", state_dir)).await {
        exec.write_file(&format!("{}/state-at-failure.json", dir), &state, PRIVATE)
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
        exec.write_file(&format!("{}/journal-tail.jsonl", dir), &tail, PRIVATE)
            .await?;
    }
    exec.write_file(&format!("{}/versions.txt", dir), versions, PRIVATE)
        .await?;
    // fix-125: the directories too. write_file creates them with the
    // default mode (0755), which lets any account list the bundles. Best
    // effort: the files inside are 0600 already, the bundle is worth more
    // than this, and the daemon repeats it at every start.
    let _ = exec
        .run(&crate::executor::Cmd::new(
            "chmod",
            &["700", &format!("{}/incidents", state_dir), &dir],
            10,
        ))
        .await;
    Ok(dir)
}

// ── Retention (fix-131) ─────────────────────────────────────────────────────

/// fix-131: bundles older than this many days are removed so they cannot
/// leak disk forever (they are in no backup). Generous on purpose: a
/// quarter covers every failure worth reading back. Story:
/// `docs/deployment/REGISTER.md`.
pub const BUNDLE_MAX_AGE_DAYS: u64 = 90;
/// fix-131: at most this many bundles are kept, newest first, so a
/// crash-looping night cannot fill the disk inside the age limit.
pub const BUNDLE_MAX_COUNT: usize = 200;

/// fix-131: the bundle directories to remove from `names`, oldest first.
/// A bundle is `<unix-ts>-<op>`; a name that is not is not the pruner's to
/// judge and stays.
pub fn bundles_to_prune(
    names: &[String],
    now: u64,
    max_age_days: u64,
    max_count: usize,
) -> Vec<String> {
    let mut dated: Vec<(u64, &String)> = names
        .iter()
        .filter_map(|n| {
            let (ts, op) = n.split_once('-')?;
            (!op.is_empty()).then_some(())?;
            Some((ts.parse::<u64>().ok()?, n))
        })
        .collect();
    // Newest first: the first `max_count` young enough are kept.
    dated.sort_by_key(|d| std::cmp::Reverse(d.0));
    let oldest_kept = now.saturating_sub(max_age_days * 86_400);
    let mut kept = 0usize;
    let mut gone: Vec<(u64, &String)> = Vec::new();
    for (ts, n) in dated {
        if ts >= oldest_kept && kept < max_count {
            kept += 1;
        } else {
            gone.push((ts, n));
        }
    }
    gone.sort_by_key(|(ts, _)| *ts);
    gone.into_iter().map(|(_, n)| n.clone()).collect()
}

/// fix-131: past this size the journal is compacted to half of it.
pub const JOURNAL_MAX_BYTES: usize = 4 * 1024 * 1024;

/// fix-131: the journal cut back to its newest lines, or None when
/// `content` is within `max_bytes`. It grew for good and was read whole at
/// every start and failure. The cut keeps whole lines, about half of
/// `max_bytes` of the newest ones, and in front of them the last record of
/// every operation still marked running, so `interrupted_ops` reads the same
/// answer after the cut as before it.
pub fn compact_journal(content: &str, max_bytes: usize) -> Option<String> {
    if content.len() <= max_bytes {
        return None;
    }
    let lines: Vec<&str> = content.lines().collect();
    let interrupted: Vec<String> = interrupted_ops(content)
        .into_iter()
        .map(|(op, _)| op)
        .collect();
    // The last line of each interrupted operation, by index.
    let mut keep_idx: Vec<usize> = interrupted
        .iter()
        .filter_map(|op| {
            lines.iter().rposition(|l| {
                serde_json::from_str::<serde_json::Value>(l)
                    .ok()
                    .and_then(|v| v.get("op").and_then(|o| o.as_str()).map(|o| o == op))
                    .unwrap_or(false)
            })
        })
        .collect();
    let mut budget =
        (max_bytes / 2).saturating_sub(keep_idx.iter().map(|i| lines[*i].len() + 1).sum());
    let mut start = lines.len();
    while start > 0 && lines[start - 1].len() < budget {
        budget -= lines[start - 1].len() + 1;
        start -= 1;
    }
    keep_idx.retain(|i| *i < start);
    keep_idx.sort_unstable();
    keep_idx.extend(start..lines.len());
    let mut out = String::new();
    for i in keep_idx {
        out.push_str(lines[i]);
        out.push('\n');
    }
    Some(out)
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
