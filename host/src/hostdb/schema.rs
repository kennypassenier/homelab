//! The schema as ONE numbered list, append only: a step that shipped is never
//! edited. `PRAGMA user_version` holds how many steps this file has had.

use rusqlite::Connection;

use super::{Res, why};

/// Step N is `MIGRATIONS[N - 1]`.
const MIGRATIONS: &[&str] = &[
    // 1: the four stores and the import markers.
    "CREATE TABLE journal (
        id     INTEGER PRIMARY KEY AUTOINCREMENT,
        ts     INTEGER NOT NULL,
        op     TEXT    NOT NULL,
        step   TEXT    NOT NULL,
        status TEXT    NOT NULL
     );
     CREATE INDEX journal_op ON journal (op, id);
     CREATE TABLE notices (
        seq  INTEGER PRIMARY KEY,
        at   INTEGER NOT NULL,
        body TEXT    NOT NULL
     );
     CREATE INDEX notices_at ON notices (at);
     CREATE TABLE watched_backups (
        name        TEXT PRIMARY KEY,
        rclone_path TEXT    NOT NULL,
        newest_unix INTEGER,
        learned_at  INTEGER NOT NULL,
        source      TEXT    NOT NULL
     );
     CREATE TABLE incidents (
        name   TEXT PRIMARY KEY,
        ts     INTEGER NOT NULL,
        op     TEXT    NOT NULL,
        report TEXT,
        events TEXT
     );
     CREATE INDEX incidents_ts ON incidents (ts);
     CREATE TABLE imports (
        source TEXT PRIMARY KEY,
        rows   INTEGER NOT NULL,
        at     INTEGER NOT NULL
     );",
];

/// Apply every step this file has not had, each in its own transaction.
pub(super) fn apply(conn: &mut Connection) -> Res<()> {
    let have: i64 = why(
        "read user_version",
        conn.query_row("PRAGMA user_version", [], |r| r.get(0)),
    )?;
    if have as usize > MIGRATIONS.len() {
        return Err(format!(
            "host db: the file is from a newer host (schema {have}, this build knows {})",
            MIGRATIONS.len()
        ));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(have as usize) {
        let tx = why("begin migration", conn.transaction())?;
        why("migration", tx.execute_batch(sql))?;
        why(
            "user_version",
            tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1)),
        )?;
        why("commit migration", tx.commit())?;
    }
    Ok(())
}
