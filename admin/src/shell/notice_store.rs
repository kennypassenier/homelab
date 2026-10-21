//! feat-kpadopt-2 (4.0.0): the notification centre's rows in the app
//! database. One `notices` table (a column for everything a page filters or
//! the code looks up by, one JSON cell for the rest) and one `notify_state`
//! row for the settings and the cursors. Every function here is plain
//! blocking SQL on a connection or a transaction the caller owns, so the
//! centre runs each operation as ONE transaction: a power cut leaves the
//! state before or after it, never half (the old file was temp + fsync +
//! rename for the same reason).
//!
//! Nothing here deletes a notice. What the old store dropped on its own (the
//! oldest past 500, anything past 180 days) now stays until the Storage page
//! deletes it with a preview and a typed confirmation (Kenny, 2026-10-21).
//! Reads are bounded by a `LIMIT` instead.

use std::collections::BTreeMap;

use chassis::Error;
use chassis::storage::rusqlite::{Connection, OptionalExtension, Row, Transaction, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::core::notify::{
    self, AlertDraft, Draft, Fix, Kind, Notice, PushOutcome, Settings, settles,
};

type Res<T> = Result<T, Error>;

pub(crate) const COLS: &str = "id, at, kind, level, stack, is_read, title, body, job, since, \
     source, notice_key, op, label, resolved_by, extra";

/// What a notice carries that no query asks about.
#[derive(Serialize, Deserialize)]
struct Extra {
    push: PushOutcome,
    push_short: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    consequence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remedy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    link: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    fixes: Vec<Fix>,
}

fn bad(what: &str, e: impl std::fmt::Display) -> Error {
    Error::internal(
        format!("a stored notification does not read: {what}: {e}"),
        "the database file may be damaged; restore the pre-update copy or the last backup",
    )
}

/// `ActionDone` as `action_done`, the spelling the JSON and the table use.
pub(crate) fn enum_text<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

fn enum_from<T: DeserializeOwned>(s: &str) -> Res<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|e| bad(s, e))
}

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

struct Raw {
    id: i64,
    at: i64,
    kind: String,
    level: String,
    stack: Option<String>,
    is_read: i64,
    title: String,
    body: String,
    job: Option<i64>,
    since: Option<i64>,
    source: Option<String>,
    key: Option<String>,
    op: Option<String>,
    label: Option<String>,
    resolved_by: Option<i64>,
    extra: String,
}

fn raw(r: &Row) -> chassis::storage::rusqlite::Result<Raw> {
    Ok(Raw {
        id: r.get(0)?,
        at: r.get(1)?,
        kind: r.get(2)?,
        level: r.get(3)?,
        stack: r.get(4)?,
        is_read: r.get(5)?,
        title: r.get(6)?,
        body: r.get(7)?,
        job: r.get(8)?,
        since: r.get(9)?,
        source: r.get(10)?,
        key: r.get(11)?,
        op: r.get(12)?,
        label: r.get(13)?,
        resolved_by: r.get(14)?,
        extra: r.get(15)?,
    })
}

impl Raw {
    fn notice(self) -> Res<Notice> {
        let extra: Extra = serde_json::from_str(&self.extra).map_err(|e| bad("extra", e))?;
        Ok(Notice {
            id: self.id as u64,
            at: self.at,
            kind: enum_from(&self.kind)?,
            stack: self.stack,
            title: self.title,
            body: self.body,
            job: self.job.map(|j| j as u64),
            read: self.is_read != 0,
            push: extra.push,
            push_short: extra.push_short,
            level: enum_from(&self.level)?,
            since: self.since,
            consequence: extra.consequence,
            remedy: extra.remedy,
            link: extra.link,
            source: self.source,
            key: self.key,
            fixes: extra.fixes,
            op: self.op,
            label: self.label,
            resolved_by: self.resolved_by.map(|j| j as u64),
        })
    }
}

fn many(
    c: &Connection,
    sql: &str,
    args: &[&dyn chassis::storage::rusqlite::ToSql],
) -> Res<Vec<Notice>> {
    let mut stmt = c.prepare(sql)?;
    let rows = stmt.query_map(args, raw)?.collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(Raw::notice).collect()
}

/// Write `n` as a new row. `keep_id` writes its id (the one-time import);
/// otherwise the table hands out the next id, which is never reused, even
/// after a delete (`AUTOINCREMENT`). The id the row has.
pub(crate) fn insert(tx: &Transaction, n: &Notice, keep_id: bool) -> Res<u64> {
    let extra = json(&Extra {
        push: n.push.clone(),
        push_short: n.push_short.clone(),
        consequence: n.consequence.clone(),
        remedy: n.remedy.clone(),
        link: n.link.clone(),
        fixes: n.fixes.clone(),
    });
    let id: Option<i64> = keep_id.then_some(n.id as i64);
    tx.execute(
        &format!("INSERT INTO notices ({COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)"),
        params![
            id,
            n.at,
            enum_text(&n.kind),
            enum_text(&n.level),
            n.stack,
            n.read as i64,
            n.title,
            n.body,
            n.job.map(|j| j as i64),
            n.since,
            n.source,
            n.key,
            n.op,
            n.label,
            n.resolved_by.map(|j| j as i64),
            extra
        ],
    )?;
    Ok(tx.last_insert_rowid() as u64)
}

