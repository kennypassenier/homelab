//! The operation journal (B5, AR13): one row per record, written before a
//! step runs, so an interrupted operation is visible after a restart.
//!
//! The rows render back to the JSON lines the pure readers in
//! `homelab_core::incidents` and `ops::today` already understand, so they
//! did not change.

use rusqlite::params;

use super::{HostDb, Res, why};

/// One record, as one transaction.
pub(crate) fn record(db: &HostDb, ts: u64, op: &str, step: &str, status: &str) -> Res<()> {
    db.with(|c| {
        why(
            "journal insert",
            c.execute(
                "INSERT INTO journal (ts, op, step, status) VALUES (?1, ?2, ?3, ?4)",
                params![ts as i64, op, step, status],
            ),
        )
        .map(|_| ())
    })
}

fn line(ts: i64, op: &str, step: &str, status: &str) -> String {
    serde_json::json!({"ts": ts, "op": op, "step": step, "status": status}).to_string()
}

/// The newest `max` records, oldest first, as JSON lines.
pub(crate) fn tail(db: &HostDb, max: usize) -> Res<Vec<String>> {
    let mut rows = rows(db, "ORDER BY id DESC LIMIT ?1", max as i64)?;
    rows.reverse();
    Ok(rows.into_iter().map(|(_, l)| l).collect())
}

fn rows(db: &HostDb, tail_sql: &str, arg: i64) -> Res<Vec<(i64, String)>> {
    db.with(|c| {
        let sql = format!("SELECT id, ts, op, step, status FROM journal {tail_sql}");
        let mut st = why("journal prepare", c.prepare(&sql))?;
        let mapped = why(
            "journal read",
            st.query_map(params![arg], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    line(
                        r.get(1)?,
                        &r.get::<_, String>(2)?,
                        &r.get::<_, String>(3)?,
                        &r.get::<_, String>(4)?,
                    ),
                ))
            }),
        )?;
        why("journal row", mapped.collect())
    })
}

/// The whole journal as JSON lines, oldest first (capped by [`compact`]).
pub(crate) fn text(db: &HostDb) -> Res<String> {
    let all = rows(db, "WHERE id > ?1 ORDER BY id", 0)?;
    Ok(all.into_iter().map(|(_, l)| l + "\n").collect())
}

/// Operations whose newest record is still `running`: (operation, step).
pub(crate) fn interrupted(db: &HostDb) -> Res<Vec<(String, String)>> {
    Ok(homelab_core::incidents::interrupted_ops(&text(db)?))
}

/// Cut the journal back to the newest half of `max_bytes` when it has
/// outgrown `max_bytes`, keeping the last record of every operation still
/// running (`incidents::journal_keep`). Returns (before, after) bytes when
/// it cut. One transaction: a cut mid-way leaves the old table.
pub(crate) fn compact(db: &HostDb, max_bytes: u64) -> Res<Option<(usize, usize)>> {
    let all = rows(db, "WHERE id > ?1 ORDER BY id", 0)?;
    let lines: Vec<&str> = all.iter().map(|(_, l)| l.as_str()).collect();
    let Some(keep) = homelab_core::incidents::journal_keep(&lines, max_bytes as usize) else {
        return Ok(None);
    };
    let kept_ids: std::collections::HashSet<i64> = keep.iter().map(|i| all[*i].0).collect();
    let before: usize = lines.iter().map(|l| l.len() + 1).sum();
    let after: usize = keep.iter().map(|i| lines[*i].len() + 1).sum();
    db.with(|c| {
        let tx = why("compact begin", c.transaction())?;
        for (id, _) in all.iter().filter(|(id, _)| !kept_ids.contains(id)) {
            why(
                "compact delete",
                tx.execute("DELETE FROM journal WHERE id = ?1", params![id]),
            )?;
        }
        why("compact commit", tx.commit())
    })?;
    Ok(Some((before, after)))
}
