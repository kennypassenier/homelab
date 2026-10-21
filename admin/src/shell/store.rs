//! feat-kpadopt-2 (4.0.0): all of the dashboard's data in one sqlite file,
//! `<state dir>/homelab-admin.db`, through chassis-rs 3.11.0's storage layer
//! (Kenny, 2026-10-21). This module is the whole description of that file:
//!
//! * [`migrations`]: the schema as ONE numbered list, append only. It holds
//!   the tables of all six stores the dashboard used to keep as JSON, so
//!   moving a store over later changes code and no schema.
//! * [`import`]: the one-time move of the six JSON files, in one transaction
//!   with a count check, run by chassis at the first start of 4.0.0.
//! * [`kinds`]: what the kit's Storage page lists, filters and deletes
//!   (targeted delete, with a preview and a typed confirmation).
//! * [`warn_draft`]: the Inbox notice for the budget's warning.
//!
//! | table | was | time column | moved |
//! |---|---|---|---|
//! | `notices` | `notifications.json` | `at` | yes |
//! | `notify_state` | `notifications.json` (settings, cursors) | | yes |
//! | `inbox_snoozes` | `inbox-snoozes.json` | `set_at` | yes |
//! | `schedules` | `schedules.json` | `created_at` | yes |
//! | `update_records` | `update-flows.json` | `at` | yes |
//! | `uptime_cells` | `uptime-history.json` | `hour_at` | yes |
//! | `trend_buckets`, `trend_stacks` | `fleet-trend.json` | `at` | yes |
//!
//! All six stores run on their tables; the import copies the six JSON files
//! in and renames each to `*.migrated` (a store reported `moved`, chassis 3.12.0). Nothing deletes a row on
//! its own except what is finished by definition (ended snoozes): the budget
//! only warns, and old rows go through the Storage page (Kenny, 2026-10-21).
//! `hour_at` and `at` are unix SECONDS, the unit [`TimeFormat::UnixSeconds`]
//! reads.
//!
//! Power loss: sqlite runs in WAL mode with `synchronous=FULL` (chassis),
//! every write here is one transaction, and the import commits its marker
//! with its data, so a cut leaves the old JSON files or the finished
//! database, and the next start tries the import again in the first case.

use std::path::{Path, PathBuf};

use chassis::Error;
use chassis::storage::rusqlite::{Connection, Transaction, params};
use chassis::storage::{
    AppDb, AppDbSpec, BrowserKeys, DataKind, Filter, ImportReport, ImportStore,
    Level as BudgetLevel, Migration, StorageReport, TimeFormat,
};

use super::actions_state::read_json;
use super::notice_store;
use crate::core::notify::{Detail, Draft, Kind, Level, NotifyFile};
use crate::core::rowsnooze;
use crate::core::schedule::ScheduleFile;
use crate::core::trend;
use crate::core::updateflow::UpdateRecord;
use crate::core::uptime;

pub const FILE_NOTICES: &str = "notifications.json";
pub const FILE_SNOOZES: &str = "inbox-snoozes.json";
pub const FILE_SCHEDULES: &str = "schedules.json";
pub const FILE_FLOWS: &str = "update-flows.json";
pub const FILE_UPTIME: &str = "uptime-history.json";
pub const FILE_TREND: &str = "fleet-trend.json";

/// The browser keys that follow Kenny from device to device (chassis-rs
/// 3.12.0 preferences, Kenny 2026-10-21: "app state belongs in the database,
/// not the browser, unless the web app needs it to work").
///
/// * `homelab.` / `homelab-`: the dashboard's own choices (log levels, Stacks
///   view and measure, Map looks, Apps stars, the tour, the update layout).
/// * `nx-sort:` / `kp-remember:`: table sorts, and every `data-kp-remember`
///   choice of kp-themes (its writes are caught without changing kp).
/// * `kp-unsaved:` in `sessionStorage`: the Settings Save bar's draft
///   (`kp-unsaved:/settings:settings`). kp leaves password fields out of it
///   and chassis refuses a draft with a secret-named field; a secret is
///   never kept, never synced (`web/test/secretkeep.test.js`, invariant 121).
///
/// Left in the browser: the theme keys `theme-boot` reads before login, the
/// stored `theme` that `themesettle` repairs, calm mode (a device's own
/// comfort, listed under "This browser"), and the per-tab Live view state.
pub fn browser_keys() -> BrowserKeys {
    BrowserKeys::new()
        .prefix("homelab.")
        .prefix("homelab-")
        .prefix("nx-sort:")
        .prefix("kp-remember:")
        .session_prefix("kp-unsaved:")
        // Read before login by theme-boot, so the browser's copy must stay.
        .except_exact("homelab.theme-follow-system")
        .except_exact("homelab.theme-light")
        .except_exact("homelab.theme-dark")
        // themesettle's key (kp's theme store).
        .except_exact("theme")
        // Per tab, by design: the tab's name and the Live view switch.
        .except_prefix("homelab.drive.")
        .except_exact("homelab.live-view")
        // This device's calm mode.
        .except_prefix("kp-remember:calm:")
}

