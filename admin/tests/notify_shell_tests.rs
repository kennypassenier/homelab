//! Decision "Notifications and Grafana" (2026-09-30), the shell half: the
//! host's notices read after a cursor, Alertmanager's hook with its own
//! bearer token, and the 09:00 digest pushed once, only when something
//! waits. No kyu, no host: pushes go to a recorder.

mod act_support;

use std::sync::Arc;

use act_support::{RecPusher, Recorder, TestClock};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use homelab_admin::core::notify::{DigestLine, Draft, Kind, Level, PushOutcome};
use homelab_admin::shell::actions_notify::{HookSlot, NotifyCenter, hooks_router};
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
        page: "/stacks/media".into(),
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

fn center(_tag: &str, clock: &TestClock) -> (Arc<NotifyCenter>, Arc<RecPusher>, Arc<Recorder>) {
    let pusher = Arc::new(RecPusher::default());
    let live = Arc::new(Recorder::default());
    let c = NotifyCenter::load(
        homelab_admin::shell::store::memory_db(),
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
    assert_eq!(n["link"], "/stacks/media");
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
                "click_url": "https://admin.kp-soft.dev/health?block=checks"},
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
    assert_eq!(n["link"], "/health?block=checks");
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
    assert_eq!(p["click_url"], "https://dash.example/notifications");
    assert!(
        p["error"]
            .as_str()
            .unwrap()
            .contains("backup of media failed")
    );
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

/// ask-kp11be-10: a test push goes through the push route, stores no
/// notice, ignores a push switch that is off but says so, and cannot be
/// pressed twice in a row.
#[tokio::test]
async fn a_test_push_reaches_the_route_stores_nothing_and_waits_between_presses() {
    use homelab_admin::shell::actions_notify::{TEST_PUSH_EVERY_S, TestPushError};
    let clock = TestClock::at(5_000);
    let (c, pusher, _live) = center("testpush", &clock);
    let before = c.snapshot().await["notices"].as_array().map(Vec::len);
    let first = c.test_push().await.unwrap();
    assert_eq!(first["sent"], true);
    assert_eq!(first["pushes_on"], true);
    let sent = pusher.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].contains("test-push"), "{}", sent[0]);
    assert_eq!(
        c.snapshot().await["notices"].as_array().map(Vec::len),
        before
    );
    // Too soon: nothing more is sent.
    clock.advance(3);
    assert_eq!(
        c.test_push().await.unwrap_err(),
        TestPushError::TooSoon(TEST_PUSH_EVERY_S - 3)
    );
    assert_eq!(pusher.sent.lock().unwrap().len(), 1);
    clock.advance(TEST_PUSH_EVERY_S);
    assert!(c.test_push().await.is_ok());
}

fn plain(kind: Kind, op: &str, stack: Option<&str>, label: Option<&str>) -> Draft {
    let mut d = Draft::new(kind, op, "t", "b");
    d.stack = stack.map(str::to_string);
    d.detail.label = label.map(str::to_string);
    d
}

/// feat-overview-5 on the database: ids count up, read and unread, the
/// badge per stack, and the push column's few fixed words.
#[tokio::test]
async fn the_list_keeps_read_and_unread_with_ids_that_count_up() {
    let clock = TestClock::at(10);
    let (c, _, _) = center("list", &clock);
    let a = c
        .notify(plain(Kind::ActionDone, "deploy-media", Some("media"), None))
        .await;
    clock.advance(1);
    let b = c
        .notify(plain(Kind::ActionFailed, "deploy-home", Some("home"), None))
        .await;
    assert_eq!((a.id, b.id), (1, 2));
    // Owner decision 2026-09-30 (item 3): the push column is a handful of
    // fixed words, never the raw routing reason.
    assert!(a.push_short.starts_with("No push"), "{}", a.push_short);
    let snap = c.snapshot().await;
    assert_eq!(
        (snap["unread"].as_u64(), snap["total"].as_u64()),
        (Some(2), Some(2))
    );
    assert_eq!(snap["notices"][0]["id"], 2, "newest first");
    assert_eq!(c.mark(Some(vec![1]), true).await, 1);
    assert_eq!(c.mark(Some(vec![1]), true).await, 0, "already read");
    let snap = c.snapshot().await;
    assert_eq!(snap["unread_by_stack"]["home"], 1);
    assert!(snap["unread_by_stack"].get("media").is_none());
    assert_eq!(c.mark(None, true).await, 1);
    assert_eq!(c.snapshot().await["unread"], 0);
    assert_eq!(c.mark(None, false).await, 2);
    let json = &c.snapshot().await["notices"][0];
    for k in ["id", "at", "kind", "title", "body", "read", "push"] {
        assert!(json.get(k).is_some(), "{k} in {json}");
    }
}

