//! host-db-1: the host database — import of the old files, crash safety,
//! caps, and each reader and writer on the new store.

use crate::hostdb::{HostDb, import, incidents, journal, notices, watched};
use crate::tests::support::{config_from_text, test_state};
use crate::*;

fn tmp(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "homelab-hostdb-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn notice(seq: u64, at: u64) -> homelab_core::notify::HostNotice {
    serde_json::from_value(serde_json::json!({
        "seq": seq, "at": at, "since": at, "op": "deploy-x", "label": "deploy", "ok": true,
        "title": "t", "what": "w", "consequence": "c", "remedy": "r", "page": "/",
        "urgent": false, "routed": "r", "push": "centre only"
    }))
    .unwrap()
}

fn jl(ts: u64, op: &str, step: &str, status: &str) -> String {
    format!("{{\"ts\":{ts},\"op\":\"{op}\",\"step\":\"{step}\",\"status\":\"{status}\"}}\n")
}

fn old_files(dir: &std::path::Path) {
    let journal = jl(10, "deploy-a", "s1", "running")
        + "{torn line\n"
        + &jl(11, "deploy-a", "s1", "ok")
        + &jl(12, "deploy-b", "s2", "running");
    std::fs::write(dir.join("journal.jsonl"), journal).unwrap();
    let ns = format!(
        "{}\nnot json\n{}\n",
        serde_json::to_string(&notice(5, 100)).unwrap(),
        serde_json::to_string(&notice(9, 200)).unwrap()
    );
    std::fs::write(dir.join("notices.jsonl"), ns).unwrap();
    std::fs::write(
        dir.join("watched-backups.json"),
        r#"{"router":{"rclone_path":"gdrive:r","newest_unix":42,"learned_at":50,"source":"nightly"}}"#,
    )
    .unwrap();
    let b = dir.join("incidents/1800000000-deploy-media");
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(b.join("report.json"), "{\"op\":\"deploy-media\"}").unwrap();
    std::fs::write(b.join("events.jsonl"), "{\"e\":1}\n").unwrap();
    std::fs::write(b.join("versions.txt"), "host=1").unwrap();
}

