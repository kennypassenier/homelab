//! host-db-1 (Kenny, 2026-11-02: "all data goes in databases"): the host's own
//! records in one sqlite file, `<state_dir>/homelab-host.db`, the same pattern
//! as the dashboard's `homelab-admin.db`.
//!
//! | table | was | cap |
//! |---|---|---|
//! | `journal` | `journal.jsonl` | `journal_max_bytes` ([`journal::compact`]) |
//! | `notices` | `notices.jsonl` | `history_days` / `history_max_mib` ([`notices::prune`]) |
//! | `watched_backups` | `watched-backups.json` | [`watched::MAX_ROWS`] |
//! | `incidents` | `incidents/*/report.json`, `events.jsonl` | age and count ([`incidents::prune`]) |
//!
//! Not here: `logs/host.jsonl` and `audit.log` (capped files by decision) and
//! the other files of an incident bundle (`commands.sh`, `versions.txt`, ...).
//!
//! Power loss: WAL with `synchronous=FULL`, every write is one transaction,
//! and an import commits its marker row with its data ([`import`]).
//!
//! Callers never hold a connection: they ask [`for_dir`] for the handle of a
//! state directory (one per directory per process) and call a store function.

pub(crate) mod import;
mod import_incidents;
pub(crate) mod incidents;
pub(crate) mod journal;
pub(crate) mod notices;
mod records;
mod schema;
pub(crate) mod watched;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::Connection;

pub(crate) use records::HostRecords;

/// The database file inside the state directory.
pub(crate) const DB_FILE: &str = "homelab-host.db";

/// What every store call answers with: the text names the store and the step.
pub(crate) type Res<T> = Result<T, String>;

/// Turn a sqlite error into one that says what was being done.
pub(crate) fn why<T>(what: &str, r: rusqlite::Result<T>) -> Res<T> {
    r.map_err(|e| format!("host db: {what} :: {e}"))
}

/// One open database. Calls serialise on the connection; every call is short.
pub(crate) struct HostDb {
    conn: Mutex<Connection>,
    pub(crate) state_dir: PathBuf,
}

type Registry = Mutex<HashMap<PathBuf, Arc<HostDb>>>;

fn registry() -> &'static Registry {
    static REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// The handle for `state_dir`, opened (schema, one-time import) on first use.
pub(crate) fn for_dir(state_dir: &str) -> Res<Arc<HostDb>> {
    let key = PathBuf::from(state_dir);
    let mut map = registry().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(db) = map.get(&key) {
        return Ok(db.clone());
    }
    let db = Arc::new(HostDb::open(&key)?);
    map.insert(key, db.clone());
    Ok(db)
}

impl HostDb {
    /// Open (creating the file 0600), set the pragmas, apply the schema and
    /// import whatever the old files still hold.
    pub(crate) fn open(state_dir: &Path) -> Res<HostDb> {
        std::fs::create_dir_all(state_dir)
            .map_err(|e| format!("host db: cannot create {} :: {e}", state_dir.display()))?;
        let path = state_dir.join(DB_FILE);
        let mut conn = why("open", Connection::open(&path))?;
        private(&path)?;
        why("pragmas", conn.execute_batch(PRAGMAS))?;
        schema::apply(&mut conn)?;
        import::all(&mut conn, state_dir)?;
        Ok(HostDb {
            conn: Mutex::new(conn),
            state_dir: state_dir.to_path_buf(),
        })
    }

    /// An in-memory database with the schema, for tests of a store alone.
    #[cfg(test)]
    pub(crate) fn memory() -> HostDb {
        let mut conn = Connection::open_in_memory().expect("memory db");
        schema::apply(&mut conn).expect("schema");
        HostDb {
            conn: Mutex::new(conn),
            state_dir: PathBuf::new(),
        }
    }

    /// Run `f` on the connection.
    pub(crate) fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Res<T>) -> Res<T> {
        let mut conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut conn)
    }
}

/// WAL + FULL: a cut leaves the last committed transaction or the one before.
const PRAGMAS: &str = "PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; \
                       PRAGMA busy_timeout = 5000;";

/// The file is private; sqlite gives `-wal` and `-shm` the same mode.
fn private(path: &Path) -> Res<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("host db: cannot make {} private :: {e}", path.display()))
}

/// Seconds since the epoch.
pub(crate) fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Rows in `table`; the import's before/after check reads it.
pub(crate) fn count(conn: &Connection, table: &str) -> Res<u64> {
    let n: i64 = why(
        "count",
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)),
    )?;
    Ok(n as u64)
}