fn update(tx: &Transaction, n: &Notice) -> Res<()> {
    let extra = json(&Extra {
        push: n.push.clone(),
        push_short: n.push_short.clone(),
        consequence: n.consequence.clone(),
        remedy: n.remedy.clone(),
        link: n.link.clone(),
        fixes: n.fixes.clone(),
    });
    tx.execute(
        "UPDATE notices SET level=?2, stack=?3, is_read=?4, title=?5, body=?6, job=?7, since=?8, \
         source=?9, notice_key=?10, op=?11, label=?12, resolved_by=?13, extra=?14 WHERE id=?1",
        params![
            n.id as i64,
            enum_text(&n.level),
            n.stack,
            n.read as i64,
            n.title,
            n.body,
            n.job.map(|j| j as i64),
            n.since,
            n.source,
            n.key,
            n.op,
            n.label,
            n.resolved_by.map(|j| j as i64),
            extra
        ],
    )?;
    Ok(())
}

/// Store a draft. A success settles every open failure, incident or missed
/// schedule of the same operation on the same stack (fix-inbox-resolve-1;
/// Kenny, 2026-10-09: "als er in inbox meldingen zijn zoals bv 'deploy media
/// failed', dan moet die melding zelf resolven de volgende keer dat media
/// succesvol deployed"). Only unread rows can be open, so only they are read.
pub(crate) fn add(tx: &Transaction, d: Draft, at: i64, push: PushOutcome) -> Res<Notice> {
    let mut n = Notice::from_draft(0, d, at, push);
    n.id = insert(tx, &n, false)?;
    if n.kind == Kind::ActionDone
        && let Some(stack) = n.stack.clone()
    {
        let open = many(
            tx,
            &format!(
                "SELECT {COLS} FROM notices WHERE is_read = 0 AND stack = ?1 AND at <= ?2 \
                 AND id <> ?3 AND kind IN ('action_failed','incident','schedule_missed')"
            ),
            &[&stack, &n.at, &(n.id as i64)],
        )?;
        for mut x in open {
            if settles(&n, &x) {
                x.read = true;
                x.resolved_by = Some(n.id);
                tx.execute(
                    "UPDATE notices SET is_read = 1, resolved_by = ?2 WHERE id = ?1",
                    params![x.id as i64, n.id as i64],
                )?;
            }
        }
    }
    Ok(n)
}

/// fix-watch-1: a Down notice for a key that is still open and unread. The
/// watch does not add a second one while it waits (every dashboard restart
/// used to announce the same outage again); its body is refreshed instead,
/// so the newest reason shows. The same holds for the storage warning, which
/// would otherwise repeat at every start. `None` when none is open or the
/// draft is not a keyed one of those kinds.
pub(crate) fn refresh_open(tx: &Transaction, d: &Draft) -> Res<Option<Notice>> {
    let Some(key) = d.detail.key.as_deref() else {
        return Ok(None);
    };
    if !matches!(d.kind, Kind::Down | Kind::StorageWarn) {
        return Ok(None);
    }
    let found = many(
        tx,
        &format!(
            "SELECT {COLS} FROM notices WHERE kind = ?1 AND is_read = 0 AND notice_key = ?2 \
             ORDER BY id DESC LIMIT 1"
        ),
        &[&enum_text(&d.kind), &key],
    )?;
    let Some(mut x) = found.into_iter().next() else {
        return Ok(None);
    };
    x.body = d.body.clone();
    update(tx, &x)?;
    Ok(Some(x))
}

/// One host notice into the list. The notice of the dashboard job that
/// asked for it (`job`) takes the host's words instead of a second notice
/// being added. The notice as stored.
pub(crate) fn import_host(
    tx: &Transaction,
    n: &homelab_core::notify::HostNotice,
    job: Option<u64>,
    at: i64,
) -> Res<Notice> {
    let (d, push) = notify::host_draft(n);
    if let Some(j) = job {
        let found = many(
            tx,
            &format!("SELECT {COLS} FROM notices WHERE job = ?1 ORDER BY id DESC LIMIT 1"),
            &[&(j as i64)],
        )?;
        if let Some(mut x) = found.into_iter().next() {
            notify::merge_host(&mut x, d);
            update(tx, &x)?;
            return Ok(x);
        }
    }
    add(tx, d, at, push)
}

