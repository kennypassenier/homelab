//! Live view: announce, plan and pause (Kenny, 2026-09-29, form "Live view
//! aankondigen"). The driver holds a step for its countdown on the server,
//! a viewer's Pause holds it until Continue (and the host is told to wait),
//! Stop fails it and ends the drive, and the plan follows the steps taken.
//! Time is tokio's paused clock, so a 30-minute pause takes no real time.

mod act_support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{
    MemFiles, MockHost, Recorder, Script, TestClock, history, shared, temp_dir, until,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use homelab_admin::core::drivelive::Control;
use homelab_admin::shell::actions::{Actions, ActionsDeps};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::drive::{Driver, LiveTiming, router};
use homelab_proto::{Scope, UiStep};
use serde_json::Value;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tower::ServiceExt as _;

struct World {
    host: Arc<MockHost>,
    live: Arc<Recorder>,
    driver: Driver,
    clock: TestClock,
}

fn world(tag: &str) -> World {
    let clock = TestClock::at(1_790_000_000);
    let host = MockHost::start(
        clock.clone(),
        Arc::new(|_| Script::ok(&[("pull", 5), ("up", 5)])),
        history("deploy-media", &[]),
        Arc::new(std::sync::Mutex::new(Vec::new())),
    );
    let live = Arc::new(Recorder::default());
    let dir = temp_dir(&format!("follow-live-{tag}"));
    let notify = NotifyCenter::load(
        dir.join("notifications.json"),
        Arc::new(act_support::RecPusher::default()),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    let shared = shared(&[("media", 106, None), ("admin", 120, None)]);
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: live.clone(),
        files: Arc::new(MemFiles::default()),
        shared: shared.clone(),
        notify,
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    });
    let driver = Driver::new(actions, shared, live.clone(), clock.clock());
    driver.set_timing(LiveTiming {
        announce: Duration::from_secs(3),
        max_pause: Duration::from_secs(1800),
    });
    World {
        host,
        live,
        driver,
        clock,
    }
}

fn goto(p: &str) -> UiStep {
    UiStep::Goto { path: p.into() }
}

type Holds = Arc<Mutex<Vec<(u64, Option<String>)>>>;

/// A step sent as the host relays it, with the host's holds recorded.
fn send(w: &World, step: UiStep, holds: &Holds) -> JoinHandle<Value> {
    let (d, h) = (w.driver.clone(), holds.clone());
    tokio::spawn(async move {
        let hold = move |wait: u64, note: Option<String>| h.lock().unwrap().push((wait, note));
        d.step_held("wsl", Scope::Operate, step, "3.70.0", &hold)
            .await
    })
}

async fn announced(w: &World) -> homelab_admin::core::drivelive::Announce {
    let d = w.driver.clone();
    until("the step is announced", || d.snapshot().announce.is_some()).await;
    w.driver.snapshot().announce.unwrap()
}

fn deploys(w: &World) -> usize {
    w.host
        .ran()
        .iter()
        .filter(|(_, name, _)| name == "deploy_stack")
        .count()
}