/// How long after it ended a snooze is left for the dashboard to tell its
/// reminder (the first tick after a start does): the same 30 days the JSON
/// store kept ended snoozes for.
pub const SNOOZE_GRACE_S: i64 = 30 * 86_400;

/// The schema. Append only: a step that shipped is never edited.
pub fn migrations() -> Vec<Migration> {
    vec![
        // 1 · the Inbox's notices. One column for everything a page filters
        // by or the code looks up by; the rest of a notice is the JSON cell.
        // `AUTOINCREMENT`: an id is never handed out twice, even after the
        // Storage page deleted the newest row.
        Migration::sql(
            "CREATE TABLE notices (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                at          INTEGER NOT NULL,
                kind        TEXT    NOT NULL,
                level       TEXT    NOT NULL,
                stack       TEXT,
                is_read     INTEGER NOT NULL DEFAULT 0,
                title       TEXT    NOT NULL,
                body        TEXT    NOT NULL,
                job         INTEGER,
                since       INTEGER,
                source      TEXT,
                notice_key  TEXT,
                op          TEXT,
                label       TEXT,
                resolved_by INTEGER,
                extra       TEXT    NOT NULL DEFAULT '{}'
            ) STRICT;
            CREATE INDEX notices_at ON notices (at);
            CREATE INDEX notices_level_at ON notices (level, at);
            CREATE INDEX notices_unread ON notices (id) WHERE is_read = 0;
            CREATE INDEX notices_stack ON notices (stack, is_read) WHERE stack IS NOT NULL;
            CREATE INDEX notices_key ON notices (notice_key, id) WHERE notice_key IS NOT NULL;
            CREATE INDEX notices_job ON notices (job) WHERE job IS NOT NULL;",
        ),
        // 2 · the centre's settings and cursors: one row, settings as the
        // JSON the API already speaks (`{}` reads as every default).
        Migration::sql(
            "CREATE TABLE notify_state (
                id               INTEGER PRIMARY KEY CHECK (id = 1),
                settings         TEXT    NOT NULL DEFAULT '{}',
                seen_incidents   TEXT    NOT NULL DEFAULT '[]',
                incidents_seeded INTEGER NOT NULL DEFAULT 0,
                host_cursor      INTEGER NOT NULL DEFAULT 0,
                host_seeded      INTEGER NOT NULL DEFAULT 0,
                last_digest      TEXT
            ) STRICT;
            INSERT INTO notify_state (id) VALUES (1);",
        ),
        // 3 · the Inbox's per-row snoozes (at most 300 rows: core::rowsnooze).
        Migration::sql(
            "CREATE TABLE inbox_snoozes (
                row_key TEXT    PRIMARY KEY,
                until   INTEGER NOT NULL,
                title   TEXT    NOT NULL,
                href    TEXT,
                remind  INTEGER NOT NULL DEFAULT 0,
                set_at  INTEGER NOT NULL
            ) STRICT;
            CREATE INDEX inbox_snoozes_until ON inbox_snoozes (until);",
        ),
        // 4 · schedules: what the page lists and the tick compares as
        // columns, the action's arguments and the two last-run records as
        // JSON cells (their shapes belong to core::schedule).
        Migration::sql(
            "CREATE TABLE schedules (
                id            TEXT    PRIMARY KEY,
                stack         TEXT    NOT NULL,
                action        TEXT    NOT NULL,
                enabled       INTEGER NOT NULL DEFAULT 1,
                note          TEXT    NOT NULL DEFAULT '',
                created_at    INTEGER NOT NULL,
                handled_until INTEGER NOT NULL,
                paused_until  INTEGER,
                args          TEXT    NOT NULL DEFAULT '{}',
                when_json     TEXT    NOT NULL,
                last_run      TEXT,
                last_missed   TEXT
            ) STRICT;
            CREATE INDEX schedules_stack ON schedules (stack);",
        ),
        // 5 · the Update flow's undo records. `stacks` is the touched stacks
        // as `,a,b,` so the page can filter by one.
        Migration::sql(
            "CREATE TABLE update_records (
                job     INTEGER PRIMARY KEY,
                at      INTEGER NOT NULL,
                by_whom TEXT    NOT NULL,
                outcome TEXT    NOT NULL,
                stacks  TEXT    NOT NULL DEFAULT ',',
                items   TEXT    NOT NULL DEFAULT '[]',
                commits TEXT    NOT NULL DEFAULT '[]'
            ) STRICT;
            CREATE INDEX update_records_at ON update_records (at);
            CREATE INDEX update_records_outcome ON update_records (outcome, at);",
        ),
        // 6 · the minute watch's hourly cells. `hour_at` is the unix second
        // the hour starts at (the old file counted hours since the epoch).
        // WITHOUT ROWID: chassis 3.12.0 previews such a kind by its primary
        // key (it used to order by `rowid`, which this table lacks).
        Migration::sql(
            "CREATE TABLE uptime_cells (
                cell_key TEXT    NOT NULL,
                hour_at  INTEGER NOT NULL,
                up       INTEGER NOT NULL DEFAULT 0,
                dep      INTEGER NOT NULL DEFAULT 0,
                down     INTEGER NOT NULL DEFAULT 0,
                ms_sum   INTEGER NOT NULL DEFAULT 0,
                ms_n     INTEGER NOT NULL DEFAULT 0,
                ms_max   INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (cell_key, hour_at)
            ) STRICT, WITHOUT ROWID;
            CREATE INDEX uptime_cells_hour ON uptime_cells (hour_at);",
        ),
        // 7 · the fleet's last day, one row per five-minute bucket and one
        // per stack in it. `at` is the bucket's start; deleting a bucket
        // takes its stacks with it.
        Migration::sql(
            "CREATE TABLE trend_buckets (
                slot         INTEGER PRIMARY KEY,
                at           INTEGER NOT NULL,
                host_cpu_sum INTEGER NOT NULL DEFAULT 0,
                host_cpu_n   INTEGER NOT NULL DEFAULT 0,
                load_sum     INTEGER NOT NULL DEFAULT 0,
                load_n       INTEGER NOT NULL DEFAULT 0
            ) STRICT;
            CREATE INDEX trend_buckets_at ON trend_buckets (at);
            CREATE TABLE trend_stacks (
                slot  INTEGER NOT NULL REFERENCES trend_buckets (slot) ON DELETE CASCADE,
                stack TEXT    NOT NULL,
                sum   INTEGER NOT NULL DEFAULT 0,
                n     INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (slot, stack)
            ) STRICT, WITHOUT ROWID;",
        ),
    ]
}

