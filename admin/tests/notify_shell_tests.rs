//! Decision "Notifications and Grafana" (2026-09-30), the shell half: the
//! host's notices read after a cursor, Alertmanager's hook with its own
//! bearer token, and the 09:00 digest pushed once, only when something
//! waits. No kyu, no host: pushes go to a recorder.

mod act_support;

use std::sync::Arc;

use act_support::{temp_dir, RecPusher, Recorder, TestClock};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use homelab_admin::core::notify::{DigestLine, Level, PushOutcome};
use homelab_admin::shell::actions_notify::{hooks_router, HookSlot, NotifyCenter};
use homelab_core::notify::HostNotice;
use tower::ServiceExt as _;

fn notice(seq: u64, ok: bool, req: Option<u64>) -> HostNotice {
    HostNotice {
        seq,
        at: 1_000,
        since: 990,
        op: "backup-media".into(),
        label: "scheduled-backup".into(),
        ok,
        deferred: false,
        stack: Some("media".into()),
        title: if ok {
            "scheduled-backup media: done".into()
        } else {
            "scheduled-backup media failed".into()
        },
        what: String::new(),
        consequence: "c".into(),
        remedy: "Run it again: `homelab backup media`.".into(),
        page: "/app/stacks/media".into(),
        urgent: !ok,
        routed: "r".into(),
        push: if ok {
            "centre only".into()
        } else {
            "sent".into()
        },
        incident: None,
        req,
        by: None,
        findings: Vec::new(),
    }
}

fn center(tag: &str, clock: &TestClock) -> (Arc<NotifyCenter>, Arc<RecPusher>, Arc<Recorder>) {
    let dir = temp_dir(tag);
    let pusher = Arc::new(RecPusher::default());
    let live = Arc::new(Recorder::default());
    let c = NotifyCenter::load(
        dir.join("notifications.json"),
        pusher.clone(),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    // app-knowledge (2026-09-30): the public address is configuration.
    c.set_base_url("https://dash.example");
    (c, pusher, live)
}

#[tokio::test]
async fn host_notices_start_from_now_then_arrive_once_and_merge_into_jobs() {
    let clock = TestClock::at(2_000);
    let (c, pusher, live) = center("hostnotices", &clock);
    let none = |_req: u64| None;
    // The first read: what the host kept before is history.
    assert_eq!(
        c.host_notices(vec![notice(5, true, None)], 5, &none).await,
        0
    );
    assert_eq!(c.snapshot().await["notices"].as_array().unwrap().len(), 0);
    // Then every new one, once.
    assert_eq!(
        c.host_notices(
            vec![notice(5, true, None), notice(6, false, None)],
            6,
            &none
        )
        .await,
        1
    );
    assert_eq!(
        c.host_notices(vec![notice(6, false, None)], 6, &none).await,
        0
    );
    let snap = c.snapshot().await;
    let n = &snap["notices"][0];
    assert_eq!(n["kind"], "host_event");
    assert_eq!(n["level"], "critical");
    assert_eq!(n["since"], 990);
    assert_eq!(n["link"], "/app/stacks/media");
    assert_eq!(n["fixes"][0]["action"], "backup");
    assert_eq!(n["push"]["state"], "by_sender");
    assert!(pusher.sent.lock().unwrap().is_empty(), "the host pushed it");
    assert_eq!(live.events("notification").last().unwrap()["pop_up"], true);
    // A request a dashboard job sent: that job's notice takes the words.
    let mut own = homelab_admin::core::notify::Draft::new(
        homelab_admin::core::notify::Kind::ActionFailed,
        "backup-media",
        "Back up media: failed",
        "x",
    );
    own.job = Some(9);
    c.notify(own).await;
    let job_of = |req: u64| (req == 4242).then_some(9);
    assert_eq!(
        c.host_notices(vec![notice(7, false, Some(4242))], 7, &job_of)
            .await,
        1
    );
    let snap = c.snapshot().await;
    assert_eq!(
        snap["notices"].as_array().unwrap().len(),
        2,
        "merged, not added"
    );
    assert_eq!(snap["notices"][0]["job"], 9);
    assert_eq!(
        snap["notices"][0]["remedy"],
        "Run it again: `homelab backup media`."
    );
}

fn alert_body(status: &str) -> String {
    serde_json::json!({
        "status": status,
        "alerts": [{
            "status": status,
            "labels": {"alertname": "FilesystemAlmostFull", "severity": "warning", "device": "/dev/sda1"},
            "annotations": {"summary": "/dev/sda1 is over 90% full", "description": "d",
                "consequence": "writes fail", "remedy": "free space",
                "click_url": "https://admin.kp-soft.dev/app/checks"},
            "startsAt": "2026-09-30T07:00:00.123Z",
            "fingerprint": "abc"
        }]
    })
    .to_string()
}

async fn post(app: axum::Router, auth: Option<&str>, body: String) -> (StatusCode, String) {
    let mut req = Request::post("/hooks/alertmanager").header("content-type", "application/json");
    if let Some(a) = auth {
        req = req.header("authorization", a);
    }
    let resp = app
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into())
}

