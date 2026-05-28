use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseRecord {
    pub name: String,
    pub status: String,
    pub error: Option<String>,
    pub ts_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionLedger {
    pub operation: String,
    pub stack_name: String,
    pub status: String,
    pub phases: Vec<PhaseRecord>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn ledger_dir() -> PathBuf {
    PathBuf::from(".client-state/transactions")
}

fn save(path: &Path, ledger: &TransactionLedger) -> io::Result<()> {
    let content = serde_json::to_string_pretty(ledger)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    fs::write(path, content)
}

fn load(path: &Path) -> io::Result<TransactionLedger> {
    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

pub fn begin(operation: &str, stack_name: &str) -> io::Result<PathBuf> {
    let dir = ledger_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}-{}-{}.json", operation, stack_name, now_unix()));
    let ledger = TransactionLedger {
        operation: operation.to_string(),
        stack_name: stack_name.to_string(),
        status: "in_progress".to_string(),
        phases: Vec::new(),
    };
    save(&path, &ledger)?;
    Ok(path)
}

pub fn record_phase(path: &Path, phase_name: &str, status: &str, error: Option<&str>) -> io::Result<()> {
    let mut ledger = load(path)?;
    ledger.phases.push(PhaseRecord {
        name: phase_name.to_string(),
        status: status.to_string(),
        error: error.map(str::to_string),
        ts_unix: now_unix(),
    });
    save(path, &ledger)
}

pub fn finish(path: &Path, ok: bool) -> io::Result<()> {
    let mut ledger = load(path)?;
    ledger.status = if ok { "completed" } else { "failed" }.to_string();
    save(path, &ledger)
}
