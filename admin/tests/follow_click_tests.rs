//! fix-239 (Kenny, 2026-10-02: "waarom gebruik je live view niet?"): Live
//! view drives every button a page draws, not only the catalog's actions
//! and the edit forms. A page's own control (`homelab ui click <control>
//! [row]`) is taken by ONE tab that follows — the first to claim it — which
//! clicks it as a person would and answers; the shared state follows that
//! answer (the page, the page-level dialog it opened), and the driver hears
//! why when it could not be taken, or that no tab took it at all.

mod act_support;

use std::sync::Arc;
use std::time::Duration;

use act_support::{
    MemFiles, MockHost, Recorder, Script, TestClock, history, shared, temp_dir, until,
};
use homelab_admin::core::drive::TabAnswer;
use homelab_admin::shell::actions::{Actions, ActionsDeps};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::drive::Driver;
use homelab_proto::{Scope, UiStep};
use serde_json::Value;
use tokio::task::JoinHandle;

fn driver(tag: &str) -> (Driver, Arc<Recorder>) {
    let clock = TestClock::at(1_790_000_000);
    let host = MockHost::start(
        clock.clone(),
        Arc::new(|_| Script::ok(&[("pull", 5)])),
        history("deploy-media", &[]),
        Arc::new(std::sync::Mutex::new(Vec::new())),
    );
    let live = Arc::new(Recorder::default());
    let dir = temp_dir(&format!("follow-click-{tag}"));
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

/// The step's seq, once the tabs have been sent it.
async fn waiting(d: &Driver, live: &Recorder, n: usize) -> u64 {
    until("the step went out to the tabs", || {
        live.events("drive")
            .iter()
            .filter(|e| e["kind"] == "step")
            .count()
            >= n
    })
    .await;
    d.snapshot().seq
}

/// covers: fix-239
#[tokio::test(start_paused = true)]
async fn fix_239_a_click_is_taken_by_the_one_tab_that_claims_it() {
    let (d, live) = driver("claim");
    let pending = send(&d, click("pin-update", Some("media/web/web")));
    let seq = waiting(&d, &live, 1).await;
    // The step went to every tab as a click, before any tab took it.
    let ev = live.events("drive").last().cloned().unwrap();
    assert_eq!(ev["step"]["do"], "click", "{ev}");
    assert_eq!(ev["step"]["row"], "media/web/web", "{ev}");

    d.claim(seq, "tab-a").unwrap();
    let other = d.claim(seq, "tab-b").unwrap_err();
    assert!(other.why.contains("claimed it first"), "{other:?}");
    assert!(d.claim(seq + 1, "tab-a").is_err(), "another seq is no step");
    assert!(
        d.taken(seq, "tab-b", TabAnswer::default()).is_err(),
        "only the claimant answers"
    );
    d.taken(
        seq,
        "tab-a",
        TabAnswer {
            ok: true,
            page: Some("/fleetview".into()),
            dialog: Some("Update media/web to 2.0".into()),
            ..TabAnswer::default()
        },
    )
    .unwrap();
    let answer = pending.await.unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["state"]["page"], "/fleetview");
    assert_eq!(
        answer["state"]["page_dialog"]["title"],
        "Update media/web to 2.0"
    );
    assert_eq!(answer["state"]["page_dialog"]["control"], "pin-update");

    // Inside that dialog, a tick and a press are the holder's alone.
    let pending = send(
        &d,
        UiStep::Check {
            field: "pin-major-read".into(),
            on: true,
        },
    );
    let seq = waiting(&d, &live, 2).await;
    let not_holder = d.claim(seq, "tab-b").unwrap_err();
    assert!(not_holder.why.contains("another tab"), "{not_holder:?}");
    d.claim(seq, "tab-a").unwrap();
    d.taken(
        seq,
        "tab-a",
        TabAnswer {
            ok: true,
            dialog: Some("Update media/web to 2.0".into()),
            ..TabAnswer::default()
        },
    )
    .unwrap();
    assert_eq!(pending.await.unwrap()["ok"], true);

    // Close closes it on the shared state; a goto is taken again.
    let closed = d.step("wsl", Scope::Operate, UiStep::Close).await;
    assert_eq!(closed["ok"], true, "{closed}");
    assert!(closed["state"]["page_dialog"].is_null(), "{closed}");
}

/// covers: fix-239
#[tokio::test(start_paused = true)]
async fn fix_239_what_the_tab_could_not_do_reaches_the_driver() {
    let (d, live) = driver("refused");
    let pending = send(&d, click("pin-update", Some("media/nope/nope")));
    let seq = waiting(&d, &live, 1).await;
    d.claim(seq, "tab-a").unwrap();
    d.taken(
        seq,
        "tab-a",
        TabAnswer {
            ok: false,
            why: Some("pin-update has no row media/nope/nope".into()),
            fix: Some("its rows are: media/web/web".into()),
            page: Some("/fleetview".into()),
            dialog: None,
        },
    )
    .unwrap();
    let answer = pending.await.unwrap();
    assert_eq!(answer["ok"], false, "{answer}");
    assert_eq!(answer["refusal"]["what"], "ui click");
    assert_eq!(answer["refusal"]["fix"], "its rows are: media/web/web");
    assert!(answer["state"]["page_dialog"].is_null());
}

/// covers: fix-239
#[tokio::test(start_paused = true)]
async fn fix_239_a_click_no_tab_takes_is_refused_and_says_to_turn_live_view_on() {
    let (d, _live) = driver("nobody");
    let answer = d
        .step("wsl", Scope::Operate, click("new-schedule", None))
        .await;
    assert_eq!(answer["ok"], false, "{answer}");
    let why = answer["refusal"]["why"].as_str().unwrap_or_default();
    assert!(why.contains("no dashboard tab in Live view"), "{answer}");
    assert!(
        answer["refusal"]["fix"]
            .as_str()
            .unwrap_or_default()
            .contains("turn Live view on"),
        "{answer}"
    );
}

/// covers: fix-239
#[tokio::test(start_paused = true)]
async fn fix_239_a_click_while_a_server_form_is_open_says_to_press_or_close() {
    let (d, _live) = driver("form-open");
    let open = d
        .step(
            "wsl",
            Scope::Operate,
            UiStep::Open {
                form: "backup".into(),
                target: Some("media".into()),
            },
        )
        .await;
    assert_eq!(open["ok"], true, "{open}");
    let answer = d
        .step("wsl", Scope::Operate, click("pin-update", None))
        .await;
    assert_eq!(answer["ok"], false, "{answer}");
    assert!(
        answer["refusal"]["fix"]
            .as_str()
            .unwrap_or_default()
            .contains("homelab ui press"),
        "{answer}"
    );
}