/// What the Storage page lists. Only the stores that live in the database
/// are declared: a table that still mirrors a JSON file would show numbers
/// nobody reads and offer a delete that changes nothing.
pub fn kinds() -> Vec<DataKind> {
    vec![
        DataKind::new("notices", "Notices")
            .description(
                "What the dashboard, the host and Alertmanager told you in the Inbox. \
                 Nothing here is removed on its own: delete the old ones when you want the space.",
            )
            .unit("notices")
            .table("notices")
            .time_column("at", TimeFormat::UnixSeconds)
            .filter(Filter::choice(
                "level",
                "Level",
                "level",
                ["critical", "warning", "info", "ok"],
            ))
            .filter(Filter::flag("read", "Read", "is_read"))
            .filter(Filter::text("stack", "Stack", "stack")),
        DataKind::new("snoozes", "Inbox snoozes")
            .description(
                "Rows you hid from the Inbox until a moment. A snooze that ended \
                 more than 30 days ago is finished and is removed daily.",
            )
            .unit("snoozes")
            .table("inbox_snoozes")
            .time_column("set_at", TimeFormat::UnixSeconds)
            .filter(Filter::flag("remind", "Remind me", "remind"))
            .finished_by_definition("until < :now - 2592000"),
        DataKind::new("schedules", "Schedules")
            .description(
                "The planned actions (backups, updates) of the Schedules page. \
                 Deleting one here removes the plan itself, so it stops running.",
            )
            .unit("schedules")
            .table("schedules")
            .time_column("created_at", TimeFormat::UnixSeconds)
            .filter(Filter::text("stack", "Stack", "stack")),
        DataKind::new("update_records", "Update records")
            .description(
                "The Update flow's undo records: what each update changed and \
                 whether it worked. The History page offers the last 7 days to undo; \
                 older ones only take space.",
            )
            .unit("records")
            .table("update_records")
            .time_column("at", TimeFormat::UnixSeconds)
            .filter(Filter::text("stack", "Stack", "stacks"))
            .filter(Filter::choice(
                "outcome",
                "Outcome",
                "outcome",
                ["done", "failed", "rolled back"],
            )),
        DataKind::new("uptime", "Uptime history")
            .description(
                "What the minute watch measured, one cell per hour and tile. \
                 The Apps strips draw the last 90 days; older cells only take space.",
            )
            .unit("hours")
            .table("uptime_cells")
            .time_column("hour_at", TimeFormat::UnixSeconds)
            .filter(Filter::text("key", "Tile or ping", "cell_key")),
        DataKind::new("cpu_lines", "CPU lines")
            .description(
                "The fleet's CPU and load, one point per five minutes. The Stacks \
                 sparklines draw the last 24 hours; older points only take space.",
            )
            .unit("points")
            .table("trend_buckets")
            .time_column("at", TimeFormat::UnixSeconds),
    ]
}