#[tokio::test]
async fn the_alertmanager_hook_takes_its_own_token_and_stores_alerts() {
    let clock = TestClock::at(2_000);
    let (c, _pusher, _live) = center("hook", &clock);
    let slot = HookSlot::default();
    let app = hooks_router(slot.clone());
    // Before the centre runs: retry later.
    let (st, _) = post(app.clone(), Some("Bearer s3cret"), alert_body("firing")).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    slot.fill(c.clone(), Some("s3cret".into()));
    let (st, _) = post(app.clone(), None, alert_body("firing")).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = post(app.clone(), Some("Bearer wrong"), alert_body("firing")).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = post(app.clone(), Some("Bearer s3cret"), "not json".into()).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, body) = post(app.clone(), Some("Bearer s3cret"), alert_body("firing")).await;
    assert_eq!((st, body.as_str()), (StatusCode::OK, r#"{"stored":1}"#));
    let snap = c.snapshot().await;
    let n = &snap["notices"][0];
    assert_eq!(n["kind"], "alert");
    assert_eq!(n["level"], "critical", "a disk almost full is urgent");
    assert_eq!(n["since"], 1_790_751_600);
    assert_eq!(n["consequence"], "writes fail");
    assert_eq!(n["link"], "/app/checks");
    assert_eq!(n["push"]["who"], "Alertmanager");
    let (_, body) = post(app.clone(), Some("Bearer s3cret"), alert_body("resolved")).await;
    assert_eq!(body, r#"{"stored":1}"#);
    assert_eq!(c.snapshot().await["unread"], 0, "resolved: nothing waits");
    // No token configured: the hook refuses everything.
    let (c2, _, _) = center("hook2", &clock);
    let slot2 = HookSlot::default();
    slot2.fill(c2, None);
    let (st, _) = post(hooks_router(slot2), Some("Bearer "), alert_body("firing")).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn the_digest_goes_out_once_at_nine_only_when_something_waits() {
    // 2026-09-30 09:00 in Brussels.
    let nine = 1_790_751_600;
    let clock = TestClock::at(nine - 600);
    let (c, pusher, live) = center("digest", &clock);
    let no_items = || async { Ok::<Vec<DigestLine>, String>(Vec::new()) };
    assert!(c.digest_tick(no_items).await.is_none(), "not yet 09:00");
    clock.set(nine + 30);
    // All clear: recorded, nothing pushed.
    let rec = c.digest_tick(no_items).await.unwrap();
    assert!(matches!(rec.push, PushOutcome::Skipped { .. }), "{rec:?}");
    assert!(pusher.sent.lock().unwrap().is_empty());
    // The next day something waits: one push, worst first, with the link.
    clock.set(nine + 86_400 + 30);
    let item = || async {
        Ok::<Vec<DigestLine>, String>(vec![DigestLine {
            level: Level::Critical,
            text: "backup of media failed".into(),
        }])
    };
    let rec = c.digest_tick(item).await.unwrap();
    assert_eq!((rec.count, rec.push.clone()), (1, PushOutcome::Sent));
    let sent = pusher.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    let p: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(p["click_url"], "https://dash.example/app/notifications");
    assert!(p["error"]
        .as_str()
        .unwrap()
        .contains("backup of media failed"));
    assert_eq!(p["ok"], false, "so Home Assistant pushes it");
    // Once a day.
    clock.advance(600);
    assert!(c.digest_tick(item).await.is_none());
    assert_eq!(live.events("notify_digest").len(), 2);
    assert_eq!(c.snapshot().await["last_digest"]["count"], 1);
}

/// Kenny, 2026-09-30 09:16: Today's items and the fleet check's findings
/// carry the Fix their remedy names.
#[test]
fn today_and_findings_rows_carry_their_fix() {
    let rows = homelab_admin::shell::parity::with_fixes(serde_json::json!([
        {"what": "no backup for 3 days", "remedy": "homelab backup media"},
        {"what": "drift", "remedy": "edit the stack file"},
    ]));
    assert_eq!(rows[0]["fix"]["action"], "backup");
    assert_eq!(rows[0]["fix"]["stack"], "media");
    assert_eq!(rows[0]["fix"]["label"], "Back up");
    assert!(rows[1].get("fix").is_none());
}