/// Every old file lands, in order and with its timestamps; a malformed line is
/// skipped (and stays in the backup); backups and `.migrated` files exist.
#[test]
fn host_db_1_import_round_trip_keeps_rows_order_and_backups() {
    let dir = tmp("import");
    old_files(&dir);
    let db = HostDb::open(&dir).unwrap();
    let text = journal::text(&db).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(lines[0].contains("\"ts\":10") && lines[2].contains("deploy-b"));
    assert_eq!(
        journal::interrupted(&db).unwrap(),
        vec![("deploy-b".to_string(), "s2".to_string())]
    );
    let all = notices::all(&db).unwrap();
    assert_eq!(
        all.iter().map(|n| (n.seq, n.at)).collect::<Vec<_>>(),
        vec![(5, 100), (9, 200)]
    );
    let w = watched::load(&db).unwrap();
    assert_eq!(
        (w["router"].newest_unix, w["router"].learned_at),
        (Some(42), 50)
    );
    let (report, events) = incidents::get(&db, "1800000000-deploy-media")
        .unwrap()
        .unwrap();
    assert_eq!(
        (report.as_deref(), events.as_deref()),
        (Some("{\"op\":\"deploy-media\"}"), Some("{\"e\":1}\n"))
    );
    for f in [
        "journal.jsonl",
        "notices.jsonl",
        "watched-backups.json",
        "incidents/1800000000-deploy-media/report.json",
        "incidents/1800000000-deploy-media/events.jsonl",
    ] {
        let p = dir.join(f);
        assert!(!p.exists(), "{f} is renamed");
        assert!(dir.join(format!("{f}.migrated")).exists(), "{f}.migrated");
        assert!(
            dir.join(format!("{f}.pre-db-backup")).exists(),
            "{f}.pre-db-backup"
        );
    }
    let kept = std::fs::read_to_string(dir.join("journal.jsonl.pre-db-backup")).unwrap();
    assert!(
        kept.contains("{torn line"),
        "the skipped line stays in the backup"
    );
    assert!(
        dir.join("incidents/1800000000-deploy-media/versions.txt")
            .exists()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A cut before the commit: backup exists, no marker, the file is intact; the
/// next start imports once. A cut after the commit: marker present, file not
/// renamed; the next start only renames, never imports twice. An existing
/// backup is never overwritten.
#[test]
fn host_db_1_a_cut_midway_reruns_safely_and_never_duplicates() {
    let dir = tmp("crash");
    old_files(&dir);
    std::fs::write(dir.join("journal.jsonl.pre-db-backup"), "keep me").unwrap();
    import::backup(&dir.join("notices.jsonl")).unwrap(); // cut after the backup, before the commit
    drop(HostDb::open(&dir).unwrap());
    assert_eq!(
        std::fs::read_to_string(dir.join("journal.jsonl.pre-db-backup")).unwrap(),
        "keep me"
    );
    // Cut after the commit: put the journal file back as if the rename never ran.
    std::fs::rename(
        dir.join("journal.jsonl.migrated"),
        dir.join("journal.jsonl"),
    )
    .unwrap();
    let db = HostDb::open(&dir).unwrap();
    assert_eq!(
        journal::text(&db).unwrap().lines().count(),
        3,
        "no duplicate rows"
    );
    assert_eq!(notices::all(&db).unwrap().len(), 2);
    assert!(!dir.join("journal.jsonl").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Journal compaction keeps the newest rows and the last record of every
/// operation still running; the other tables stay within their caps.
#[test]
fn host_db_1_each_table_stays_within_its_cap() {
    let db = HostDb::memory();
    journal::record(&db, 1, "long-op", "s", "running").unwrap();
    for i in 0..500 {
        journal::record(&db, 2 + i, "op", "s", "ok").unwrap();
    }
    assert!(
        journal::compact(&db, 1 << 20).unwrap().is_none(),
        "under the cap: untouched"
    );
    let (before, after) = journal::compact(&db, 8_000).unwrap().unwrap();
    assert!(after < before && after <= 8_000, "{before} -> {after}");
    assert_eq!(
        journal::interrupted(&db).unwrap().len(),
        1,
        "the running op survives the cut"
    );
    for seq in 1..=60 {
        notices::append(&db, &notice(seq, 1_000 + seq), 2_000, 10_000_000, 2_000).unwrap();
    }
    let kept = notices::all(&db).unwrap();
    assert!(
        kept.len() < 60 && kept.last().unwrap().seq == 60,
        "{} notices",
        kept.len()
    );
    let records: homelab_core::ops::watched::WatchedRecords = (0..1_005)
        .map(|i| {
            (
                format!("w{i:04}"),
                homelab_core::ops::watched::WatchedRecord {
                    rclone_path: "p".into(),
                    newest_unix: None,
                    learned_at: i,
                    source: "t".into(),
                },
            )
        })
        .collect();
    watched::save(&db, &records).unwrap();
    let left = watched::load(&db).unwrap();
    assert_eq!(left.len() as i64, watched::MAX_ROWS);
    assert!(left.contains_key("w1004") && !left.contains_key("w0000"));
}

/// Incidents: pruned by age and count, row and directory together; listed as
/// the union of rows and directories.
#[test]
fn host_db_1_incident_rows_list_prune_and_show() {
    let dir = tmp("inc");
    let d = dir.to_string_lossy().to_string();
    let now = 1_800_000_000u64;
    let db = crate::hostdb::for_dir(&d).unwrap();
    for (age, op) in [(100 * 86_400, "old"), (86_400, "young")] {
        incidents::save(&db, &format!("{}-{op}", now - age), "{}", "").unwrap();
    }
    std::fs::create_dir_all(dir.join(format!("incidents/{}-dironly", now - 5))).unwrap();
    assert_eq!(incident_names(&d).unwrap().len(), 3);
    assert_eq!(prune_incidents(&d, now, 90, 200), 1);
    assert_eq!(incident_names(&d).unwrap().len(), 2);
    assert_eq!(
        prune_incidents(&d, now, 90, 1),
        1,
        "count cap keeps the newest"
    );
    let shown = incident_text(&d, &format!("{}-dironly", now - 5)).unwrap();
    assert!(shown.contains("incident "), "{shown}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The writers and readers other code uses: `DbJournal`, `record_notice`, the
/// `Notices` rpc, and core's `write_bundle` / watched record through `records()`.
#[tokio::test]
async fn host_db_1_writers_and_readers_agree() {
    use homelab_core::executor::Executor as _;
    let dir = tmp("rw");
    let d = dir.to_string_lossy().to_string();
    let j = DbJournal {
        state_dir: d.clone(),
    };
    j.record("deploy-a", "pull", "running");
    let db = crate::hostdb::for_dir(&d).unwrap();
    assert_eq!(journal::interrupted(&db).unwrap()[0].0, "deploy-a");
    assert_eq!(
        RealExecutor.records().journal_tail(&d, 10).unwrap().len(),
        1
    );

    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{d}\"\n"
    )));
    for seq in [7, 8, 9] {
        record_notice(&state, &notice(seq, unix_now()));
    }
    let req = RpcRequest {
        id: 1,
        command: Rpc::Notices { after: 7, limit: 1 },
    };
    let got = handle_rpc(&state, req).await;
    let v: serde_json::Value = serde_json::from_str(&got.message).unwrap();
    assert_eq!(
        (v["notices"][0]["seq"].as_u64(), v["last_seq"].as_u64()),
        (Some(8), Some(9)),
        "{}",
        got.message
    );

    let report = homelab_core::runner::OperationReport {
        op: "deploy-x".into(),
        ok: false,
        steps: vec![],
        error: None,
        deferred: None,
        note: None,
    };
    homelab_core::incidents::write_bundle(&RealExecutor, &d, 1_800_000_000, &report, &[], "v")
        .await
        .unwrap();
    let (r, e) = incidents::get(&db, "1800000000-deploy-x").unwrap().unwrap();
    assert!(r.unwrap().contains("deploy-x") && e.unwrap().is_empty());
    assert!(
        dir.join("incidents/1800000000-deploy-x/journal-tail.jsonl")
            .exists()
    );
    assert!(
        !dir.join("incidents/1800000000-deploy-x/report.json")
            .exists()
    );

    let rec = homelab_core::ops::watched::WatchedRecord {
        rclone_path: "p".into(),
        newest_unix: Some(3),
        learned_at: 4,
        source: "check".into(),
    };
    let one = [("w".to_string(), rec.clone())].into();
    homelab_core::ops::watched::save(&RealExecutor, &d, &one)
        .await
        .unwrap();
    assert_eq!(
        homelab_core::ops::watched::load(&RealExecutor, &d)
            .await
            .unwrap()["w"],
        rec
    );
    let _ = std::fs::remove_dir_all(&dir);
}