/// Live view.
///
/// A step that changes the screen is announced to every tab with its words
/// and a 3 s countdown, and taken only after it, on the server: the CLI's
/// answer comes after the step ran. Typing is not announced and not held.
#[tokio::test(start_paused = true)]
async fn follow_live_a_step_is_announced_and_held_for_the_countdown() {
    let w = world("announce");
    let holds = Holds::default();
    let t0 = Instant::now();
    let pending = send(&w, goto("/jobs"), &holds);
    let a = announced(&w).await;
    assert_eq!(a.text, "go to the jobs page");
    assert!(
        a.countdown && a.total_ms == 3000 && a.left_ms <= 3000,
        "{a:?}"
    );
    assert_eq!(w.driver.snapshot().page, "/", "not taken yet");
    // Every tab heard the announcement before the step.
    let kinds: Vec<Value> = w
        .live
        .events("drive")
        .iter()
        .map(|e| e["kind"].clone())
        .collect();
    assert_eq!(kinds, [Value::from("announce")]);
    let answer = pending.await.unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert!(t0.elapsed() >= Duration::from_secs(3), "{:?}", t0.elapsed());
    assert_eq!(answer["state"]["page"], "/jobs");
    assert!(answer["state"]["announce"].is_null());
    assert!(
        holds.lock().unwrap().is_empty(),
        "3 s fits the host's usual wait"
    );

    // A step that will be refused is refused at once, never announced.
    let before = w.live.events("drive").len();
    let t1 = Instant::now();
    let r = send(&w, goto("/nowhere"), &holds).await.unwrap();
    assert_eq!(r["ok"], false);
    assert!(t1.elapsed() < Duration::from_millis(100));
    assert!(
        w.live.events("drive")[before..]
            .iter()
            .all(|e| e["kind"] != "announce")
    );

    // Typing is played letter by letter by the tabs: no countdown.
    let opened = send(
        &w,
        UiStep::Open {
            form: "restore".into(),
            target: Some("media".into()),
        },
        &holds,
    );
    assert!(announced(&w).await.text.starts_with("open "));
    assert_eq!(opened.await.unwrap()["ok"], true);
    let t2 = Instant::now();
    let typed = send(
        &w,
        UiStep::Type {
            field: "act-snapshot".into(),
            text: "latest".into(),
        },
        &holds,
    )
    .await
    .unwrap();
    assert_eq!(typed["ok"], true, "{typed}");
    assert!(t2.elapsed() < Duration::from_millis(100));
}

/// Live view.
///
/// Pause holds the step past the host's usual 20 s (the host is told to
/// wait, and the CLI hears who paused); `ui state` still answers at once;
/// Continue takes the step; a pause past the longest one fails the step
/// with what, why and fix and takes nothing.
#[tokio::test(start_paused = true)]
async fn follow_live_pause_holds_the_step_until_continue_or_the_longest_pause() {
    let w = world("pause");
    let holds = Holds::default();
    let pending = send(&w, goto("/jobs"), &holds);
    announced(&w).await;
    let st = w
        .driver
        .control(Control::Pause, "the viewer kenny@example.org", None)
        .unwrap();
    assert_eq!(
        st.paused_by.as_deref(),
        Some("the viewer kenny@example.org")
    );
    tokio::time::sleep(Duration::from_secs(600)).await;
    assert!(!pending.is_finished(), "paused for 10 min, still held");
    assert_eq!(w.driver.snapshot().page, "/");
    // The screen can be read while a step is held.
    let now = w.driver.step("wsl", Scope::Read, UiStep::State).await;
    assert_eq!(now["state"]["paused_by"], "the viewer kenny@example.org");
    assert!(now["state"]["announce"]["left_ms"].as_u64().unwrap() <= 3000);
    {
        let h = holds.lock().unwrap();
        assert_eq!(h.len(), 1, "{h:?}");
        assert!(
            h[0].0 >= 1800 + 20,
            "the host waits past the longest pause: {h:?}"
        );
        let note = h[0].1.as_deref().unwrap();
        assert!(
            note.contains("paused by the viewer kenny@example.org"),
            "{note}"
        );
        assert!(note.contains("at most 30 min"), "{note}");
    }
    w.driver
        .control(Control::Continue, "the viewer kenny@example.org", None)
        .unwrap();
    let answer = pending.await.unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["state"]["page"], "/jobs");
    let notes: Vec<String> = holds
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(_, n)| n.clone())
        .collect();
    assert!(
        notes[1].contains("continued by the viewer kenny@example.org"),
        "{notes:?}"
    );
    // Continue with nothing paused is refused.
    assert!(w.driver.control(Control::Continue, "kenny", None).is_err());

    // A pause pressed between two steps holds the next one.
    w.driver
        .control(Control::Pause, "the viewer kenny", None)
        .unwrap();
    let t0 = Instant::now();
    let pending = send(&w, goto("/host"), &holds);
    let r = pending.await.unwrap();
    assert!(
        t0.elapsed() >= Duration::from_secs(1800),
        "{:?}",
        t0.elapsed()
    );
    assert_eq!(r["ok"], false);
    let why = r["refusal"]["why"].as_str().unwrap();
    assert!(
        why.contains("paused by the viewer kenny for more than 30 min"),
        "{why}"
    );
    assert!(!r["refusal"]["fix"].as_str().unwrap().is_empty());
    assert_eq!(r["state"]["page"], "/jobs", "nothing was taken");
    assert!(r["state"]["paused_by"].is_null() && r["state"]["announce"].is_null());
}

