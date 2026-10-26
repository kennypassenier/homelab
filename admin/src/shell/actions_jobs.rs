//! jobs-kept-1 (Kenny, 2026-10-26: the Jobs page "no longer kept" a job
//! after the dashboard's own update): the kept jobs live in the app
//! database as well as in memory, and a restarted dashboard reads them back.
//!
//! One writer task owns the writes. A job changes often (every progress
//! event), so the task coalesces: it writes each changed job once per
//! wake-up, from the newest in-memory copy. The log is written only when it
//! is complete (the job finished) or when the process is about to die under
//! it (a job that restarts the dashboard), never on every event.

use std::collections::VecDeque;

use chassis::Error;
use chassis::storage::AppDb;
use chassis::storage::rusqlite::{Connection, Transaction, params};
use tokio::sync::mpsc;

use super::{Actions, JOB_LOG_MAX, JobState, JobView, Origin};

/// The most log lines written per job. The memory copy keeps
/// [`JOB_LOG_MAX`]; the disk copy is the tail a person reads after a restart.
pub const STORED_LOG_MAX: usize = 5_000;

const NOTE: &str = "the dashboard restarted while this job ran; History tells how it ended";

impl Actions {
    /// Mount: read the kept jobs back, then write every later change down.
    pub fn set_job_store(&self, db: AppDb) {
        let rows = match db.read_blocking(read_jobs) {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("the kept jobs were not read: {}", e.message);
                Vec::new()
            }
        };
        self.restore_jobs(rows);
        let (tx, rx) = mpsc::unbounded_channel();
        if self.inner.job_writes.set(tx).is_err() {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move { me.write_jobs(db, rx).await });
    }

    fn restore_jobs(&self, rows: Vec<(JobView, Vec<serde_json::Value>)>) {
        let (Ok(mut jobs), Ok(mut logs), Ok(mut batches)) = (
            self.inner.jobs.lock(),
            self.inner.job_logs.lock(),
            self.inner.batches.lock(),
        ) else {
            return;
        };
        for (mut view, log) in rows {
            // The process that ran it is gone. A job that restarts the
            // dashboard keeps its state: the tab that started it reads the
            // real end from the host's history. Any other unfinished job
            // would otherwise stay "running" for good.
            if !view.state.finished() && !view.restarts_dashboard {
                view.state = JobState::Unknown;
                view.message.get_or_insert_with(|| NOTE.to_string());
            }
            if let Origin::Batch { batch } = &view.origin {
                batches.entry(*batch).or_default().push(view.job);
            }
            logs.insert(view.job, VecDeque::from(log));
            jobs.push_back(view);
        }
    }

    /// Called by `store`: this job changed, write it down.
    pub(super) fn note_job(&self, view: &JobView) {
        if let Some(tx) = self.inner.job_writes.get() {
            let _ = tx.send((view.job, view.state.finished() || view.restarts_dashboard));
        }
    }

    async fn write_jobs(self, db: AppDb, mut rx: mpsc::UnboundedReceiver<(u64, bool)>) {
        while let Some(first) = rx.recv().await {
            let mut todo = vec![first];
            while let Ok(more) = rx.try_recv() {
                todo.push(more);
            }
            todo.sort_unstable();
            // One entry per job; its log is wanted when any of its notes wanted it.
            let mut batch: Vec<(u64, bool)> = Vec::new();
            for (job, with_log) in todo {
                match batch.last_mut() {
                    Some(last) if last.0 == job => last.1 |= with_log,
                    _ => batch.push((job, with_log)),
                }
            }
            let rows: Vec<(JobView, Option<Vec<serde_json::Value>>)> = batch
                .into_iter()
                .filter_map(|(job, with_log)| {
                    let view = self.job(job)?;
                    let log = with_log.then(|| tail(self.job_log_lines(job).unwrap_or_default()));
                    Some((view, log))
                })
                .collect();
            let keep = self.keep_jobs();
            if let Err(e) = db.write(move |tx| put_jobs(tx, &rows, keep)).await {
                tracing::warn!("the jobs were not kept: {}", e.message);
            }
        }
    }
}

fn tail(mut lines: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let keep = STORED_LOG_MAX.min(JOB_LOG_MAX);
    if lines.len() > keep {
        lines.drain(..lines.len() - keep);
    }
    lines
}

fn stored(what: &str, e: impl std::fmt::Display) -> Error {
    Error::internal(
        format!("a job could not be stored: {what}: {e}"),
        "report this with the dashboard's log",
    )
}

fn put_jobs(
    tx: &Transaction,
    rows: &[(JobView, Option<Vec<serde_json::Value>>)],
    keep: usize,
) -> Result<(), Error> {
    for (view, log) in rows {
        let json = serde_json::to_string(view).map_err(|e| stored("the job", e))?;
        let state = crate::shell::notice_store::enum_text(&view.state);
        match log {
            Some(log) => {
                let log = serde_json::to_string(log).map_err(|e| stored("the log", e))?;
                tx.execute(
                    "INSERT INTO jobs (job, queued_at, state, view, log) VALUES (?1,?2,?3,?4,?5) \
                     ON CONFLICT (job) DO UPDATE SET queued_at = ?2, state = ?3, view = ?4, log = ?5",
                    params![view.job as i64, view.queued_at, state, json, log],
                )?;
            }
            None => {
                tx.execute(
                    "INSERT INTO jobs (job, queued_at, state, view) VALUES (?1,?2,?3,?4) \
                     ON CONFLICT (job) DO UPDATE SET queued_at = ?2, state = ?3, view = ?4",
                    params![view.job as i64, view.queued_at, state, json],
                )?;
            }
        }
    }
    // The cap: the newest `keep` jobs stay (job ids grow with the clock).
    tx.execute(
        "DELETE FROM jobs WHERE job NOT IN (SELECT job FROM jobs ORDER BY job DESC LIMIT ?1)",
        params![keep as i64],
    )?;
    Ok(())
}

fn read_jobs(c: &Connection) -> Result<Vec<(JobView, Vec<serde_json::Value>)>, Error> {
    let mut stmt = c.prepare("SELECT view, log FROM jobs ORDER BY job ASC")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (view, log) = row?;
        // A row that does not read (an older shape) is skipped, not fatal:
        // the Jobs page loses that one job, the dashboard still starts.
        match serde_json::from_str::<JobView>(&view) {
            Ok(v) => out.push((v, serde_json::from_str(&log).unwrap_or_default())),
            Err(e) => tracing::warn!("a stored job does not read: {e}"),
        }
    }
    Ok(out)
}
