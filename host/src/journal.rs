//! The database-backed journal of operation records (B5).

use crate::*;

/// B5/AR13: append-only journal in the host database; "running" records land
/// (committed, `synchronous=FULL`) before a step executes, so an interrupted
/// operation is visible after restart.
pub(crate) struct DbJournal {
    pub(crate) state_dir: String,
}

impl Journal for DbJournal {
    fn record(&self, op: &str, step: &str, status: &str) {
        let written = hostdb::for_dir(&self.state_dir)
            .and_then(|db| hostdb::journal::record(&db, hostdb::now_s(), op, step, status));
        if let Err(e) = written {
            // The `Journal` trait cannot fail an operation; the loss is said.
            tracing::error!("journal: could not record {op} / {step} / {status} :: {e}");
        }
    }
}

// ── Server ──────────────────────────────────────────────────────────────────