/// Live view.
///
/// Stop fails the held step with "stopped by the viewer", ends the drive
/// (the dialog closes, the plan is gone) and refuses every later step until
/// the driver says `done`; a final press stopped in its countdown never
/// runs.
#[tokio::test(start_paused = true)]
async fn follow_live_stop_ends_the_sequence_and_a_stopped_press_never_runs() {
    let w = world("stop");
    let holds = Holds::default();
    let opened = send(
        &w,
        UiStep::Open {
            form: "deploy".into(),
            target: Some("media".into()),
        },
        &holds,
    );
    announced(&w).await;
    assert_eq!(opened.await.unwrap()["state"]["form"]["step"], "review");
    let pressed = send(
        &w,
        UiStep::Press {
            button: "confirm".into(),
        },
        &holds,
    );
    let a = announced(&w).await;
    assert!(a.text.contains("the final press"), "{}", a.text);
    w.driver
        .control(Control::Stop, "the viewer kenny", None)
        .unwrap();
    let r = pressed.await.unwrap();
    assert_eq!(r["ok"], false);
    assert!(
        r["refusal"]["why"]
            .as_str()
            .unwrap()
            .starts_with("stopped by the viewer kenny")
    );
    assert!(r["state"]["form"].is_null() && r["state"]["active"] == false);
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(deploys(&w), 0, "{:?}", w.host.ran());
    // Every tab closes the dialog as on `done`.
    assert!(
        w.live
            .events("drive")
            .iter()
            .any(|e| e["step"]["do"] == "done" && e["applied"] == true)
    );

    // Refused until `done`, at once and without an announcement.
    let r = send(&w, goto("/jobs"), &holds).await.unwrap();
    let fix = r["refusal"]["fix"].as_str().unwrap();
    assert!(fix.contains("homelab ui done"), "{fix}");
    assert_eq!(
        w.driver.step("wsl", Scope::Operate, UiStep::Done).await["ok"],
        true
    );
    let r = send(&w, goto("/jobs"), &holds).await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    // Nothing to stop or pause once nobody drives.
    w.driver.step("wsl", Scope::Operate, UiStep::Done).await;
    assert!(w.driver.control(Control::Stop, "kenny", None).is_err());
}

/// Live view.
///
/// The plan is sent up front and changes nothing on screen; each step taken
/// ticks the plan's next one; a step that deviates is still taken and
/// marks the plan changed; `done` ends it.
#[tokio::test(start_paused = true)]
async fn follow_live_the_plan_is_ticked_step_by_step_and_a_deviation_marks_it_changed() {
    let w = world("plan");
    let holds = Holds::default();
    let plan = UiStep::Plan {
        steps: vec![goto("/jobs"), goto("/host"), UiStep::Done],
    };
    let r = w.driver.step("wsl", Scope::Operate, plan).await;
    assert_eq!(r["ok"], true, "{r}");
    let p = &r["state"]["plan"];
    assert_eq!(p["steps"][1]["text"], "go to the host page");
    assert_eq!(
        (p["next"].clone(), p["changed"].clone()),
        (0.into(), false.into())
    );
    assert_eq!(r["state"]["page"], "/");
    let r = send(&w, goto("/jobs"), &holds).await.unwrap();
    assert_eq!(r["state"]["plan"]["next"], 1);
    assert_eq!(r["state"]["plan"]["steps"][0]["done"], true);
    assert_eq!(r["state"]["plan"]["changed"], false);
    let r = send(&w, goto("/log"), &holds).await.unwrap();
    assert_eq!(r["ok"], true, "a step off the plan is still taken");
    assert_eq!(r["state"]["plan"]["changed"], true);
    assert_eq!(r["state"]["plan"]["next"], 1);
    let r = w.driver.step("wsl", Scope::Operate, UiStep::Done).await;
    assert!(r["state"]["plan"].is_null());
    // A plan with a step that changes nothing is refused.
    let r = w
        .driver
        .step(
            "wsl",
            Scope::Operate,
            UiStep::Plan {
                steps: vec![UiStep::State],
            },
        )
        .await;
    assert_eq!(r["ok"], false);
}