/// feat-kpadopt-2 (Kenny, 2026-10-21: never auto-prune useful data): the
/// old 500-notice and 180-day caps are gone. `keep` only sizes one list;
/// every notice stays, however old, until the Storage page deletes it.
#[tokio::test]
async fn no_notice_falls_off_on_its_own() {
    use homelab_admin::shell::actions_notify::NotifyLimits;
    let clock = TestClock::at(0);
    let (c, _, _) = center("nocap", &clock);
    c.set_limits(NotifyLimits {
        keep: 3,
        ..NotifyLimits::default()
    });
    for i in 0..6 {
        // A year apart: far past the 180 days the old store kept.
        clock.set(i * 365 * 86_400);
        c.notify(plain(Kind::ActionDone, "x", None, None)).await;
    }
    let snap = c.snapshot().await;
    assert_eq!(snap["total"], 6, "nothing was deleted");
    let ids: Vec<u64> = snap["notices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids, [6, 5, 4], "a list shows the newest `keep`");
    assert_eq!(snap["unread"], 6, "the unread count is every notice");
    assert_eq!(
        c.mark(Some(vec![1]), true).await,
        1,
        "the oldest is still there"
    );
}

/// fix-inbox-resolve-1 (Kenny, 2026-10-09): "deploy media failed" settles
/// itself the next time media deploys well; another stack's or another
/// operation's failure stays.
#[tokio::test]
async fn a_success_settles_the_same_operations_failure_on_the_same_stack() {
    let clock = TestClock::at(10);
    let (c, _, _) = center("settle", &clock);
    let failed = c
        .notify(plain(
            Kind::ActionFailed,
            "backup-media",
            Some("media"),
            Some("backup"),
        ))
        .await;
    clock.advance(10);
    let missed = c
        .notify(plain(
            Kind::ScheduleMissed,
            "backup-media",
            Some("media"),
            Some("backup"),
        ))
        .await;
    clock.advance(10);
    let other_stack = c
        .notify(plain(
            Kind::ActionFailed,
            "backup-films",
            Some("films"),
            Some("backup"),
        ))
        .await;
    clock.advance(10);
    let deploy = c
        .notify(plain(
            Kind::ActionFailed,
            "deploy-media",
            Some("media"),
            Some("deploy"),
        ))
        .await;
    clock.advance(10);
    let done = c
        .notify(plain(
            Kind::ActionDone,
            "backup-media",
            Some("media"),
            Some("backup"),
        ))
        .await;
    let snap = c.snapshot().await;
    let by = |id: u64| {
        snap["notices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .cloned()
            .unwrap()
    };
    assert_eq!(by(failed.id)["read"], true);
    assert_eq!(by(failed.id)["resolved_by"], done.id);
    assert_eq!(
        by(missed.id)["read"],
        true,
        "a missed schedule of the same backup is settled too"
    );
    assert_eq!(
        by(other_stack.id)["read"],
        false,
        "another stack's failure stays"
    );
    assert_eq!(
        by(deploy.id)["read"],
        false,
        "another operation's failure stays"
    );
}

/// A Down still open and unread is kept, not repeated (fix-watch-1), and so
/// is the storage warning, which would otherwise repeat at every start.
#[tokio::test]
async fn an_open_down_and_an_open_storage_warning_are_refreshed_not_repeated() {
    let clock = TestClock::at(10);
    let (c, _, live) = center("open", &clock);
    let mut down = plain(Kind::Down, "watch", Some("media"), None);
    down.detail.key = Some("media".into());
    down.body = "first reason".into();
    let first = c.notify(down.clone()).await;
    down.body = "newer reason".into();
    let again = c.notify(down).await;
    assert_eq!(again.id, first.id);
    let snap = c.snapshot().await;
    assert_eq!(snap["total"], 1);
    assert_eq!(snap["notices"][0]["body"], "newer reason");
    assert_eq!(
        live.events("notification").len(),
        1,
        "a refresh tells no page"
    );
    // The warning for a budget past its warn line.
    let warn = homelab_admin::shell::store::warn_notice(42_000_000, 40_000_000, 50_000_000, false);
    let w1 = c.notify(warn.clone()).await;
    let w2 = c.notify(warn.clone()).await;
    assert_eq!(w1.id, w2.id, "a restart does not stack a second warning");
    assert_eq!(c.snapshot().await["total"], 2);
    // Once read it is history: the next warning is news again.
    c.mark(Some(vec![w1.id]), true).await;
    let w3 = c.notify(warn).await;
    assert_ne!(w3.id, w1.id);
    assert_eq!(w3.kind, Kind::StorageWarn);
    assert_eq!(w3.level, Level::Warning);
    assert_eq!(w3.link.as_deref(), Some("/storage"));
}

/// An alert that still fires adds nothing; its end is stored read and ends
/// the firing one; firing again afterwards is news.
#[tokio::test]
async fn alertmanager_repeats_add_nothing_and_a_resolve_ends_the_firing_notice() {
    let clock = TestClock::at(2_000);
    let (c, _, _) = center("alerts", &clock);
    let body = |s: &str| serde_json::from_str::<serde_json::Value>(&alert_body(s)).unwrap();
    assert_eq!(c.alerts(&body("firing")).await, 1);
    assert_eq!(c.alerts(&body("firing")).await, 0, "a 12 h repeat");
    assert_eq!(c.snapshot().await["unread"], 1);
    assert_eq!(c.alerts(&body("resolved")).await, 1);
    let snap = c.snapshot().await;
    assert_eq!(snap["unread"], 0);
    assert_eq!(snap["notices"][0]["read"], true);
    assert_eq!(snap["total"], 2);
    assert_eq!(c.alerts(&body("firing")).await, 1, "firing again is news");
}

/// The settings and the cursors live in the database: a muted stack, a
/// snooze (bounded), the first incident list that only seeds, and the host
/// cursor.
#[tokio::test]
async fn settings_snooze_and_cursors_are_kept() {
    let clock = TestClock::at(1_000);
    let (c, _, _) = center("state", &clock);
    c.set_stack_muted("media", true).await;
    assert_eq!(c.settings().await.muted_stacks, ["media"]);
    assert_eq!(c.snooze(3_600).await.unwrap(), Some(4_600));
    assert!(c.snapshot().await["snoozed"].as_bool().unwrap());
    assert!(c.snooze(8 * 86_400).await.is_err(), "a week at most");
    assert_eq!(
        c.settings().await.snooze_until,
        Some(4_600),
        "a refused snooze changes nothing"
    );
    assert_eq!(c.snooze(0).await.unwrap(), None);
    let mut s = c.settings().await;
    s.digest_at = Some("25:00".into());
    assert!(c.set_settings(s).await.is_err());
    // Incidents: the first list is history, a new bundle is news.
    assert_eq!(c.incidents(vec!["1800000000-deploy-media".into()]).await, 0);
    assert_eq!(
        c.incidents(vec![
            "1800000000-deploy-media".into(),
            "1800000500-backup-home".into()
        ])
        .await,
        1
    );
    assert_eq!(c.snapshot().await["notices"][0]["kind"], "incident");
    assert_eq!(c.host_cursor().await, 0);
    c.host_notices(vec![notice(5, true, None)], 5, &|_| None)
        .await;
    assert_eq!(c.host_cursor().await, 5);
}