/// One alert into the list. A repeat of an alert that still fires adds
/// nothing; a resolved one is stored read, and the firing notice it ends no
/// longer waits. An alert is open when the newest notice with its key is a
/// firing one.
pub(crate) fn add_alert(tx: &Transaction, a: AlertDraft, at: i64) -> Res<Option<Notice>> {
    let key = a.draft.detail.key.clone().unwrap_or_default();
    let newest = many(
        tx,
        &format!("SELECT {COLS} FROM notices WHERE notice_key = ?1 ORDER BY id DESC LIMIT 1"),
        &[&key],
    )?
    .into_iter()
    .next()
    .filter(|n| n.kind == Kind::Alert);
    if a.firing {
        if newest.is_some() {
            return Ok(None);
        }
        return Ok(Some(add(tx, a.draft, at, a.push)?));
    }
    if let Some(open) = newest {
        tx.execute(
            "UPDATE notices SET is_read = 1 WHERE id = ?1",
            params![open.id as i64],
        )?;
    }
    let mut n = add(tx, a.draft, at, a.push)?;
    tx.execute(
        "UPDATE notices SET is_read = 1 WHERE id = ?1",
        params![n.id as i64],
    )?;
    n.read = true;
    Ok(Some(n))
}

/// Mark some notices (or all, with `ids` None) read or unread; how many
/// changed.
pub(crate) fn mark(tx: &Transaction, ids: Option<&[u64]>, read: bool) -> Res<usize> {
    let to = read as i64;
    Ok(match ids {
        None => tx.execute(
            "UPDATE notices SET is_read = ?1 WHERE is_read <> ?1",
            params![to],
        )?,
        Some(ids) => tx.execute(
            "UPDATE notices SET is_read = ?1 WHERE is_read <> ?1 \
             AND id IN (SELECT value FROM json_each(?2))",
            params![to, json(&ids)],
        )?,
    })
}

pub(crate) fn unread(c: &Connection) -> Res<usize> {
    Ok(
        c.query_row("SELECT COUNT(*) FROM notices WHERE is_read = 0", [], |r| {
            r.get::<_, i64>(0)
        })? as usize,
    )
}

/// All notices, read or not: for the Inbox's "of N" line.
pub(crate) fn total(c: &Connection) -> Res<u64> {
    Ok(c.query_row("SELECT COUNT(*) FROM notices", [], |r| r.get::<_, i64>(0))? as u64)
}

/// The unread count per stack, for badges.
pub(crate) fn unread_by_stack(c: &Connection) -> Res<BTreeMap<String, usize>> {
    let mut stmt = c.prepare(
        "SELECT stack, COUNT(*) FROM notices WHERE is_read = 0 AND stack IS NOT NULL GROUP BY stack",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().map(|(s, n)| (s, n as usize)).collect())
}

/// The newest `limit` notices, newest first.
pub(crate) fn newest(c: &Connection, limit: usize) -> Res<Vec<Notice>> {
    many(
        c,
        &format!("SELECT {COLS} FROM notices ORDER BY id DESC LIMIT ?1"),
        &[&(limit as i64)],
    )
}

/// The newest `limit` unread notices, newest first (the digest's).
pub(crate) fn newest_unread(c: &Connection, limit: usize) -> Res<Vec<Notice>> {
    many(
        c,
        &format!("SELECT {COLS} FROM notices WHERE is_read = 0 ORDER BY id DESC LIMIT ?1"),
        &[&(limit as i64)],
    )
}

// ── the settings and the cursors ────────────────────────────────────────

/// The one `notify_state` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct State {
    pub settings: Settings,
    /// Incident bundles already turned into a notice.
    pub seen_incidents: Vec<String>,
    pub incidents_seeded: bool,
    /// The newest host notice read (`HostNotice::seq`).
    pub host_cursor: u64,
    pub host_seeded: bool,
    pub last_digest: Option<notify::DigestRecord>,
}

pub(crate) fn state(c: &Connection) -> Res<State> {
    let (settings, seen, inc, cursor, hs, digest): (
        String,
        String,
        i64,
        i64,
        i64,
        Option<String>,
    ) = c
        .query_row(
            "SELECT settings, seen_incidents, incidents_seeded, host_cursor, host_seeded, last_digest \
             FROM notify_state WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| bad("notify_state", "the row is missing"))?;
    Ok(State {
        settings: serde_json::from_str(&settings).map_err(|e| bad("settings", e))?,
        seen_incidents: serde_json::from_str(&seen).map_err(|e| bad("seen_incidents", e))?,
        incidents_seeded: inc != 0,
        host_cursor: cursor as u64,
        host_seeded: hs != 0,
        last_digest: digest
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| bad("last_digest", e))?,
    })
}

pub(crate) fn put_state(tx: &Transaction, s: &State) -> Res<()> {
    tx.execute(
        "UPDATE notify_state SET settings = ?1, seen_incidents = ?2, incidents_seeded = ?3, \
         host_cursor = ?4, host_seeded = ?5, last_digest = ?6 WHERE id = 1",
        params![
            json(&s.settings),
            json(&s.seen_incidents),
            s.incidents_seeded as i64,
            s.host_cursor as i64,
            s.host_seeded as i64,
            s.last_digest.as_ref().map(json),
        ],
    )?;
    Ok(())
}
