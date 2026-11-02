//! The host's answer to `Executor::records()`: core operations reach the
//! database through this, by state directory, never by file path.

use std::collections::BTreeMap;

use homelab_core::error::CoreError;
use homelab_core::ops::watched::WatchedRecord;
use homelab_core::records::RecordStore;

use super::{for_dir, incidents, journal, watched};

/// Zero-sized: the open databases live in the registry ([`for_dir`]).
pub(crate) struct HostRecords;

fn core(e: String) -> CoreError {
    CoreError::State(e)
}

fn db(dir: &str) -> Result<std::sync::Arc<super::HostDb>, CoreError> {
    for_dir(dir).map_err(core)
}

impl RecordStore for HostRecords {
    fn watched_load(&self, dir: &str) -> Result<BTreeMap<String, WatchedRecord>, CoreError> {
        watched::load(&*db(dir)?).map_err(core)
    }

    fn watched_save(
        &self,
        dir: &str,
        r: &BTreeMap<String, WatchedRecord>,
    ) -> Result<(), CoreError> {
        watched::save(&*db(dir)?, r).map_err(core)
    }

    fn journal_tail(&self, dir: &str, max: usize) -> Result<Vec<String>, CoreError> {
        journal::tail(&*db(dir)?, max).map_err(core)
    }

    fn incident_save(
        &self,
        dir: &str,
        name: &str,
        report: &str,
        events: &str,
    ) -> Result<(), CoreError> {
        incidents::save(&*db(dir)?, name, report, events).map_err(core)
    }
}
