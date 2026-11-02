//! host-db-1: the host's own records (watched-backup answers, incident
//! bundle data, the journal tail) behind one trait, so core operations ask
//! the executor for them instead of reading files by path. The host answers
//! from its sqlite database; [`MemRecords`] answers in tests.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::error::CoreError;
use crate::ops::watched::WatchedRecord;

/// Where the host keeps what it has learned and what it has recorded.
/// `state_dir` names the store (one database per state directory).
pub trait RecordStore: Send + Sync {
    /// The recorded watched-backup answers, by watcher name.
    fn watched_load(&self, state_dir: &str) -> Result<BTreeMap<String, WatchedRecord>, CoreError>;
    /// Record these answers (insert or replace per watcher).
    fn watched_save(
        &self,
        state_dir: &str,
        records: &BTreeMap<String, WatchedRecord>,
    ) -> Result<(), CoreError>;
    /// The newest `max` journal records as JSON lines, oldest first.
    fn journal_tail(&self, state_dir: &str, max: usize) -> Result<Vec<String>, CoreError>;
    /// Keep an incident bundle's `report.json` and `events.jsonl` text under
    /// the bundle's name (`<unix-ts>-<op>`).
    fn incident_save(
        &self,
        state_dir: &str,
        name: &str,
        report_json: &str,
        events_jsonl: &str,
    ) -> Result<(), CoreError>;
}

/// The answer of an executor that has no store: every read is empty and a
/// write is refused out loud, never swallowed.
pub struct NoRecords;

fn none() -> CoreError {
    CoreError::State("this executor keeps no records (RecordStore)".into())
}

impl RecordStore for NoRecords {
    fn watched_load(&self, _: &str) -> Result<BTreeMap<String, WatchedRecord>, CoreError> {
        Ok(BTreeMap::new())
    }
    fn watched_save(&self, _: &str, _: &BTreeMap<String, WatchedRecord>) -> Result<(), CoreError> {
        Err(none())
    }
    fn journal_tail(&self, _: &str, _: usize) -> Result<Vec<String>, CoreError> {
        Ok(Vec::new())
    }
    fn incident_save(&self, _: &str, _: &str, _: &str, _: &str) -> Result<(), CoreError> {
        Err(none())
    }
}

/// In-memory records for the `MockExecutor` (state directory ignored).
#[derive(Default)]
pub struct MemRecords {
    watched: Mutex<BTreeMap<String, WatchedRecord>>,
    journal: Mutex<Vec<String>>,
    incidents: Mutex<BTreeMap<String, (String, String)>>,
}

impl MemRecords {
    /// Add one journal line, as the host would have recorded it.
    pub fn seed_journal(&self, line: &str) {
        self.journal.lock().unwrap().push(line.to_string());
    }
    /// The saved (report, events) of a bundle.
    pub fn incident(&self, name: &str) -> Option<(String, String)> {
        self.incidents.lock().unwrap().get(name).cloned()
    }
    /// Names of the saved bundles.
    pub fn incident_names(&self) -> Vec<String> {
        self.incidents.lock().unwrap().keys().cloned().collect()
    }
    /// The recorded watched answers.
    pub fn watched(&self) -> BTreeMap<String, WatchedRecord> {
        self.watched.lock().unwrap().clone()
    }
    /// Seed the watched answers.
    pub fn seed_watched(&self, records: &BTreeMap<String, WatchedRecord>) {
        self.watched.lock().unwrap().extend(records.clone());
    }
}

impl RecordStore for MemRecords {
    fn watched_load(&self, _: &str) -> Result<BTreeMap<String, WatchedRecord>, CoreError> {
        Ok(self.watched())
    }
    fn watched_save(&self, _: &str, r: &BTreeMap<String, WatchedRecord>) -> Result<(), CoreError> {
        self.seed_watched(r);
        Ok(())
    }
    fn journal_tail(&self, _: &str, max: usize) -> Result<Vec<String>, CoreError> {
        let j = self.journal.lock().unwrap();
        Ok(j[j.len().saturating_sub(max)..].to_vec())
    }
    fn incident_save(
        &self,
        _: &str,
        name: &str,
        report: &str,
        events: &str,
    ) -> Result<(), CoreError> {
        self.incidents
            .lock()
            .unwrap()
            .insert(name.to_string(), (report.to_string(), events.to_string()));
        Ok(())
    }
}