/// The app database's description: schema, kinds are declared separately
/// (`App::storage_kind`), and the one-time import of the JSON files in
/// `data_dir`.
pub fn spec(data_dir: PathBuf) -> AppDbSpec {
    AppDbSpec::new(migrations()).with_import(move |tx, _ctx| import(tx, &data_dir))
}

/// A database in a file, opened the way the service opens it (migrations
/// and kinds, no import), for tests that read it back through a second
/// handle.
pub fn file_db(path: &Path) -> AppDb {
    AppDb::open(path, AppDbSpec::new(migrations()), kinds()).expect("the database opens")
}

/// An empty database in memory, for tests of everything that reads or
/// writes the dashboard's data.
pub fn memory_db() -> AppDb {
    AppDb::open_in_memory(AppDbSpec::new(migrations()), kinds())
        .expect("the schema and the kinds agree")
}

// ── the one-time import ─────────────────────────────────────────────────

fn refuse(what: &str, e: impl std::fmt::Display) -> Error {
    Error::invalid(
        format!("{what} does not read as the dashboard's file: {e}"),
        format!(
            "fix or move {what} aside; the dashboard starts again with the next restart (nothing was imported)"
        ),
    )
}

/// The same check the old loader made, before anything opens: an
/// `notifications.json` this build cannot read stops the start (never
/// silently replaced by an empty list), exactly as it did.
pub fn precheck(data_dir: &Path) -> Result<(), String> {
    let path = data_dir.join(FILE_NOTICES);
    match read_json::<NotifyFile>(&path) {
        Ok(Some(f)) => f
            .check()
            .map_err(|why| format!("{}: {why}", path.display())),
        Ok(None) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// A store that never was a reason to stop: unreadable means empty, with a
/// line in the log (the old loaders did the same for a measurement log).
fn lenient<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    match read_json::<T>(path) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "an old dashboard file does not read and is not imported");
            None
        }
    }
}

fn count(tx: &Transaction, table: &str) -> Result<u64, Error> {
    Ok(
        tx.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
            r.get::<_, i64>(0)
        })? as u64,
    )
}

