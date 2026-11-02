//! The incident bundles' data: `report.json` and `events.jsonl` of each, one
//! row per bundle. The rest of a bundle (`commands.sh`, `versions.txt`,
//! `state-at-failure.json`, `journal-tail.jsonl`) stays in the bundle's
//! directory; [`names`] lists the union so a half-written bundle shows too.

use std::collections::BTreeSet;

use rusqlite::params;

use super::{HostDb, Res, why};

/// A bundle's stored text: (report JSON, events JSONL).
pub(crate) type Stored = (Option<String>, Option<String>);

/// `<unix-ts>-<op>` → (ts, op); None when the name is not a bundle name.
pub(super) fn split_name(name: &str) -> Option<(u64, &str)> {
    let (ts, op) = name.split_once('-')?;
    (!op.is_empty()).then_some(())?;
    Some((ts.parse().ok()?, op))
}

pub(crate) fn save(db: &HostDb, name: &str, report: &str, events: &str) -> Res<()> {
    let (ts, op) =
        split_name(name).ok_or_else(|| format!("host db: '{name}' is not a bundle name"))?;
    db.with(|c| {
        why(
            "incident save",
            c.execute(
                "INSERT OR REPLACE INTO incidents (name, ts, op, report, events) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![name, ts as i64, op, report, events],
            ),
        )
        .map(|_| ())
    })
}

pub(crate) fn get(db: &HostDb, name: &str) -> Res<Option<Stored>> {
    db.with(|c| {
        let mut st = why(
            "incident prepare",
            c.prepare("SELECT report, events FROM incidents WHERE name = ?1"),
        )?;
        let mut rows = why("incident read", st.query(params![name]))?;
        match why("incident row", rows.next())? {
            Some(r) => Ok(Some((why("report", r.get(0))?, why("events", r.get(1))?))),
            None => Ok(None),
        }
    })
}

fn db_names(db: &HostDb) -> Res<Vec<String>> {
    db.with(|c| {
        let mut st = why("incident names", c.prepare("SELECT name FROM incidents"))?;
        let rows = why(
            "incident names",
            st.query_map([], |r| r.get::<_, String>(0)),
        )?;
        why("incident names", rows.collect())
    })
}

/// Every bundle name, sorted: the rows plus the directories under
/// `<state_dir>/incidents/`.
pub(crate) fn names(db: &HostDb) -> Res<Vec<String>> {
    let mut all: BTreeSet<String> = db_names(db)?.into_iter().collect();
    let dir = db.state_dir.join("incidents");
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            all.insert(e.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(all.into_iter().collect())
}

/// Remove one bundle: its row and its directory. Returns what failed.
pub(crate) fn remove(db: &HostDb, name: &str) -> Res<()> {
    db.with(|c| {
        why(
            "incident delete",
            c.execute("DELETE FROM incidents WHERE name = ?1", params![name]),
        )
        .map(|_| ())
    })?;
    let dir = db.state_dir.join("incidents").join(name);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "incidents: could not remove {} :: {e}",
            dir.display()
        )),
    }
}
