//! The incident half of the one-time import (see [`super::import`]): the
//! `report.json` and `events.jsonl` of every bundle directory become one row
//! each; every file is backed up, then renamed `*.migrated` after the commit.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::import::{Fill, backup, commit_import, finish};
use super::incidents::split_name;
use super::{Res, why};

const MARKER: &str = "incidents";
const FILES: [&str; 2] = ["report.json", "events.jsonl"];

/// (bundle name, directory) of every `<ts>-<op>` bundle that still holds an
/// old file.
fn pending(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir.join("incidents"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Some((e.file_name().to_str()?.to_string(), e.path())))
        .filter(|(n, p)| split_name(n).is_some() && FILES.iter().any(|f| p.join(f).is_file()))
        .collect();
    out.sort();
    out
}

fn text(p: &Path) -> Res<Option<String>> {
    match std::fs::read(p) {
        Ok(b) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("host db: cannot read {} :: {e}", p.display())),
    }
}

pub(super) fn run(conn: &mut Connection, dir: &Path) -> Res<()> {
    let bundles = pending(dir);
    let done: i64 = why(
        "marker",
        conn.query_row(
            "SELECT COUNT(*) FROM imports WHERE source = ?1",
            params![MARKER],
            |r| r.get(0),
        ),
    )?;
    if done == 0 {
        if bundles.is_empty() {
            return Ok(());
        }
        let mut loaded = Vec::new();
        for (name, p) in &bundles {
            for f in FILES {
                if p.join(f).is_file() {
                    backup(&p.join(f))?;
                }
            }
            loaded.push((
                name.clone(),
                text(&p.join(FILES[0]))?,
                text(&p.join(FILES[1]))?,
            ));
        }
        let read = commit_import(conn, MARKER, "incidents", |tx| {
            for (name, report, events) in &loaded {
                let (ts, op) = split_name(name).ok_or("host db: bundle name")?;
                why(
                    "incident import",
                    tx.execute(
                        "INSERT INTO incidents (name, ts, op, report, events) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![name, ts as i64, op, report, events],
                    ),
                )?;
            }
            Ok(Fill {
                rows: loaded.len() as u64,
                skipped: 0,
            })
        })?;
        tracing::info!(
            "incidents: {} bundle(s) moved into the host database",
            read.rows
        );
    }
    // After a commit (this run's or an earlier one's) the files of the bundles
    // the database holds are renamed. A bundle it does not hold (an old-format
    // bundle written after the move, by a rolled-back host) is left alone.
    for (name, p) in &bundles {
        let held: i64 = why(
            "incident held",
            conn.query_row(
                "SELECT COUNT(*) FROM incidents WHERE name = ?1",
                params![name],
                |r| r.get(0),
            ),
        )?;
        if held == 0 {
            tracing::warn!(
                "incidents: {name} still has old files the database does not hold; left alone"
            );
            continue;
        }
        for f in FILES.iter().filter(|f| p.join(f).is_file()) {
            finish(&p.join(f))?;
        }
    }
    Ok(())
}