/// Move the six JSON files into the tables, in the caller's transaction.
/// Each store reports how many records its file held; after all six are
/// written the tables are counted and a store whose table holds another
/// number fails the whole import (chassis then rolls everything back).
pub fn import(tx: &Transaction, data_dir: &Path) -> Result<ImportReport, Error> {
    let at = |name: &str| data_dir.join(name);
    // (file, table, records the file held for that table)
    let mut checks: Vec<(&'static str, &'static str, u64)> = Vec::new();
    let mut note = |file: &'static str, table: &'static str, n: u64| {
        checks.push((file, table, n));
    };

    // notifications.json: strict, as the old loader was.
    if let Some(f) =
        read_json::<NotifyFile>(&at(FILE_NOTICES)).map_err(|e| refuse(FILE_NOTICES, e))?
    {
        f.check().map_err(|why| refuse(FILE_NOTICES, why))?;
        let mut top = f.next_id.saturating_sub(1) as i64;
        for n in &f.notices {
            notice_store::insert(tx, n, true)?;
            top = top.max(n.id as i64);
        }
        // The next id is the file's, even when its newest notice had fallen
        // off: an id that was handed out stays spent.
        if tx.execute(
            "UPDATE sqlite_sequence SET seq = MAX(seq, ?1) WHERE name = 'notices'",
            params![top],
        )? == 0
        {
            tx.execute(
                "INSERT INTO sqlite_sequence (name, seq) VALUES ('notices', ?1)",
                params![top],
            )?;
        }
        notice_store::put_state(
            tx,
            &notice_store::State {
                settings: f.settings,
                seen_incidents: f.seen_incidents,
                incidents_seeded: f.incidents_seeded,
                host_cursor: f.host_cursor,
                host_seeded: f.host_seeded,
                last_digest: f.last_digest,
            },
        )?;
        note(FILE_NOTICES, "notices", f.notices.len() as u64);
    }

    // inbox-snoozes.json: a file that does not read is set aside, the rows
    // simply show again (what the old loader did).
    let snoozes = at(FILE_SNOOZES);
    match read_json::<rowsnooze::File>(&snoozes) {
        Ok(Some(f)) => {
            let mut keys = std::collections::BTreeSet::new();
            let mut n = 0u64;
            // The last row of a key wins, as a snooze set again moves.
            for e in f.rows.iter().rev().filter(|e| keys.insert(e.key.clone())) {
                tx.execute(
                    "INSERT INTO inbox_snoozes (row_key, until, title, href, remind, set_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![e.key, e.until, e.title, e.href, e.remind as i64, e.set_at],
                )?;
                n += 1;
            }
            note(FILE_SNOOZES, "inbox_snoozes", n);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(error = %e, "inbox snoozes not read, starting empty");
            let _ = std::fs::rename(&snoozes, snoozes.with_extension("json.bad"));
        }
    }

    // schedules.json
    // Strict, as the old loader was: a file of another version or zone stops
    // the import (and with it the start) instead of being replaced by nothing.
    if let Some(f) =
        read_json::<ScheduleFile>(&at(FILE_SCHEDULES)).map_err(|e| refuse(FILE_SCHEDULES, e))?
    {
        f.check().map_err(|why| refuse(FILE_SCHEDULES, why))?;
        for s in &f.schedules {
            super::scheduler::put_schedule(tx, s)?;
        }
        note(FILE_SCHEDULES, "schedules", f.schedules.len() as u64);
    }

    // update-flows.json
    if let Some(all) = lenient::<Vec<UpdateRecord>>(&at(FILE_FLOWS)) {
        for r in &all {
            tx.execute(
                "INSERT INTO update_records (job, at, by_whom, outcome, stacks, items, commits) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    r.job as i64,
                    r.at,
                    r.by,
                    r.outcome,
                    super::actions::stacks_cell(r),
                    to_json(&r.items)?,
                    to_json(&r.commits)?,
                ],
            )?;
        }
        note(FILE_FLOWS, "update_records", all.len() as u64);
    }

    // uptime-history.json
    if let Some(s) = lenient::<uptime::Store>(&at(FILE_UPTIME)) {
        let mut n = 0u64;
        for (key, cells) in &s.keys {
            for c in cells {
                tx.execute(
                    "INSERT INTO uptime_cells (cell_key, hour_at, up, dep, down, ms_sum, ms_n, ms_max) \
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![
                        key,
                        c.h.saturating_mul(3600),
                        c.up,
                        c.dep,
                        c.down,
                        c.ms_sum as i64,
                        c.ms_n,
                        c.ms_max
                    ],
                )?;
                n += 1;
            }
        }
        note(FILE_UPTIME, "uptime_cells", n);
    }

    // fleet-trend.json
    if let Some(saved) = lenient::<trend::Saved>(&at(FILE_TREND)) {
        let step = saved.trend.step_s();
        let (mut buckets, mut stacks) = (0u64, 0u64);
        for b in saved.trend.buckets() {
            tx.execute(
                "INSERT INTO trend_buckets (slot, at, host_cpu_sum, host_cpu_n, load_sum, load_n) \
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    b.slot as i64,
                    (b.slot.saturating_mul(step)) as i64,
                    b.host_cpu.sum as i64,
                    b.host_cpu.n as i64,
                    b.load_x100.sum as i64,
                    b.load_x100.n as i64
                ],
            )?;
            buckets += 1;
            for (name, m) in &b.stacks {
                tx.execute(
                    "INSERT INTO trend_stacks (slot, stack, sum, n) VALUES (?1,?2,?3,?4)",
                    params![b.slot as i64, name, m.sum as i64, m.n as i64],
                )?;
                stacks += 1;
            }
        }
        note(FILE_TREND, "trend_buckets", buckets);
        note(FILE_TREND, "trend_stacks", stacks);
    }

    // The count check, per store (chassis 3.12.0): what each table holds is
    // what its file held, and every store here lives in the database, so
    // each of its files is renamed once the import committed.
    let mut stores: Vec<ImportStore> = Vec::new();
    for (file, table, want) in &checks {
        let have = count(tx, table)?;
        if have != *want {
            return Err(Error::internal(
                format!("the import of `{table}` wrote {have} rows, its file held {want}"),
                "nothing was kept; report this, the next start tries again",
            ));
        }
        match stores.iter_mut().find(|s| s.name == *file) {
            Some(s) => {
                s.expected += want;
                s.imported += have;
            }
            None => {
                let mut s = ImportStore::new(file, *want, have);
                if at(file).exists() {
                    s = s.moved(vec![at(file)]);
                }
                stores.push(s);
            }
        }
    }
    Ok(ImportReport::from_stores(stores))
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<String, Error> {
    serde_json::to_string(v)
        .map_err(|e| Error::internal(format!("a record does not serialise: {e}"), "report this"))
}

// ── the budget's warning ────────────────────────────────────────────────

/// Megabytes as chassis counts them (1 MB = 1 000 000 bytes), one decimal.
fn mb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_000_000.0)
}

