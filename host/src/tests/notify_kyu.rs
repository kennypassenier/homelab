//! kyu-notify through the host: one POST to a stub kyu, and the notice
//! written when kyu does not accept it.

use super::support::*;
use crate::*;
use std::io::{Read, Write};

fn stub_kyu(reply: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/t/homelab.ops", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = conn.read(&mut buf).unwrap();
        conn.write_all(reply.as_bytes()).unwrap();
        String::from_utf8_lossy(&buf[..n]).to_string()
    });
    (url, handle)
}

fn failed_backup_report() -> homelab_core::runner::OperationReport {
    homelab_core::runner::OperationReport {
        op: "backup-home".into(),
        steps: Vec::new(),
        ok: false,
        error: Some(homelab_core::error::OperatorError {
            what: "restic failed".into(),
            why: "repo locked".into(),
            remedy: "unlock it".into(),
        }),
        deferred: None,
        note: None,
    }
}

async fn failed_backup_notices(tag: &str, webhook: &str) -> Vec<serde_json::Value> {
    let dir =
        std::env::temp_dir().join(format!("homelab-kyunotify-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let hook = if webhook.is_empty() {
        String::new()
    } else {
        format!("notify_webhook = \"{}\"\n", webhook)
    };
    let raw = format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n{}\
         notify_fallback_webhook = \"http://127.0.0.1:9/never-used\"\n",
        dir.display(),
        hook
    );
    let state = test_state(config_from_text(&raw));
    let report = failed_backup_report();
    notify(
        &state,
        &RealExecutor,
        "scheduled-backup",
        &report,
        1,
        None,
        None,
    )
    .await;
    let text = std::fs::read_to_string(notices_path(&state.config.state_dir)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[tokio::test]
pub(crate) async fn kyu_notify_an_accepted_push_sends_the_key_once_and_writes_one_notice() {
    let (url, seen) =
        stub_kyu("HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let notices = failed_backup_notices("ok", &url).await;
    let head = seen.join().unwrap().to_ascii_lowercase();
    assert!(
        head.contains("kyu-dedup-key: scheduled-backup")
            || head.contains("kyu-dedup-key: backup-home"),
        "{head}"
    );
    assert!(head.contains("kyu-dedup-window: 20h"), "{head}");
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["push"], "sent");
}

#[tokio::test]
pub(crate) async fn kyu_notify_a_refused_push_adds_a_could_not_be_delivered_notice_and_no_retry() {
    // The stub answers exactly one request: a second POST would hang the
    // test on a closed listener, so a retry or fallback would show here.
    let (url, seen) =
        stub_kyu("HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let notices = failed_backup_notices("refused", &url).await;
    seen.join().unwrap();
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert_eq!(notices[0]["push"], "failed: HTTP 503");
    assert_eq!(notices[1]["title"], "Notification could not be delivered");
    assert!(notices[1]["what"].as_str().unwrap().contains("HTTP 503"));
}

#[tokio::test]
pub(crate) async fn kyu_notify_an_unconfigured_host_stays_quiet_and_says_so_in_the_push_column() {
    let notices = failed_backup_notices("unset", "").await;
    assert_eq!(notices.len(), 1, "{notices:?}");
    let push = notices[0]["push"].as_str().unwrap();
    assert!(push.contains("not configured"), "{push}");
}

#[tokio::test]
pub(crate) async fn kyu_notify_a_dead_kyu_keeps_one_undelivered_notice_with_a_count() {
    // Nothing listens on a freed port: both pushes fail at connect.
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/t/x", l.local_addr().unwrap())
    };
    let dir = std::env::temp_dir().join(format!("homelab-kyunotify-fold-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let raw = format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\nnotify_webhook = \"{}\"\n",
        dir.display(),
        dead
    );
    let state = test_state(config_from_text(&raw));
    for _ in 0..3 {
        let report = failed_backup_report();
        notify(
            &state,
            &RealExecutor,
            "scheduled-backup",
            &report,
            1,
            None,
            None,
        )
        .await;
    }
    let text = std::fs::read_to_string(notices_path(&state.config.state_dir)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let lost: Vec<_> = text
        .lines()
        .filter(|l| l.contains("Notification could not be delivered"))
        .collect();
    assert_eq!(lost.len(), 1, "{text}");
    assert!(lost[0].contains("3 pushes"), "{}", lost[0]);
    assert_eq!(text.lines().count(), 4, "three originals plus one folded");
}