/// Live view.
///
/// The buttons' route: anyone logged in may press them; nothing to pause
/// when Claude is not driving is a 409 with what, why and fix.
#[tokio::test(start_paused = true)]
async fn follow_live_the_control_route_pauses_and_refuses_when_nobody_drives() {
    let w = world("route");
    let app = router(w.driver.clone());
    let post = |body: &str| {
        Request::post("/data/drive/control")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let r = app
        .clone()
        .oneshot(post(r#"{"do":"pause"}"#))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CONFLICT);
    let body: Value =
        serde_json::from_slice(&axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap())
            .unwrap();
    assert_eq!(body["why"], "Claude is not driving");
    let holds = Holds::default();
    let pending = send(&w, goto("/jobs"), &holds);
    announced(&w).await;
    let r = app
        .clone()
        .oneshot(post(r#"{"do":"pause"}"#))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert!(w.driver.snapshot().paused_by.is_some());
    let r = app
        .clone()
        .oneshot(post(r#"{"do":"continue"}"#))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(pending.await.unwrap()["ok"], true);
    let r = app.oneshot(post(r#"{"do":"fly"}"#)).await.unwrap();
    assert!(r.status().is_client_error());
}

/// fix-163 (Kenny, 2026-09-29), the pure part: a confirmed dialog whose job
/// has finished, with no step since, is released 30 s after the later of
/// the job's end and the last step; never while the job runs, a step is
/// announced or a viewer paused, and never a drive that is not active.
#[test]
fn fix_163_release_is_due_30_s_after_the_confirmed_job_ends() {
    use homelab_admin::core::actions::ActionKind;
    use homelab_admin::core::drive::{
        DriveState, JobRef, OpenForm, RELEASE_AFTER_JOB_S, Sources, action_form,
    };
    assert_eq!(RELEASE_AFTER_JOB_S, 30);
    let mut form = OpenForm::new(
        action_form(ActionKind::Deploy, "media"),
        &Sources::default(),
    );
    form.job = Some(JobRef {
        job: 7,
        state: "done".into(),
        message: Some("deploy complete".into()),
        progress: None,
    });
    let st = DriveState {
        active: true,
        by: Some("wsl".into()),
        seq: 5,
        form: Some(form),
        last_at: 1000,
        ..DriveState::default()
    };
    // The job ended at 1010, after the confirm at 1000.
    assert!(!st.release_due(1039, Some(1010)));
    assert!(st.release_due(1040, Some(1010)));
    // Still running: no end yet.
    assert!(!st.release_due(5000, None));
    // A step after the job's end starts the 30 s again.
    let later = DriveState {
        last_at: 1030,
        ..st.clone()
    };
    assert!(!later.release_due(1059, Some(1010)));
    assert!(later.release_due(1060, Some(1010)));
    // A dialog with no job, a drive that is over, an announced step or a
    // pause: nothing to release.
    let mut no_job = st.clone();
    no_job.form.as_mut().unwrap().job = None;
    assert!(!no_job.release_due(5000, Some(1010)));
    let over = DriveState {
        active: false,
        ..st.clone()
    };
    assert!(!over.release_due(5000, Some(1010)));
    let paused = DriveState {
        paused_by: Some("kenny".into()),
        ..st.clone()
    };
    assert!(!paused.release_due(5000, Some(1010)));
    // Released: as `homelab ui done`, the dialog closed and the tabs free.
    let mut done = st.clone();
    done.release(1040);
    assert!(done.form.is_none() && !done.active && done.plan.is_none());
    assert_eq!(done.seq, 6);
}

/// fix-163: after `ui press confirm` the dialog shows the job's end; when no
/// step follows within 30 s the dashboard closes it and gives the tabs
/// back, the same as `homelab ui done`, and every tab hears it as a step.
#[tokio::test(start_paused = true)]
async fn fix_163_a_finished_confirmed_dialog_is_closed_and_released() {
    let w = world("release");
    w.driver.set_timing(LiveTiming::default());
    let open = UiStep::Open {
        form: "deploy".into(),
        target: Some("media".into()),
    };
    assert_eq!(w.driver.step("wsl", Scope::Operate, open).await["ok"], true);
    let pressed = w
        .driver
        .step(
            "wsl",
            Scope::Operate,
            UiStep::Press {
                button: "confirm".into(),
            },
        )
        .await;
    assert_eq!(pressed["ok"], true, "{pressed}");
    // Before the job ends nothing is released, however long it runs.
    assert!(!w.driver.release_if_done());
    let d = w.driver.clone();
    until("the job ended", || {
        d.snapshot()
            .form
            .and_then(|f| f.job)
            .is_some_and(|j| j.state == "done")
    })
    .await;
    assert!(!w.driver.release_if_done(), "not before 30 s");
    w.clock.advance(29);
    assert!(!w.driver.release_if_done(), "not before 30 s");
    w.clock.advance(1);
    assert!(w.driver.release_if_done());
    let s = w.driver.snapshot();
    assert!(s.form.is_none() && !s.active, "{s:?}");
    let last = w.live.events("drive").last().cloned().unwrap();
    assert_eq!(last["kind"], "step");
    assert_eq!(last["step"]["do"], "done");
    assert_eq!(last["applied"], true);
    // Once released there is nothing left to release.
    w.clock.advance(60);
    assert!(!w.driver.release_if_done());
    assert_eq!(deploys(&w), 1);
}

/// fix-185 (Kenny, 2026-10-02): a viewer's Stop stayed held after `ui done`
/// had already acknowledged it, and a brand new round of steps was refused
/// "stopped by the viewer" all the same. The cause: `Driver::control` judged
/// whether a drive was active from the CURRENT state alone, so a Stop
/// delayed in flight (a slow connection, a double-submitted click) and only
/// delivered after `done` had cleanly ended round one could still land while
/// round two's first step was announced — and since announcing makes the
/// drive "active" again, that stale Stop was honoured and stopped the wrong,
/// unrelated round. The fix: a Stop (or Pause/Continue) carries the `seq`
/// the viewer's button was drawn against; one that no longer matches the
/// live state is refused as stale rather than acted on.
#[tokio::test(start_paused = true)]
async fn fix_185_a_stale_stop_never_reaches_into_a_later_round() {
    let w = world("stale-stop");
    let holds = Holds::default();

    // Round one: open, press confirm, and Stop it — exactly the existing
    // Stop behaviour, with the `seq` the viewer's button was drawn against.
    let opened = send(
        &w,
        UiStep::Open {
            form: "deploy".into(),
            target: Some("media".into()),
        },
        &holds,
    );
    announced(&w).await;
    assert_eq!(opened.await.unwrap()["state"]["form"]["step"], "review");
    let round_one_seq = w.driver.snapshot().seq;
    let pressed = send(
        &w,
        UiStep::Press {
            button: "confirm".into(),
        },
        &holds,
    );
    announced(&w).await;
    w.driver
        .control(Control::Stop, "the viewer kenny", Some(round_one_seq))
        .unwrap();
    assert_eq!(pressed.await.unwrap()["ok"], false);

    // The driver acknowledges the stop and starts a clean round two.
    assert_eq!(
        w.driver.step("wsl", Scope::Operate, UiStep::Done).await["ok"],
        true
    );
    let round_two = send(&w, goto("/jobs"), &holds);
    announced(&w).await;

    // The STALE Stop from round one — carrying round one's seq, delivered
    // only now — must be refused: it is not round two's seq.
    let stale = w
        .driver
        .control(Control::Stop, "the viewer kenny", Some(round_one_seq));
    assert!(stale.is_err(), "a stale Stop must not be honoured");
    let why = stale.unwrap_err().why;
    assert!(why.contains("moved on"), "{why}");

    // Round two's step still runs, undisturbed by the stale Stop.
    let r = round_two.await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["state"]["page"], "/jobs");
    assert!(r["state"]["stopped_by"].is_null());

    // A Stop with no seq at all (an older tab) keeps working as before.
    w.driver
        .control(Control::Stop, "the viewer kenny", None)
        .unwrap();
    assert_eq!(
        w.driver.snapshot().stopped_by.as_deref(),
        Some("the viewer kenny")
    );
}

/// fix-199 (replaces fix-185's blanket version refusal; Kenny, 2026-10-02:
/// a version check informs, it never blocks). A tab reports its loaded page
/// version AND its own `formspec.json`'s pages/forms over
/// `/data/drive/attach`; a step for a page/form it does not list is refused
/// at once, naming only that page/form (never a version number); a step for
/// one it DOES list is taken normally, however the two sides' versions
/// compare; `state` never checks (reading the screen is always safe); a tab
/// that reported no capabilities at all is never refused on their absence.
#[tokio::test(start_paused = true)]
async fn fix_199_a_step_is_refused_only_for_a_page_or_form_the_tab_does_not_know() {
    let w = world("version-match");
    let holds = Holds::default();
    let app = router(w.driver.clone());
    let attach = |v: &str, pages: &str, forms: &str| {
        Request::post("/data/drive/attach")
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"page_version":"{v}","known_pages":{pages},"known_forms":{forms}}}"#
            )))
            .unwrap()
    };
    // An older tab (3.69.0) that does not yet list "jobs" among its pages.
    let r = app
        .clone()
        .oneshot(attach("3.69.0", r#"["","overview","host"]"#, "[]"))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(
        w.driver.snapshot().tab_page_version.as_deref(),
        Some("3.69.0")
    );

    // `state` is read-only and never checks the tab's capabilities.
    let s = w
        .driver
        .step_held("wsl", Scope::Read, UiStep::State, "3.70.0", &|_, _| {})
        .await;
    assert_eq!(s["ok"], true, "{s}");

    // "jobs" is not in what the tab reported: refused, naming the page —
    // never a version number, since the versions are no longer compared.
    let r = w
        .driver
        .step_held("wsl", Scope::Operate, goto("/jobs"), "3.70.0", &|_, _| {})
        .await;
    assert_eq!(r["ok"], false, "{r}");
    let why = r["refusal"]["why"].as_str().unwrap();
    assert!(why.contains("page \"jobs\""), "{why}");
    assert!(!why.contains("3.69.0") && !why.contains("3.70.0"), "{why}");
    let fix = r["refusal"]["fix"].as_str().unwrap();
    assert!(fix.contains("homelab ui reload"), "{fix}");
    assert_eq!(w.driver.snapshot().page, "/", "nothing was taken");

    // "host" IS one of the pages this same older tab reported: taken
    // normally, although the tab is still on 3.69.0 and the client is
    // 3.70.0 — a known page is never refused on the version alone.
    let pending = send(&w, goto("/host"), &holds);
    announced(&w).await;
    let r = pending.await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["state"]["page"], "/host");

    // Once the tab reports "jobs" too (its dashboard updated and it
    // reloaded), the same step that was refused now succeeds.
    app.clone()
        .oneshot(attach("3.70.0", r#"["","overview","host","jobs"]"#, "[]"))
        .await
        .unwrap();
    let pending = send(&w, goto("/jobs"), &holds);
    announced(&w).await;
    let r = pending.await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["state"]["page"], "/jobs");
}

/// fix-199: a tab that has never reported any capabilities (an old tab from
/// before this field existed, or `/data/drive/attach` not yet called) is
/// never refused for it — the permissive default from fix-185 (nothing
/// known about a tab is never a reason to refuse) carries over unchanged.
#[tokio::test(start_paused = true)]
async fn fix_199_a_tab_that_never_reported_capabilities_is_never_refused() {
    let w = world("no-caps");
    let holds = Holds::default();
    let pending = send(&w, goto("/jobs"), &holds);
    announced(&w).await;
    let r = pending.await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
}

/// fix-185 (`homelab ui reload`): the driven tab takes the dashboard's
/// current page — the live channel carries a "reload" event — and the step
/// waits, bounded, until a tab reports the client's own version; it fails
/// when nothing ever does.
#[tokio::test(start_paused = true)]
async fn fix_185_reload_waits_for_the_tab_to_report_the_new_version() {
    let w = world("reload");
    let holds = Holds::default();
    let app = router(w.driver.clone());
    let attach = |v: &str| {
        Request::post("/data/drive/attach")
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"page_version":"{v}"}}"#)))
            .unwrap()
    };
    app.clone().oneshot(attach("3.69.0")).await.unwrap();

    // `reload` itself is never refused by the version check, even though
    // the tab is stale — it is the step that fixes that. It is announced
    // and held like any other step first (the existing `send`/`announced`
    // helpers already drive a step through its countdown).
    let pending = send(&w, UiStep::Reload, &holds);
    announced(&w).await;
    // Every tab was told to reload, once the announcement's countdown ran.
    until("the reload was broadcast", || {
        w.live.events("drive").iter().any(|e| e["kind"] == "reload")
    })
    .await;
    assert!(!pending.is_finished(), "still waiting for the new version");
    // The tab comes back on the new version.
    app.clone().oneshot(attach("3.70.0")).await.unwrap();
    let r = pending.await.unwrap();
    assert_eq!(r["ok"], true, "{r}");

    // Nothing ever reports the client's version: the step fails after the
    // bounded wait, naming it.
    app.clone().oneshot(attach("3.69.0")).await.unwrap();
    let pending = send(&w, UiStep::Reload, &holds);
    announced(&w).await;
    let r = pending.await.unwrap();
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r["refusal"]["why"].as_str().unwrap().contains("3.70.0"),
        "{r}"
    );
}

/// fix-226 (Kenny, 2026-10-02: "er staat in live-view nog altijd 'next:
/// claude's next step' terwijl je niks aan het doen bent, je moet na je
/// commands controle altijd direct teruggeven"): a drive silent for the idle
/// limit — now 20 s — is due to be given back, even with no `ui done` and no
/// job; a step still in its announce countdown is never cut short.
#[test]
fn fix_226_a_silent_drive_is_given_back_within_seconds() {
    use homelab_admin::core::drive::{DriveState, IDLE_S};
    const {
        assert!(
            IDLE_S <= 30,
            "the idle release must be seconds, not minutes"
        )
    };
    let st = DriveState {
        active: true,
        by: Some("wsl".into()),
        last_at: 1000,
        ..DriveState::default()
    };
    assert!(!st.idle_release_due(1000 + IDLE_S - 1));
    assert!(st.idle_release_due(1000 + IDLE_S));
    let over = DriveState {
        active: false,
        ..st.clone()
    };
    assert!(!over.idle_release_due(5000));
}