/// The Inbox notice for a budget that is at its warn line or past the
/// budget. Chassis only warns, and so does this: the notice says nothing was
/// refused or deleted, and where to look.
pub fn warn_draft(r: &StorageReport) -> Draft {
    let (total, warn, budget) = (r.total_bytes, r.warn_bytes, r.budget_bytes);
    let over = r.level == BudgetLevel::Over;
    let mut d = Draft::new(
        Kind::StorageWarn,
        "storage-warn",
        &if over {
            format!(
                "The dashboard's data is past its budget: {} of {} MB",
                mb(total),
                mb(budget)
            )
        } else {
            format!(
                "The dashboard's data is nearly at its budget: {} of {} MB",
                mb(total),
                mb(budget)
            )
        },
        &format!(
            "The files the dashboard keeps its data in hold {} MB; the warning line is {} MB and the budget {} MB.",
            mb(total),
            mb(warn),
            mb(budget)
        ),
    );
    d.detail = Detail {
        level: Level::Warning,
        consequence: Some(
            "Nothing breaks and nothing is deleted or refused: the budget only warns.".into(),
        ),
        remedy: Some(
            "Open Storage to see what takes the space and delete what you no longer need, or raise the budget there."
                .into(),
        ),
        link: Some("/storage".into()),
        key: Some("storage-budget".into()),
        ..Default::default()
    };
    d
}

