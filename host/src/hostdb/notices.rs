//! The host's notices for the dashboard: one row per notice, keyed by its
//! sequence number (the dashboard's cursor). `body` is the notice as
//! `HostNotice` JSON, so a field added to the notice needs no schema step.

use homelab_core::notify::HostNotice;
use rusqlite::{Transaction, params};

use super::{HostDb, Res, why};

fn parse(body: String) -> Res<HostNotice> {
    serde_json::from_str(&body)
        .map_err(|e| format!("host db: a stored notice is unreadable :: {e}"))
}

/// Insert one notice (a seq that exists is replaced), then cut the table by
/// the history limits when it is over `max_bytes`: nothing older than
/// `max_age_s`, then the oldest dropped until it is under half of
/// `max_bytes`; the shape `notify::prune_notices` cut the file in.
pub(crate) fn append(
    db: &HostDb,
    n: &HostNotice,
    now: u64,
    max_age_s: u64,
    max_bytes: usize,
) -> Res<()> {
    let body = serde_json::to_string(n).map_err(|e| format!("host db: notice :: {e}"))?;
    db.with(|c| {
        let tx = why("notice begin", c.transaction())?;
        insert(&tx, n.seq, n.at, &body)?;
        prune(&tx, now, max_age_s, max_bytes)?;
        why("notice commit", tx.commit())
    })
}

pub(super) fn insert(tx: &Transaction, seq: u64, at: u64, body: &str) -> Res<()> {
    why(
        "notice insert",
        tx.execute(
            "INSERT OR REPLACE INTO notices (seq, at, body) VALUES (?1, ?2, ?3)",
            params![seq as i64, at as i64, body],
        ),
    )
    .map(|_| ())
}

/// The cap, inside the caller's transaction.
pub(crate) fn prune(tx: &Transaction, now: u64, max_age_s: u64, max_bytes: usize) -> Res<()> {
    let total: i64 = why(
        "notice size",
        tx.query_row(
            "SELECT COALESCE(SUM(LENGTH(body) + 1), 0) FROM notices",
            [],
            |r| r.get(0),
        ),
    )?;
    if (total as usize) <= max_bytes {
        return Ok(());
    }
    let cutoff = now.saturating_sub(max_age_s) as i64;
    why(
        "notice age",
        tx.execute("DELETE FROM notices WHERE at < ?1", params![cutoff]),
    )?;
    let mut st = why(
        "notice order",
        tx.prepare("SELECT seq, LENGTH(body) + 1 FROM notices ORDER BY seq"),
    )?;
    let rows: Vec<(i64, i64)> = why(
        "notice sizes",
        why(
            "notice sizes",
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?))),
        )?
        .collect(),
    )?;
    drop(st);
    let mut left: i64 = rows.iter().map(|r| r.1).sum();
    for (seq, len) in rows {
        if left as usize <= max_bytes / 2 {
            break;
        }
        why(
            "notice drop",
            tx.execute("DELETE FROM notices WHERE seq = ?1", params![seq]),
        )?;
        left -= len;
    }
    Ok(())
}

fn read(db: &HostDb, sql: &str, args: &[i64]) -> Res<Vec<HostNotice>> {
    let bodies: Vec<String> = db.with(|c| {
        let mut st = why("notices prepare", c.prepare(sql))?;
        let rows = why(
            "notices read",
            st.query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(0)),
        )?;
        why("notices row", rows.collect())
    })?;
    bodies.into_iter().map(parse).collect()
}

/// Every notice, oldest first.
pub(crate) fn all(db: &HostDb) -> Res<Vec<HostNotice>> {
    read(db, "SELECT body FROM notices ORDER BY seq", &[])
}

/// The notices after `after`, oldest first, at most `limit` (the rest wait
/// for the next read, so none is skipped), and the newest seq of all.
pub(crate) fn after(db: &HostDb, after: u64, limit: usize) -> Res<(Vec<HostNotice>, u64)> {
    let found = read(
        db,
        "SELECT body FROM notices WHERE seq > ?1 ORDER BY seq LIMIT ?2",
        &[after as i64, limit as i64],
    )?;
    Ok((found, last_seq(db)?))
}

pub(crate) fn last_seq(db: &HostDb) -> Res<u64> {
    db.with(|c| {
        let n: i64 = why(
            "last seq",
            c.query_row("SELECT COALESCE(MAX(seq), 0) FROM notices", [], |r| {
                r.get(0)
            }),
        )?;
        Ok(n as u64)
    })
}

/// Swap the open "could not be delivered" notice `prev_seq` for its folded
/// successor in one transaction, so no reader sees both or neither.
pub(crate) fn replace(db: &HostDb, prev_seq: u64, folded: &HostNotice) -> Res<()> {
    let body = serde_json::to_string(folded).map_err(|e| format!("host db: notice :: {e}"))?;
    db.with(|c| {
        let tx = why("replace begin", c.transaction())?;
        why(
            "replace delete",
            tx.execute(
                "DELETE FROM notices WHERE seq = ?1",
                params![prev_seq as i64],
            ),
        )?;
        insert(&tx, folded.seq, folded.at, &body)?;
        why("replace commit", tx.commit())
    })
}
