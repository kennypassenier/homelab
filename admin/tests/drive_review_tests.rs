//! drive-reach review (2026-10-03): "a wrong id never shows on Kenny's
//! screen, and Live view reaches every control". Each test names the
//! review finding it proves; every one is written against the dashboard's
//! JSON surface (the step answers, the live events, the routes), so it runs
//! unchanged against the code from before the fixes.

mod act_support;

use std::sync::Arc;
use std::time::Duration;

use act_support::{
    MemFiles, MockHost, Recorder, Script, TestClock, history, shared, temp_dir, until,
};
use axum::body::Body;
use axum::http::Request;
use homelab_admin::core::drive::TabAnswer;
use homelab_admin::shell::actions::{Actions, ActionsDeps};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::drive::{Driver, TAB_WAIT, router};
use homelab_proto::{Scope, UiStep};
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tower::ServiceExt as _;

fn driver(tag: &str) -> (Driver, Arc<Recorder>) {
    let clock = TestClock::at(1_790_000_000);
    let host = MockHost::start(
        clock.clone(),
        Arc::new(|_| Script::ok(&[("pull", 5)])),
        history("deploy-media", &[]),
        Arc::new(std::sync::Mutex::new(Vec::new())),
    );
    let live = Arc::new(Recorder::default());
    let dir = temp_dir(&format!("drive-review-{tag}"));
    let notify = NotifyCenter::load(
        dir.join("notifications.json"),
        Arc::new(act_support::RecPusher::default()),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    let shared = shared(&[("media", 106, None)]);
    let actions = Actions::start(ActionsDeps {
        host,
        publish: live.clone(),
        files: Arc::new(MemFiles::default()),
        shared: shared.clone(),
        notify,
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    });
    (
        Driver::new(actions, shared, live.clone(), clock.clock()),
        live,
    )
}

fn send(d: &Driver, step: UiStep) -> JoinHandle<Value> {
    let d = d.clone();
    tokio::spawn(async move { d.step("wsl", Scope::Operate, step).await })
}

fn click(control: &str, row: Option<&str>) -> UiStep {
    UiStep::Click {
        control: control.into(),
        row: row.map(str::to_string),
    }
}

/// A step from its JSON, as `homelab ui` sends it.
fn step_of(v: Value) -> UiStep {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{v} is no step here: {e}"))
}

fn answer(v: Value) -> TabAnswer {
    serde_json::from_value(v).unwrap()
}

/// The page-control steps the tabs were sent, in order.
fn tab_steps(live: &Recorder) -> Vec<Value> {
    live.events("drive")
        .into_iter()
        .filter(|e| e["kind"] == "step" && e["applied"] == true)
        .map(|e| e["step"].clone())
        .collect()
}

async fn waiting(d: &Driver, live: &Recorder, n: usize) -> u64 {
    until("the step went out to the tabs", || {
        tab_steps(live).len() >= n
    })
    .await;
    d.snapshot().seq
}

/// A tab opens a page-level dialog offering `controls` and `fields`.
async fn open_dialog(d: &Driver, live: &Recorder, controls: Value, fields: Value) {
    let pending = send(d, click("new-schedule", None));
    let seq = waiting(d, live, 1).await;
    d.claim(seq, "tab-a").unwrap();
    d.taken(
        seq,
        "tab-a",
        answer(json!({
            "ok": true, "page": "/activity", "dialog": "New schedule",
            "controls": controls, "fields": fields,
        })),
    )
    .unwrap();
    assert_eq!(pending.await.unwrap()["ok"], true);
}

/// covers: drive-reach review "older client". A name the catalog does not
/// know, sent by a client that skipped its own check, is answered to that
/// caller alone: nothing about it is published to the tabs (it showed on
/// Kenny's screen as a toast before).
#[tokio::test(start_paused = true)]
async fn drive_review_an_unknown_name_is_answered_to_the_caller_only() {
    let (d, live) = driver("unknown-quiet");
    let before = live.events("drive").len();
    let a = d
        .step("wsl", Scope::Operate, click("no-such-control", None))
        .await;
    assert_eq!(a["ok"], false, "{a}");
    let a = d
        .step(
            "wsl",
            Scope::Operate,
            UiStep::Goto {
                path: "/no-such-page".into(),
            },
        )
        .await;
    assert_eq!(a["ok"], false, "{a}");
    let after: Vec<Value> = live.events("drive")[before..].to_vec();
    assert!(after.is_empty(), "published to the tabs: {after:?}");
}

/// covers: drive-reach review "page-level dialog open" and H3. With a
/// page-level dialog open, a click or press is checked against the buttons
/// that dialog offers (by name or by label) before any tab is asked.
#[tokio::test(start_paused = true)]
async fn drive_review_inside_a_page_dialog_only_its_own_buttons_go_out() {
    let (d, live) = driver("dialog-check");
    open_dialog(
        &d,
        &live,
        json!([{"id": "save-schedule", "label": "Save"}, {"id": "", "label": "Cancel"}]),
        json!(["sched-at"]),
    )
    .await;
    let a = d
        .step(
            "wsl",
            Scope::Operate,
            UiStep::Press {
                button: "delete".into(),
            },
        )
        .await;
    assert_eq!(a["ok"], false, "{a}");
    let why = a["refusal"]["why"].as_str().unwrap_or_default();
    assert!(
        why.contains("there is no control delete in New schedule"),
        "{a}"
    );
    assert!(
        a["refusal"]["fix"]
            .as_str()
            .unwrap_or_default()
            .contains("save-schedule"),
        "{a}"
    );
    assert_eq!(tab_steps(&live).len(), 1, "the refused press reached a tab");
    // A label the dialog offers goes to the tab.
    let pending = send(&d, click("Cancel", None));
    let seq = waiting(&d, &live, 2).await;
    d.claim(seq, "tab-a").unwrap();
    d.taken(
        seq,
        "tab-a",
        answer(json!({"ok": true, "page": "/activity"})),
    )
    .unwrap();
    assert_eq!(pending.await.unwrap()["ok"], true);
}

/// covers: drive-reach review M5. Fields are checked like clicks: a page
/// field no page declares is refused with the closest; inside a page-level
/// dialog, a field it does not hold is refused naming the ones it has.
#[tokio::test(start_paused = true)]
async fn drive_review_fields_are_checked_like_clicks() {
    let (d, live) = driver("fields");
    let a = d
        .step(
            "wsl",
            Scope::Operate,
            UiStep::Type {
                field: "shel-line".into(),
                text: "ls".into(),
            },
        )
        .await;
    assert_eq!(a["ok"], false, "{a}");
    assert!(
        a["refusal"]["why"]
            .as_str()
            .unwrap_or_default()
            .contains("no page declares a field shel-line"),
        "{a}"
    );
    assert!(
        a["refusal"]["fix"]
            .as_str()
            .unwrap_or_default()
            .contains("shell-line"),
        "{a}"
    );
    assert!(tab_steps(&live).is_empty());
    open_dialog(&d, &live, json!([]), json!(["sched-at"])).await;
    let a = d
        .step(
            "wsl",
            Scope::Operate,
            UiStep::Pick {
                field: "sched-when".into(),
                value: "x".into(),
            },
        )
        .await;
    assert_eq!(a["ok"], false, "{a}");
    assert!(
        a["refusal"]["why"]
            .as_str()
            .unwrap_or_default()
            .contains("has no field sched-when"),
        "{a}"
    );
}

/// covers: drive-reach review H4. The refusal log keeps the verb, the
/// field and the length of what was typed, never the text itself.
#[tokio::test(start_paused = true)]
async fn drive_review_the_refusal_log_never_keeps_typed_text() {
    let (d, _live) = driver("redact");
    let typed = "hunter2-very-secret-value";
    for step in [
        UiStep::Type {
            field: "no-field".into(),
            text: typed.into(),
        },
        UiStep::Edit {
            field: "no-field".into(),
            text: typed.into(),
        },
    ] {
        let a = d.step("wsl", Scope::Operate, step).await;
        assert_eq!(a["ok"], false, "{a}");
    }
    let r = router(d.clone())
        .oneshot(
            Request::get("/data/drive/refusals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(!text.contains("hunter2"), "typed text kept: {text}");
    let v: Value = serde_json::from_slice(&body).unwrap();
    let first = &v["refusals"][0];
    assert_eq!(first["verb"], "type", "{v}");
    assert_eq!(first["field"], "no-field", "{v}");
    assert_eq!(first["text_len"], typed.len(), "{v}");
}

/// covers: drive-reach review M3. A `state` answer names the catalog by
/// hash and carries neither the catalog nor the refusals; `controls`
/// answers the catalog text that hash names; `refusals` the log.
#[tokio::test(start_paused = true)]
async fn drive_review_state_is_small_and_names_the_catalog_by_hash() {
    let (d, _live) = driver("state-small");
    let _ = d
        .step("wsl", Scope::Operate, click("nope-nope", None))
        .await;
    let s = d.step("wsl", Scope::Read, UiStep::State).await;
    assert!(s.get("catalog").is_none(), "the whole catalog in state");
    assert!(s.get("refusals").is_none(), "refusals in state");
    let hash = s["catalog_hash"].as_str().unwrap_or_default().to_string();
    assert_eq!(hash.len(), 64, "{s}");
    assert!(
        s.to_string().len() < 4096,
        "state is {} bytes",
        s.to_string().len()
    );
    let c = d
        .step("wsl", Scope::Read, step_of(json!({"do": "controls"})))
        .await;
    assert_eq!(c["hash"], hash.as_str(), "{c}");
    let text = c["text"].as_str().unwrap_or_default();
    let cat: Value = serde_json::from_str(text).unwrap_or_default();
    assert_eq!(cat["schema"], 1, "the catalog text names its schema");
    assert_eq!(c["schema"], 1);
    assert!(cat["controls"].as_array().is_some_and(|a| a.len() > 40));
    let r = d
        .step("wsl", Scope::Read, step_of(json!({"do": "refusals"})))
        .await;
    assert_eq!(r["refusals"]["total"], 1, "{r}");
}

/// covers: drive-reach review M4. A step the client refused itself is
/// counted in the dashboard's log as the verb and the name, and changes
/// nothing on screen.
#[tokio::test(start_paused = true)]
async fn drive_review_a_local_refusal_is_counted() {
    let (d, live) = driver("local");
    let a = d
        .step(
            "wsl",
            Scope::Read,
            step_of(json!({"do": "refused_locally", "verb": "click", "name": "new-schedul"})),
        )
        .await;
    assert_eq!(a["ok"], true, "{a}");
    assert!(tab_steps(&live).is_empty());
    let r = d
        .step("wsl", Scope::Read, step_of(json!({"do": "refusals"})))
        .await;
    assert_eq!(r["refusals"]["total"], 1, "{r}");
    assert_eq!(
        r["refusals"]["refusals"][0]["verb"], "refused_locally",
        "{r}"
    );
    assert_eq!(
        r["refusals"]["refusals"][0]["control"], "new-schedul",
        "{r}"
    );
}

/// covers: drive-reach review (15 s no-answer). An old name that became a
/// menu item is clicked as its control, and the press goes to the tab as
/// a step of its own (the tab no longer presses inside the click's budget).
#[tokio::test(start_paused = true)]
async fn drive_review_an_alias_with_a_press_is_two_tab_steps() {
    let (d, live) = driver("alias");
    let pending = send(&d, click("edit-schedule", Some("s1")));
    let seq = waiting(&d, &live, 1).await;
    d.claim(seq, "tab-a").unwrap();
    d.taken(
        seq,
        "tab-a",
        answer(json!({"ok": true, "page": "/activity", "dialog": "Schedule s1"})),
    )
    .unwrap();
    let seq = waiting(&d, &live, 2).await;
    assert_eq!(
        tab_steps(&live)[1],
        json!({"do": "press", "button": "edit"})
    );
    d.claim(seq, "tab-a").unwrap();
    d.taken(
        seq,
        "tab-a",
        answer(json!({"ok": true, "page": "/activity", "dialog": "Edit schedule"})),
    )
    .unwrap();
    let a = pending.await.unwrap();
    assert_eq!(a["ok"], true, "{a}");
    assert_eq!(a["state"]["page_dialog"]["title"], "Edit schedule");
}

/// covers: drive-reach review (15 s no-answer). A step that waited out
/// TAB_WAIT is closed to the tabs (they drop it, also mid-search), and the
/// answer tells a claimed step that went quiet from one nobody claimed.
#[tokio::test(start_paused = true)]
async fn drive_review_a_closed_step_is_dropped_and_its_no_answer_says_why() {
    let (d, live) = driver("closed");
    let pending = send(&d, click("new-schedule", None));
    let seq = waiting(&d, &live, 1).await;
    d.claim(seq, "tab-a").unwrap();
    let a = pending.await.unwrap();
    let why = a["refusal"]["why"].as_str().unwrap_or_default();
    assert!(why.contains("claimed but no answer"), "{a}");
    assert!(why.contains("tab-a"), "{a}");
    assert!(
        live.events("drive")
            .iter()
            .any(|e| e["kind"] == "closed" && e["seq"] == seq),
        "no closed event for seq {seq}"
    );
    let a = d
        .step("wsl", Scope::Operate, click("new-schedule", None))
        .await;
    let why = a["refusal"]["why"].as_str().unwrap_or_default();
    assert!(why.contains("nobody claimed it"), "{a}");
}

/// covers: drive-reach review (15 s no-answer). The tab's own budget for
/// one step (pagedrive.js, carried in the generated catalog) stays below
/// the dashboard's wait for its answer, with a margin for the claim and
/// the answer's round trips.
#[test]
fn drive_review_the_tab_budget_fits_inside_the_dashboard_wait() {
    let c: Value = serde_json::from_str(homelab_admin::core::drive::DRIVE_CATALOG_JSON).unwrap();
    let budget = c["tab_budget_ms"].as_u64().unwrap_or(0);
    assert!(budget > 0, "the catalog carries no tab budget");
    let margin = 3_000;
    assert!(
        budget + margin <= TAB_WAIT.as_millis() as u64,
        "tab budget {budget} ms + {margin} ms > TAB_WAIT {} ms",
        TAB_WAIT.as_millis()
    );
}

/// covers: drive-reach review (15 s no-answer). The tab holding a page-level
/// dialog reloads (a new name for the same browser tab, the old one
/// reported as `was_tab`): the hold goes with the dialog, so the next step
/// is not refused as "open in another tab".
#[tokio::test(start_paused = true)]
async fn drive_review_a_reloaded_holder_lets_go_of_the_dialog() {
    let (d, live) = driver("holder");
    open_dialog(&d, &live, json!([]), json!([])).await;
    assert!(d.snapshot().page_dialog.is_some());
    let r = router(d.clone())
        .oneshot(
            Request::post("/data/drive/attach")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"page_version": "3.71.0", "tab": "tab-a2", "was_tab": "tab-a"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(r.status().is_success());
    assert!(
        d.snapshot().page_dialog.is_none(),
        "the reloaded tab still holds a dialog"
    );
    let pending = send(&d, click("new-schedule", None));
    let seq = waiting(&d, &live, 2).await;
    d.claim(seq, "tab-a2").unwrap();
    d.taken(
        seq,
        "tab-a2",
        answer(json!({"ok": true, "page": "/activity"})),
    )
    .unwrap();
    assert_eq!(pending.await.unwrap()["ok"], true);
}
