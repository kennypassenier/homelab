//! The one-time move of the old files into the database (host-db-1).
//!
//! Per store, in this order, so a power cut at any point is safe to rerun:
//!
//! 1. the old file is copied to `<name>.pre-db-backup` (written under a
//!    temporary name and renamed, never over an existing backup);
//! 2. ONE transaction inserts every row, checks the table grew by exactly the
//!    number of rows read, writes the `imports` marker row and commits;
//! 3. only then is the old file renamed to `<name>.migrated`.
//!
//! A cut before the commit leaves the old file and an empty store: the next
//! start imports again. A cut after the commit leaves the marker, so the next
//! start only does step 3. Lines that cannot be read (a torn last line) are
//! skipped and counted in the log; they stay in the backup.

use std::path::{Path, PathBuf};

use homelab_core::notify::HostNotice;
use rusqlite::{Connection, Transaction, params};

use super::{Res, count, now_s, why};

/// What one fill read: rows inserted and lines left out.
pub(super) struct Fill {
    pub(super) rows: u64,
    pub(super) skipped: u64,
}

/// The old files, in the state directory.
pub(crate) const JOURNAL: &str = "journal.jsonl";
pub(crate) const NOTICES: &str = "notices.jsonl";
pub(crate) const WATCHED: &str = "watched-backups.json";

/// Import every old store that is still on disk.
pub(super) fn all(conn: &mut Connection, dir: &Path) -> Res<()> {
    file(conn, dir, JOURNAL, "journal", fill_journal)?;
    file(conn, dir, NOTICES, "notices", fill_notices)?;
    file(conn, dir, WATCHED, "watched_backups", fill_watched)?;
    super::import_incidents::run(conn, dir)
}

fn marked(conn: &Connection, marker: &str) -> Res<bool> {
    let n: i64 = why(
        "marker",
        conn.query_row(
            "SELECT COUNT(*) FROM imports WHERE source = ?1",
            params![marker],
            |r| r.get(0),
        ),
    )?;
    Ok(n > 0)
}

/// Copy `path` to `<path>.pre-db-backup` unless that exists.
pub(crate) fn backup(path: &Path) -> Res<()> {
    let target = suffixed(path, ".pre-db-backup");
    if target.exists() {
        return Ok(());
    }
    let tmp = suffixed(path, ".pre-db-backup.tmp");
    let copied = std::fs::copy(path, &tmp).and_then(|_| std::fs::File::open(&tmp)?.sync_all());
    copied
        .and_then(|_| std::fs::rename(&tmp, &target))
        .map_err(|e| format!("host db: could not back up {} :: {e}", path.display()))
}

/// Rename `path` to `<path>.migrated` (a free name when that is taken).
pub(super) fn finish(path: &Path) -> Res<()> {
    let mut target = suffixed(path, ".migrated");
    if target.exists() {
        target = suffixed(path, &format!(".migrated.{}", now_s()));
    }
    std::fs::rename(path, &target)
        .map_err(|e| format!("host db: could not rename {} :: {e}", path.display()))
}

pub(super) fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Run `fill` inside the import transaction: count check, marker, commit.
pub(super) fn commit_import(
    conn: &mut Connection,
    marker: &str,
    table: &str,
    fill: impl FnOnce(&Transaction) -> Res<Fill>,
) -> Res<Fill> {
    let tx = why("import begin", conn.transaction())?;
    let before = count(&tx, table)?;
    let read = fill(&tx)?;
    let after = count(&tx, table)?;
    if after - before != read.rows {
        return Err(format!(
            "host db: import of {marker} rolled back, {table} grew by {} but {} rows were read",
            after - before,
            read.rows
        ));
    }
    why(
        "marker insert",
        tx.execute(
            "INSERT INTO imports (source, rows, at) VALUES (?1, ?2, ?3)",
            params![marker, read.rows as i64, now_s() as i64],
        ),
    )?;
    why("import commit", tx.commit())?;
    Ok(read)
}

/// One single-file store: backup, transaction, rename.
fn file(
    conn: &mut Connection,
    dir: &Path,
    name: &str,
    table: &str,
    fill: fn(&Transaction, &str) -> Res<Fill>,
) -> Res<()> {
    let path = dir.join(name);
    if !path.exists() {
        return Ok(());
    }
    if !marked(conn, name)? {
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("host db: cannot read {} :: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        backup(&path)?;
        let read = commit_import(conn, name, table, |tx| fill(tx, &text))?;
        tracing::info!(
            "{name}: {} row(s) moved into the host database, {} line(s) skipped",
            read.rows,
            read.skipped
        );
    }
    finish(&path)
}

fn fill_journal(tx: &Transaction, text: &str) -> Res<Fill> {
    let (mut rows, mut skipped) = (0, 0);
    for l in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(l).unwrap_or_default();
        let (Some(op), Some(step), Some(status)) =
            (v["op"].as_str(), v["step"].as_str(), v["status"].as_str())
        else {
            tracing::warn!("journal.jsonl: skipped an unreadable line during the move");
            skipped += 1;
            continue;
        };
        let ts = v["ts"].as_u64().unwrap_or(0) as i64;
        why(
            "journal import",
            tx.execute(
                "INSERT INTO journal (ts, op, step, status) VALUES (?1, ?2, ?3, ?4)",
                params![ts, op, step, status],
            ),
        )?;
        rows += 1;
    }
    Ok(Fill { rows, skipped })
}

fn fill_notices(tx: &Transaction, text: &str) -> Res<Fill> {
    let mut by_seq = std::collections::BTreeMap::new();
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count() as u64;
    for n in homelab_core::notify::parse_notices(text) {
        by_seq.insert(n.seq, n);
    }
    for n in by_seq.values() {
        let body = serde_json::to_string::<HostNotice>(n)
            .map_err(|e| format!("host db: notice :: {e}"))?;
        super::notices::insert(tx, n.seq, n.at, &body)?;
    }
    let rows = by_seq.len() as u64;
    Ok(Fill {
        rows,
        skipped: lines.saturating_sub(rows),
    })
}

/// An unreadable file is an empty record, as it always was: the next check
/// asks once. The backup keeps the text.
fn fill_watched(tx: &Transaction, text: &str) -> Res<Fill> {
    let records: homelab_core::ops::watched::WatchedRecords = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("watched-backups.json: unreadable, moved as empty :: {e}");
            return Ok(Fill {
                rows: 0,
                skipped: 1,
            });
        }
    };
    super::watched::upsert_all(tx, &records)?;
    Ok(Fill {
        rows: records.len() as u64,
        skipped: 0,
    })
}
