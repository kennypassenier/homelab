//! kyu-notify: the host's one POST to kyu, the dedup headers, and the notice
//! for a push kyu did not accept. A stub HTTP server on a loopback port
//! plays kyu; nothing leaves the machine.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

use homelab_core::notify::{Delivery, HostNotice, verdict};
use homelab_core::notify_kyu::{
    DEDUP_KEY_MAX, curl_args_kyu, dedup_key, fold_undelivered, is_undelivered_for, sent_text,
    undelivered_count, undelivered_notice, was_duplicate,
};

/// Answers one request with `reply` (a full HTTP response) and hands the
/// request head back, so the test can read the headers curl sent.
fn stub(reply: &'static str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
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

fn post(url: &str, key: Option<&str>) -> String {
    let args = curl_args_kyu("{\"ok\":false}", url, None, None, key);
    let out = Command::new("curl").args(&args).output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn kyu_notify_a_failure_carries_the_dedup_headers_and_a_201_is_delivered() {
    let (url, seen) =
        stub("HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let key = dedup_key("backup", false, Some("disk vol")).unwrap();
    assert_eq!(key, "backup|disk vol");
    let out = post(&url, Some(&key));
    let head = seen.join().unwrap().to_ascii_lowercase();
    assert!(head.contains("kyu-dedup-key: backup|disk vol"), "{head}");
    assert!(head.contains("kyu-dedup-window: 20h"), "{head}");
    assert_eq!(verdict(true, &out), Delivery::Delivered);
    assert!(!was_duplicate(&out));
    assert_eq!(sent_text(&out), "sent");
}

#[test]
fn kyu_notify_a_200_with_kyu_duplicate_counts_as_delivered_and_says_so() {
    let (url, _seen) = stub(
        "HTTP/1.1 200 OK\r\nKyu-Duplicate: true\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let out = post(&url, Some("backup|disk vol"));
    assert_eq!(verdict(true, &out), Delivery::Delivered);
    assert!(was_duplicate(&out), "{out:?}");
    assert!(sent_text(&out).contains("already holds"));
}

#[test]
fn kyu_notify_a_success_message_sends_no_key() {
    assert_eq!(dedup_key("backup", true, None), None);
    let (url, seen) =
        stub("HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    post(&url, None);
    let head = seen.join().unwrap().to_ascii_lowercase();
    assert!(!head.contains("kyu-dedup"), "{head}");
}

#[test]
fn kyu_notify_a_refusal_and_a_dead_port_are_both_failures() {
    let (url, _seen) = stub("HTTP/1.1 500 Oops\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    assert!(matches!(
        verdict(true, &post(&url, None)),
        Delivery::Failed(w) if w == "HTTP 500"
    ));
    // Nothing listens on a port that was just freed.
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}/t/x", l.local_addr().unwrap())
    };
    assert!(matches!(
        verdict(true, &post(&dead, None)),
        Delivery::Failed(_)
    ));
}

#[test]
fn kyu_notify_keys_are_printable_ascii_within_the_limit_and_stay_distinct() {
    let k = dedup_key("deploy", false, Some("caf\u{e9}\nline")).unwrap();
    assert!(k.chars().all(|c| (' '..='~').contains(&c)), "{k}");
    let long_a = "x".repeat(400) + "a";
    let long_b = "x".repeat(400) + "b";
    let (a, b) = (
        dedup_key("op", false, Some(&long_a)).unwrap(),
        dedup_key("op", false, Some(&long_b)).unwrap(),
    );
    assert!(a.len() <= DEDUP_KEY_MAX && b.len() <= DEDUP_KEY_MAX);
    assert_ne!(a, b);
}

#[test]
fn kyu_notify_an_undelivered_push_becomes_a_quiet_notice_naming_the_cause() {
    let failed: HostNotice = serde_json::from_value(serde_json::json!({
        "seq": 1, "at": 10, "since": 10, "op": "backup", "label": "backup", "ok": false,
        "title": "Backup failed", "what": "w", "consequence": "c", "remedy": "r",
        "page": "/stacks/media", "urgent": true, "routed": "x", "push": "failed: HTTP 500"
    }))
    .unwrap();
    let n = undelivered_notice(2, 11, &failed, "HTTP 500", "kyu:80");
    assert_eq!(n.title, "Notification could not be delivered");
    assert!(n.what.contains("Backup failed") && n.what.contains("HTTP 500"));
    assert!(
        !n.urgent,
        "must not push through the route that just failed"
    );
    assert_eq!(n.seq, 2);
}

#[test]
fn kyu_notify_the_dedup_headers_sit_right_before_the_payload_and_url() {
    let args = curl_args_kyu("{\"a\":1}", "http://k/t", Some("/h"), None, Some("k1"));
    let d = args.iter().rposition(|a| a == "-d").unwrap();
    assert_eq!(&args[d + 1..], ["{\"a\":1}", "http://k/t"]);
    assert_eq!(
        &args[d - 4..d],
        ["-H", "Kyu-Dedup-Key: k1", "-H", "Kyu-Dedup-Window: 20h"]
    );
    assert_eq!(args.iter().filter(|a| *a == "Kyu-Dedup-Key: k1").count(), 1);
}

#[test]
fn kyu_notify_repeated_undelivered_notices_fold_into_one_with_a_count() {
    let failed: HostNotice = serde_json::from_value(serde_json::json!({
        "seq": 1, "at": 10, "since": 10, "op": "backup", "label": "backup", "ok": false,
        "title": "Backup failed", "what": "w", "consequence": "c", "remedy": "r",
        "page": "/p", "urgent": true, "routed": "x", "push": "failed: HTTP 500"
    }))
    .unwrap();
    let first = undelivered_notice(2, 11, &failed, "HTTP 500", "kyu:80");
    assert!(is_undelivered_for(&first, "kyu:80") && !is_undelivered_for(&first, "other:80"));
    assert_eq!(undelivered_count(&first), 1);
    let again = undelivered_notice(5, 50, &failed, "HTTP 503", "kyu:80");
    let folded = fold_undelivered(&first, again, "HTTP 503", "Backup failed");
    assert_eq!((folded.seq, folded.at, folded.since), (5, 50, 11));
    assert_eq!(undelivered_count(&folded), 2);
    assert!(folded.what.contains("2 pushes") && folded.what.contains("HTTP 503"));
}
