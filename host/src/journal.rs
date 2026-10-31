//! The file-backed journal of operation records (B5).

use crate::*;

/// B5/AR13: append-only JSONL journal; "running" records land before a step
/// executes, so an interrupted operation is visible after restart.
pub(crate) struct FileJournal {
    pub(crate) path: String,
}

impl Journal for FileJournal {
    fn record(&self, op: &str, step: &str, status: &str) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = serde_json::json!({"ts": ts, "op": op, "step": step, "status": status});
        if let Some(parent) = std::path::Path::new(&self.path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── Server ──────────────────────────────────────────────────────────────────
