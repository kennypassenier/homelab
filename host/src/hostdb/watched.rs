//! The recorded state of the watched backups (`ops::watched`): one row per
//! watcher name.

use homelab_core::ops::watched::{WatchedRecord, WatchedRecords};
use rusqlite::params;

use super::{HostDb, Res, why};

/// The most watchers kept. A host has a handful; the cap only stops a
/// config that churns watcher names from growing the table for good.
pub(crate) const MAX_ROWS: i64 = 1000;

pub(crate) fn load(db: &HostDb) -> Res<WatchedRecords> {
    db.with(|c| {
        let mut st = why(
            "watched prepare",
            c.prepare(
                "SELECT name, rclone_path, newest_unix, learned_at, source FROM watched_backups",
            ),
        )?;
        let rows = why(
            "watched read",
            st.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    WatchedRecord {
                        rclone_path: r.get(1)?,
                        newest_unix: r.get::<_, Option<i64>>(2)?.map(|t| t as u64),
                        learned_at: r.get::<_, i64>(3)? as u64,
                        source: r.get(4)?,
                    },
                ))
            }),
        )?;
        why("watched row", rows.collect())
    })
}

/// Upsert every record given, then drop the oldest beyond [`MAX_ROWS`]; one
/// transaction.
pub(crate) fn save(db: &HostDb, records: &WatchedRecords) -> Res<()> {
    db.with(|c| {
        let tx = why("watched begin", c.transaction())?;
        upsert_all(&tx, records)?;
        why(
            "watched cap",
            tx.execute(
                "DELETE FROM watched_backups WHERE name IN (SELECT name FROM watched_backups \
                 ORDER BY learned_at DESC, name LIMIT -1 OFFSET ?1)",
                params![MAX_ROWS],
            ),
        )?;
        why("watched commit", tx.commit())
    })
}

/// The insert both `save` and the import use.
pub(super) fn upsert_all(tx: &rusqlite::Transaction, records: &WatchedRecords) -> Res<()> {
    for (name, r) in records {
        why(
            "watched upsert",
            tx.execute(
                "INSERT INTO watched_backups (name, rclone_path, newest_unix, learned_at, source) \
                 VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(name) DO UPDATE SET \
                 rclone_path = ?2, newest_unix = ?3, learned_at = ?4, source = ?5",
                params![
                    name,
                    r.rclone_path,
                    r.newest_unix.map(|t| t as i64),
                    r.learned_at as i64,
                    r.source
                ],
            ),
        )?;
    }
    Ok(())
}