/// Read access for the few places that need the connection directly.
pub fn read<T>(db: &AppDb, f: impl FnOnce(&Connection) -> Result<T, Error>) -> Result<T, Error> {
    db.read_blocking(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chassis::storage::{FileReport, FileSizes, Now};

    fn dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("homelab-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn the_browser_keys_sync_the_choices_and_keep_the_boot_and_tab_keys_home() {
        let k = browser_keys();
        for synced in [
            "homelab.log-levels",
            "homelab-toured",
            "homelab-update-jobs",
            "homelab.stacks.view",
            "homelab.apps.starred",
            "nx-sort:stacks",
            "kp-remember:host-refresh:value",
        ] {
            assert!(k.is_synced_local(synced), "{synced} should sync");
        }
        assert!(k.is_synced_session("kp-unsaved:/settings:settings"));
        for home in [
            "homelab.theme-follow-system",
            "homelab.theme-light",
            "homelab.theme-dark",
            "theme",
            "homelab.drive.tab",
            "homelab.live-view",
            "kp-remember:calm:calm:value",
            "chassis-prefs:synced",
            "somebody-else",
        ] {
            assert!(!k.is_synced(home), "{home} should stay in the browser");
        }
        // One key, one area: a kp-unsaved draft is never a localStorage key.
        assert!(!k.is_synced_local("kp-unsaved:/settings:settings"));
    }

    #[test]
    fn the_schema_and_the_kinds_agree() {
        // `memory_db` validates every kind against the tables.
        let db = memory_db();
        assert_eq!(db.kinds().len(), 6);
        let tables: Vec<String> = db
            .read_blocking(|c| {
                let mut s = c.prepare(
                    "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
                     AND name NOT LIKE 'chassis_%' ORDER BY name",
                )?;
                Ok(s.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
            })
            .unwrap();
        assert_eq!(
            tables,
            [
                "inbox_snoozes",
                "notices",
                "notify_state",
                "schedules",
                "trend_buckets",
                "trend_stacks",
                "update_records",
                "uptime_cells"
            ]
        );
    }

    #[test]
    fn an_import_with_no_old_files_is_empty_and_renames_nothing() {
        let d = dir("empty");
        let db = AppDb::open(&d.join("t.db"), spec(d.clone()), kinds()).unwrap();
        assert_eq!(db.import_state(), chassis::storage::ImportState::Done);
        assert_eq!(db.read_blocking(notice_store::unread).unwrap(), 0);
    }

    #[test]
    fn the_six_files_land_in_their_tables_and_are_all_renamed() {
        let d = dir("six");
        std::fs::write(
            d.join(FILE_NOTICES),
            r#"{"schema_version":1,"next_id":9,"settings":{"push":false,"muted_stacks":["media"]},
                "notices":[{"id":3,"at":100,"kind":"action_failed","stack":"media","title":"t","body":"b",
                "read":false,"push":{"state":"sent"},"level":"warning","fixes":[]},
                {"id":4,"at":200,"kind":"down","title":"t2","body":"b2","read":true,
                "push":{"state":"skipped","why":"x"},"key":"k"}],
                "seen_incidents":["1-a"],"incidents_seeded":true,"host_cursor":7,"host_seeded":true}"#,
        )
        .unwrap();
        std::fs::write(
            d.join(FILE_SNOOZES),
            r#"{"schema_version":1,"rows":[{"key":"notice:3","until":900,"title":"t","remind":true,"set_at":100}]}"#,
        )
        .unwrap();
        std::fs::write(
            d.join(FILE_SCHEDULES),
            r#"{"schema_version":1,"zone":"Europe/Brussels","schedules":[{"id":"s1","stack":"media",
                "action":"backup","when":{"every":"day","at":"03:30"},"enabled":true,"note":"",
                "created_at":10,"handled_until":20}]}"#,
        )
        .unwrap();
        std::fs::write(
            d.join(FILE_FLOWS),
            r#"[{"job":5,"at":300,"by":"Kenny","items":[],"commits":["abc"],"outcome":"done"}]"#,
        )
        .unwrap();
        std::fs::write(
            d.join(FILE_UPTIME),
            r#"{"schema_version":1,"keys":{"tile:a":[{"h":10,"up":3},{"h":11,"down":1}]}}"#,
        )
        .unwrap();
        std::fs::write(
            d.join(FILE_TREND),
            r#"{"schema_version":1,"trend":{"step_s":300,"cap":288,"buckets":[
                {"slot":10,"host_cpu":{"sum":40,"n":2},"load_x100":{"sum":10,"n":1},
                 "stacks":{"media":{"sum":7,"n":1}}}]}}"#,
        )
        .unwrap();
        let db = AppDb::open(&d.join("t.db"), spec(d.clone()), kinds()).unwrap();
        assert_eq!(db.import_state(), chassis::storage::ImportState::Done);
        let (notices, snoozes, schedules, flows, cells): (i64, i64, i64, i64, i64) = db
            .read_blocking(|c| {
                let n = |t: &str| {
                    c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| {
                        r.get::<_, i64>(0)
                    })
                };
                Ok((
                    n("notices")?,
                    n("inbox_snoozes")?,
                    n("schedules")?,
                    n("update_records")?,
                    n("uptime_cells")?,
                ))
            })
            .unwrap();
        assert_eq!((notices, snoozes, schedules, flows, cells), (2, 1, 1, 1, 2));
        // The settings and cursors came along; the id counter is the file's.
        let st = db.read_blocking(notice_store::state).unwrap();
        assert!(!st.settings.push);
        assert_eq!(
            (st.host_cursor, st.host_seeded, st.incidents_seeded),
            (7, true, true)
        );
        let fresh = db
            .write_blocking(|tx| {
                notice_store::add(
                    tx,
                    Draft::new(Kind::ActionDone, "deploy-x", "t", "b"),
                    500,
                    crate::core::notify::PushOutcome::Sent,
                )
            })
            .unwrap();
        assert_eq!(fresh.id, 9, "next_id of the file is the next id handed out");
        // Every file that was imported is renamed, none is left to run on.
        for name in [
            FILE_NOTICES,
            FILE_SNOOZES,
            FILE_SCHEDULES,
            FILE_FLOWS,
            FILE_UPTIME,
            FILE_TREND,
        ] {
            assert!(
                !d.join(name).exists() && d.join(format!("{name}.migrated")).exists(),
                "{name} is renamed to .migrated"
            );
        }
        let (buckets, stacks): (i64, i64) = db
            .read_blocking(|c| {
                Ok((
                    c.query_row("SELECT COUNT(*) FROM trend_buckets", [], |r| r.get(0))?,
                    c.query_row("SELECT COUNT(*) FROM trend_stacks", [], |r| r.get(0))?,
                ))
            })
            .unwrap();
        assert_eq!((buckets, stacks), (1, 1));
    }

    #[test]
    fn stored_rows_are_never_dropped_by_age_or_count() {
        use crate::core::trend::{Bucket, Mean};
        use crate::core::updateflow::UpdateRecord;
        use crate::core::uptime::{Cell, Sample, Store};
        let db = memory_db();
        // 300 update records over 400 days: all stay, the read shows 7 days.
        let now = 400 * 86_400i64;
        db.write_blocking(|tx| {
            for i in 0..300i64 {
                crate::shell::actions::put_test_record(
                    tx,
                    &UpdateRecord {
                        job: i as u64,
                        at: now - i * 86_400 * 4 / 3,
                        by: "x".into(),
                        items: vec![],
                        commits: vec![],
                        outcome: "done".into(),
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
        let stored: i64 = db
            .read_blocking(|c| {
                Ok(c.query_row("SELECT COUNT(*) FROM update_records", [], |r| r.get(0))?)
            })
            .unwrap();
        assert_eq!(stored, 300);
        let shown = db
            .read_blocking(|c| crate::shell::actions::records_since_test(c, now - 7 * 86_400))
            .unwrap();
        assert_eq!(shown.len(), 6, "the undo list reads the last 7 days only");

        // Uptime: a flush writes the cells that changed; a reload reads the
        // 90-day window and nothing older, while the old cell stays stored.
        let mut s = Store::default();
        s.record("tile:a", 100 * 3600, Sample::Up);
        s.record("tile:a", 100 * 3600 + 5, Sample::Down);
        let cells = s.take_dirty();
        db.write_blocking(|tx| crate::shell::watch::put_cells(tx, &cells))
            .unwrap();
        let again = s.take_dirty();
        assert!(again.is_empty());
        let mut s2 = Store::default();
        s2.record("tile:a", 100 * 3600 + 9, Sample::Up);
        db.write_blocking(|tx| crate::shell::watch::put_cells(tx, &s2.take_dirty()))
            .unwrap();
        let back = db
            .read_blocking(|c| crate::shell::watch::cells_since(c, 0))
            .unwrap();
        assert_eq!(back.len(), 1, "one cell per key and hour, replaced");
        assert_eq!((back[0].1.up, back[0].1.h), (1, 100));
        let none = db
            .read_blocking(|c| crate::shell::watch::cells_since(c, 101 * 3600))
            .unwrap();
        assert!(none.is_empty(), "older than the window is not read");
        let _ = Cell::default();

        // The CPU lines: a bucket with its stacks, stored again without
        // doubling, read back with its stacks.
        let mut b = Bucket {
            slot: 5,
            host_cpu: Mean { sum: 30, n: 3 },
            ..Default::default()
        };
        b.stacks.insert("media".into(), Mean { sum: 9, n: 3 });
        for _ in 0..2 {
            db.write_blocking(|tx| crate::shell::host_link::put_bucket(tx, &b))
                .unwrap();
        }
        let got = db
            .read_blocking(|c| crate::shell::host_link::trend_since(c, 0))
            .unwrap();
        assert_eq!(got, vec![b]);
    }

    #[test]
    fn schedules_keep_their_order_and_their_records() {
        use crate::core::schedule::{LastRun, Schedule, When};
        let db = memory_db();
        let mk = |id: &str, h: &str| Schedule {
            id: id.into(),
            stack: "media".into(),
            action: "backup".into(),
            args: Default::default(),
            when: When::Day { at: h.into() },
            enabled: true,
            note: String::new(),
            created_at: 1,
            handled_until: 2,
            last_run: None,
            last_missed: None,
            paused_until: None,
        };
        db.write_blocking(|tx| {
            crate::shell::scheduler::put_schedule(tx, &mk("b", "01:00"))?;
            crate::shell::scheduler::put_schedule(tx, &mk("a", "02:00"))?;
            let mut again = mk("b", "03:00");
            again.last_run = Some(LastRun { slot: 9, job: 4 });
            again.paused_until = Some(77);
            crate::shell::scheduler::put_schedule(tx, &again)
        })
        .unwrap();
        let all = db.read_blocking(crate::shell::scheduler::all).unwrap();
        assert_eq!(
            all.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(all[0].last_run, Some(LastRun { slot: 9, job: 4 }));
        assert_eq!(all[0].paused_until, Some(77));
        assert_eq!(all[0].when, When::Day { at: "03:00".into() });
    }

    #[test]
    fn a_notifications_file_this_build_cannot_read_stops_the_import() {
        let d = dir("bad");
        std::fs::write(d.join(FILE_NOTICES), "{not json").unwrap();
        assert!(precheck(&d).is_err());
        let db = AppDb::open(&d.join("t.db"), spec(d.clone()), kinds()).unwrap();
        assert!(matches!(
            db.import_state(),
            chassis::storage::ImportState::Failed { .. }
        ));
        assert!(
            d.join(FILE_NOTICES).exists(),
            "the file is left where it is"
        );
        assert_eq!(db.read_blocking(notice_store::total).unwrap(), 0);
    }

    /// A report built by hand (chassis 3.12.0, gap 139).
    fn test_report(total: u64, warn: u64, budget: u64, level: BudgetLevel) -> StorageReport {
        StorageReport::new(
            vec![FileReport::new(
                "app",
                "homelab-admin.db",
                FileSizes::new(total, 0, 0),
                true,
            )],
            total,
            budget,
            warn,
            level,
            0,
        )
    }

    #[test]
    fn only_snoozes_that_ended_long_ago_are_finished_by_definition() {
        let db = memory_db();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        db.write_blocking(|tx| {
            for (k, until) in [
                ("ended-long-ago", now - SNOOZE_GRACE_S - 3_600),
                ("ended-lately", now - 3_600),
                ("running", now + 3_600),
            ] {
                tx.execute(
                    "INSERT INTO inbox_snoozes (row_key, until, title, remind, set_at) VALUES (?1,?2,'t',0,0)",
                    params![k, until],
                )?;
            }
            Ok(())
        })
        .unwrap();
        let removed = db.prune_finished_at(&Now::at(now)).unwrap();
        assert_eq!(removed, vec![("snoozes".to_string(), 1)]);
        let left: Vec<String> = db
            .read_blocking(|c| {
                let mut s = c.prepare("SELECT row_key FROM inbox_snoozes ORDER BY row_key")?;
                Ok(s.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
            })
            .unwrap();
        assert_eq!(left, ["ended-lately", "running"]);
    }

    #[test]
    fn the_warning_notice_says_nothing_was_deleted_and_where_to_look() {
        let d = warn_draft(&test_report(
            42_000_000,
            40_000_000,
            50_000_000,
            BudgetLevel::Warn,
        ));
        assert_eq!(d.kind, Kind::StorageWarn);
        assert_eq!(d.detail.link.as_deref(), Some("/storage"));
        assert!(d.title.contains("42.0 of 50.0 MB"));
        assert!(d.detail.consequence.unwrap().contains("nothing is deleted"));
    }
}
