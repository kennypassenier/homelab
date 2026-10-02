//! Milestone follow (feat-platform-10): Claude drives the dashboard step by
//! step. The driver against the act mock host: steps checked against the
//! shared form description, every step pushed to the tabs, and the final
//! press running once, whatever number of tabs follow.

mod act_support;

use std::sync::Arc;
use std::time::Duration;

use act_support::{
    MemFiles, MockHost, Recorder, Script, TestClock, history, shared, temp_dir, until,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use homelab_admin::core::actions::{self, ActionKind};
use homelab_admin::core::drive::{self, Values, check_values, spec};
use homelab_admin::shell::actions::{Actions, ActionsDeps};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::drive::{Driver, router};
use homelab_core::ops::deployguard::Ancestry;
use homelab_proto::{Scope, UiStep};
use serde_json::Value;
use tower::ServiceExt as _;

struct World {
    host: Arc<MockHost>,
    live: Arc<Recorder>,
    driver: Driver,
}

fn world(tag: &str, files: MemFiles, applied: Option<&str>) -> World {
    let clock = TestClock::at(1_790_000_000);
    let host = MockHost::start(
        clock.clone(),
        Arc::new(|_| Script::ok(&[("pull", 5), ("up", 5)])),
        history("deploy-media", &[]),
        Arc::new(std::sync::Mutex::new(Vec::new())),
    );
    let live = Arc::new(Recorder::default());
    let dir = temp_dir(&format!("follow-{tag}"));
    let notify = NotifyCenter::load(
        dir.join("notifications.json"),
        Arc::new(act_support::RecPusher::default()),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    let shared = shared(&[
        ("media", 106, applied),
        ("drill", 119, None),
        ("admin", 120, None),
    ]);
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: live.clone(),
        files: Arc::new(files),
        shared: shared.clone(),
        notify,
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    });
    let driver = Driver::new(actions, shared, live.clone(), clock.clock());
    World { host, live, driver }
}

fn goto(p: &str) -> UiStep {
    UiStep::Goto { path: p.into() }
}
fn open(f: &str, t: Option<&str>) -> UiStep {
    UiStep::Open {
        form: f.into(),
        target: t.map(str::to_string),
    }
}
fn press(b: &str) -> UiStep {
    UiStep::Press { button: b.into() }
}
fn typed(f: &str, t: &str) -> UiStep {
    UiStep::Type {
        field: f.into(),
        text: t.into(),
    }
}

async fn step(w: &World, s: UiStep) -> Value {
    w.driver.step("wsl", Scope::Operate, s).await
}

fn refused(v: &Value) -> (String, String, String) {
    assert_eq!(v["ok"], false, "expected a refusal: {v}");
    let r = &v["refusal"];
    let (what, why, fix) = (
        r["what"].as_str().unwrap_or_default().to_string(),
        r["why"].as_str().unwrap_or_default().to_string(),
        r["fix"].as_str().unwrap_or_default().to_string(),
    );
    assert!(
        !what.is_empty() && !why.is_empty() && !fix.is_empty(),
        "{v}"
    );
    (what, why, fix)
}

fn deploys(w: &World) -> usize {
    w.host
        .ran()
        .iter()
        .filter(|(_, name, _)| name == "deploy_stack")
        .count()
}

/// feat-platform-10 (milestone follow).
///
/// The form description is one file: the server reads the same
/// `formspec.json` the browser imports, and the same values give the same
/// words on both sides (the browser's half of this runs in the node suite
/// over the same cases file).
#[test]
fn follow_the_form_description_is_one_file_both_sides_read() {
    for kind in ActionKind::ALL {
        for a in kind.args() {
            assert!(
                spec().fields.contains_key(drive::arg_name(*a)),
                "formspec.json lacks {}",
                drive::arg_name(*a)
            );
        }
    }
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/test/formspec-cases.json"),
    )
    .unwrap();
    let cases: Value = serde_json::from_str(&raw).unwrap();
    for c in cases["cases"].as_array().unwrap() {
        let kind = ActionKind::from_slug(c["action"].as_str().unwrap()).unwrap();
        let args: Vec<&str> = kind.args().iter().map(|a| drive::arg_name(*a)).collect();
        let want: Vec<&str> = c["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect();
        assert_eq!(args, want, "the cases file's args drifted from the catalog");
        assert_eq!(Value::Bool(kind.confirm()), c["confirm"]);
        let form = drive::action_form(kind, c["stack"].as_str().unwrap());
        let values: Values = serde_json::from_value(c["values"].clone()).unwrap();
        let got = check_values(&form, &values, c["step"].as_str());
        let want: std::collections::BTreeMap<String, String> =
            serde_json::from_value(c["errors"].clone()).unwrap();
        assert_eq!(got, want, "{}", c);
    }
    // The ids the browser gives its inputs (actiondialog.js draws the field
    // with `id` = the field's id) are the ids a step names.
    let f = drive::action_form(ActionKind::Restore, "media");
    let ids: Vec<&str> = f.fields().map(|x| x.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "act-snapshot",
            "act-app",
            "act-skip-safety-copy",
            "act-confirm"
        ]
    );
    assert_eq!(f.steps[0].label, "Options");
    assert_eq!(f.title, "Restore · media");
}

/// feat-platform-10 (milestone follow).
///
/// Kenny's condition: the final press runs on the dashboard's server, once,
/// whether zero, one or two tabs follow. A tab only reads the state
/// (`GET /data/drive`) and the live events; a second confirm is refused.
#[tokio::test]
async fn follow_the_final_press_runs_once_with_zero_one_or_two_tabs() {
    for tabs in 0..=2usize {
        let w = world(&format!("once-{tabs}"), MemFiles::default(), None);
        let app = router(w.driver.clone());
        assert_eq!(step(&w, goto("/stacks/media")).await["ok"], true);
        let opened = step(&w, open("deploy", Some("media"))).await;
        assert_eq!(opened["ok"], true, "{opened}");
        assert_eq!(opened["state"]["form"]["step"], "review");
        // Every tab catches up, as a tab that turns following on does.
        for _ in 0..tabs {
            let r = app
                .clone()
                .oneshot(Request::get("/data/drive").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::OK);
        }
        let pressed = step(&w, press("confirm")).await;
        assert_eq!(pressed["ok"], true, "{pressed}");
        let job = pressed["state"]["form"]["job"]["job"].as_u64().unwrap();
        // The same press again is refused, never a second job.
        let again = step(&w, press("confirm")).await;
        let (_, why, _) = refused(&again);
        assert!(why.contains("one press runs once"), "{why}");
        for _ in 0..tabs {
            let _ = app
                .clone()
                .oneshot(Request::get("/data/drive").body(Body::empty()).unwrap())
                .await
                .unwrap();
        }
        let d = w.driver.clone();
        until("the driven deploy is done", || {
            d.snapshot()
                .form
                .and_then(|f| f.job)
                .is_some_and(|j| j.state == "done")
        })
        .await;
        assert_eq!(deploys(&w), 1, "{tabs} tab(s): {:?}", w.host.ran());
        let s = w.driver.snapshot();
        let j = s.form.unwrap().job.unwrap();
        assert_eq!((j.job, j.message.as_deref()), (job, Some("complete")));
        // The job says who pressed.
        let origin = w
            .live
            .events("action")
            .into_iter()
            .find(|e| e["job"] == job)
            .map(|e| e["origin"].clone())
            .unwrap();
        assert_eq!(origin, serde_json::json!({"from": "claude", "by": "wsl"}));
    }
}

/// feat-platform-10 (milestone follow).
///
/// Steps are checked against the form description: an unknown field, the
/// wrong kind, a field on another step, a value not on the list, an unknown
/// page, form or stack are refused with {what, why, fix} and change nothing;
/// a press the form holds is shown with its errors.
#[tokio::test]
async fn follow_steps_are_checked_against_the_form_description() {
    let w = world("checked", MemFiles::default(), None);
    let (_, why, fix) = refused(&step(&w, goto("/nowhere")).await);
    assert!(
        why.contains("no page") && fix.contains("/stacks/<name>"),
        "{why} {fix}"
    );
    let (_, why, _) = refused(&step(&w, goto("/stacks/ghost")).await);
    assert!(why.contains("no stack ghost"));
    let (_, _, fix) = refused(&step(&w, open("fly", Some("media"))).await);
    assert!(fix.contains("deploy"), "{fix}");
    refused(&step(&w, open("restore", Some("ghost"))).await);
    refused(&step(&w, open("restore", None)).await);
    refused(&step(&w, UiStep::Close).await);
    let seq = w.driver.snapshot().seq;
    assert_eq!(seq, 0, "a refused step changes nothing");

    let opened = step(&w, open("restore", Some("media"))).await;
    assert_eq!(opened["state"]["form"]["step"], "options");
    assert_eq!(opened["state"]["page"], "/stacks/media");
    let (_, why, fix) = refused(&step(&w, typed("act-reason", "new version")).await);
    assert!(why.contains("no field act-reason"), "{why}");
    assert!(
        fix.contains("act-snapshot") && fix.contains("act-confirm"),
        "{fix}"
    );
    let (_, _, fix) = refused(&step(&w, typed("act-skip-safety-copy", "yes")).await);
    assert!(fix.contains("homelab ui check"), "{fix}");
    let (_, why, _) = refused(&step(&w, typed("act-confirm", "media")).await);
    assert!(why.contains("on the step review"), "{why}");
    let (_, why, _) = refused(
        &step(
            &w,
            UiStep::Pick {
                field: "act-app".into(),
                value: "jellyfin".into(),
            },
        )
        .await,
    );
    assert!(why.contains("not a choice"), "{why}");
    let (_, _, fix) = refused(&step(&w, press("confirm")).await);
    assert!(fix.contains("next, close"), "{fix}");

    // A bad snapshot: the press happens, the form holds it and says why.
    assert_eq!(step(&w, typed("act-snapshot", "bad id!")).await["ok"], true);
    let held = step(&w, press("next")).await;
    let (_, why, _) = refused(&held);
    assert!(why.contains("act-snapshot"), "{why}");
    let f = &held["state"]["form"];
    assert_eq!(f["step"], "options");
    assert_eq!(
        f["errors"]["snapshot"],
        "\"bad id!\" is not a valid snapshot."
    );
    assert_eq!(step(&w, typed("act-snapshot", "latest")).await["ok"], true);
    assert_eq!(
        step(&w, press("next")).await["state"]["form"]["step"],
        "review"
    );
    // The typed name is checked at the final press, in the browser's words.
    assert_eq!(step(&w, typed("act-confirm", "medi")).await["ok"], true);
    let held = step(&w, press("confirm")).await;
    refused(&held);
    assert_eq!(
        held["state"]["form"]["errors"]["confirm"],
        "That is not media; type the stack's name exactly."
    );
    assert!(held["state"]["form"]["job"].is_null());
    assert_eq!(w.host.ran().len(), 0, "nothing reached the host");
    refused(&step(&w, goto("/jobs")).await);
    assert_eq!(step(&w, UiStep::Close).await["state"]["form"], Value::Null);
    assert_eq!(step(&w, goto("/jobs")).await["state"]["page"], "/jobs");
}

/// feat-platform-10 (milestone follow).
///
/// The driving token's scope holds: an operate token cannot open a destroy,
/// a token of scope all can and its press runs; the dashboard's own stack is
/// refused for what the dashboard never does to itself.
#[tokio::test]
async fn follow_the_driver_acts_only_within_its_own_scope() {
    let w = world("scope", MemFiles::default(), None);
    let (_, why, _) = refused(&step(&w, open("destroy", Some("drill"))).await);
    assert!(
        why.contains("needs scope All") && why.contains("wsl"),
        "{why}"
    );
    let read = w
        .driver
        .step("ro", Scope::Read, open("deploy", Some("media")))
        .await;
    refused(&read);
    let all = |s| w.driver.step("wsl-all", Scope::All, s);
    refused(&all(open("destroy", Some("admin"))).await);
    assert_eq!(all(open("destroy", Some("drill"))).await["ok"], true);
    assert_eq!(all(press("next")).await["ok"], true);
    assert_eq!(all(typed("act-confirm", "drill")).await["ok"], true);
    let pressed = all(press("confirm")).await;
    assert_eq!(pressed["ok"], true, "{pressed}");
    let d = w.driver.clone();
    until("the destroy job ends", || {
        d.snapshot()
            .form
            .and_then(|f| f.job)
            .is_some_and(|j| j.state == "done")
    })
    .await;
    assert_eq!(
        w.host
            .ran()
            .iter()
            .filter(|(_, n, s)| n == "destroy_stack" && s == "drill")
            .count(),
        1
    );
}

/// feat-platform-10 (milestone follow).
///
/// The deploy guard holds a driven press as it holds a click: the force
/// field shows only while the guard refuses, and ticking it lets the press
/// through.
#[tokio::test]
async fn follow_the_deploy_guard_holds_a_driven_press_until_force() {
    let files = MemFiles {
        ancestry: Ancestry::Unknown,
        ..MemFiles::default()
    };
    let w = world("guard", files, Some("a1b2c3d4e5f6"));
    let opened = step(&w, open("deploy", Some("media"))).await;
    let force = &opened["state"]["form"]["fields"][0];
    assert_eq!(
        (force["id"].as_str(), force["shown"].as_bool()),
        (Some("act-force"), Some(true))
    );
    assert!(!opened["state"]["form"]["guard"].is_null());
    let held = step(&w, press("confirm")).await;
    let (_, _, fix) = refused(&held);
    assert!(fix.contains("act-force on"), "{fix}");
    assert_eq!(deploys(&w), 0);
    let on = step(
        &w,
        UiStep::Check {
            field: "act-force".into(),
            on: true,
        },
    )
    .await;
    assert_eq!(on["ok"], true);
    assert_eq!(step(&w, press("confirm")).await["ok"], true);
    let d = w.driver.clone();
    until("the forced deploy is done", || {
        d.snapshot()
            .form
            .and_then(|f| f.job)
            .is_some_and(|j| j.state == "done")
    })
    .await;
    assert_eq!(deploys(&w), 1);
}

/// feat-platform-10 (milestone follow).
///
/// Every applied step and every refusal goes to the tabs as one `drive`
/// event with the whole state, numbered, so a tab that saw the step before
/// animates the next and any other catches up; reading the state is not a
/// step and is not pushed; `done` hands the tabs back.
#[tokio::test]
async fn follow_every_step_is_pushed_to_the_tabs_with_the_whole_state() {
    let w = world("pushed", MemFiles::default(), None);
    step(&w, goto("/stacks/media")).await;
    step(&w, UiStep::State).await;
    step(&w, open("update", Some("media"))).await;
    step(&w, typed("act-nope", "x")).await;
    step(&w, UiStep::Done).await;
    let events = w.live.events("drive");
    let seqs: Vec<u64> = events.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert_eq!(seqs, [1, 2, 2, 3]);
    let applied: Vec<bool> = events.iter().map(|e| e["applied"] == true).collect();
    assert_eq!(applied, [true, true, false, true]);
    assert_eq!(events[1]["step"]["do"], "open");
    assert_eq!(events[1]["state"]["form"]["action"], "update");
    assert_eq!(events[2]["refusal"]["what"], "ui type");
    assert_eq!(events[3]["state"]["active"], false);
    assert_eq!(events[3]["state"]["form"], Value::Null);
    // The shape the CLI renders.
    let text = homelab_client::ui_cli::render(&step(&w, UiStep::State).await.to_string()).unwrap();
    assert!(text.contains("not driving"), "{text}");
    let _ = actions::HOST_TARGET;
}

/// TUI parity round: the new forms are drivable like every action form, and
/// the press still needs the action's own scope. exec (all scope) is one
/// step, no typed name: an operate token cannot press it, an all token can,
/// once; the copied line is the CLI's.
#[tokio::test]
async fn parity_exec_is_drivable_and_keeps_its_scope() {
    let w = world("exec", MemFiles::default(), None);
    let (_, why, _) = refused(&step(&w, open("exec", None)).await);
    assert!(why.contains("needs scope All"), "{why}");
    let all = |s: UiStep| {
        let d = w.driver.clone();
        async move { d.step("claude", Scope::All, s).await }
    };
    let opened = all(open("exec", None)).await;
    assert_eq!(opened["ok"], true, "{opened}");
    assert_eq!(opened["state"]["form"]["step"], "review");
    assert_eq!(opened["state"]["page"], "/host");
    assert_eq!(all(typed("act-vmid", "106")).await["ok"], true);
    let t = all(typed("act-command", "df -h")).await;
    assert_eq!(t["ok"], true, "{t}");
    assert_eq!(t["state"]["form"]["cli"], "homelab exec 106 'df -h'");
    let pressed = all(press("confirm")).await;
    assert_eq!(pressed["ok"], true, "{pressed}");
    let d = w.driver.clone();
    until("the exec ran", || {
        d.snapshot()
            .form
            .and_then(|f| f.job)
            .is_some_and(|j| j.state == "done")
    })
    .await;
    let ran: Vec<String> = w.host.ran().into_iter().map(|(_, n, _)| n).collect();
    assert_eq!(ran.iter().filter(|n| *n == "exec_in").count(), 1, "{ran:?}");
}

/// cli-yes: a driven restore's line carries --yes once the typed name
/// matches, and not before.
#[tokio::test]
async fn parity_the_driven_line_carries_yes_once_the_name_is_typed() {
    let w = world("yes", MemFiles::default(), None);
    assert_eq!(step(&w, open("restore", Some("media"))).await["ok"], true);
    let review = step(&w, press("next")).await;
    let cli = review["state"]["form"]["cli"].as_str().unwrap().to_string();
    assert!(cli.starts_with("homelab restore media latest"), "{cli}");
    assert!(!cli.contains("--yes"), "{cli}");
    let t = step(&w, typed("act-confirm", "medi")).await;
    assert!(
        !t["state"]["form"]["cli"]
            .as_str()
            .unwrap()
            .contains("--yes")
    );
    let t = step(&w, typed("act-confirm", "media")).await;
    assert_eq!(
        t["state"]["form"]["cli"], "homelab restore media latest --yes",
        "{t}"
    );
}

/// The answer form's checks come from the host's list; the press sends the
/// CLI's AnswerManualCheck.
#[tokio::test]
async fn parity_a_check_answer_is_drivable() {
    let clock = TestClock::at(1_790_000_000);
    let host = MockHost::start(
        clock.clone(),
        Arc::new(|c: &homelab_proto::Command| {
            match c {
            homelab_proto::Command::ListManualChecks { .. } => Script {
                message: serde_json::json!({"checks": [
                    {"id": "c4bca102", "record": {"stack": "media", "app": "jellyfin", "text": "posters?", "registered_at": 1}}
                ]})
                .to_string(),
                ..Script::ok(&[])
            },
            _ => Script::ok(&[("answer", 1)]),
        }
        }),
        history("x", &[]),
        Arc::new(std::sync::Mutex::new(Vec::new())),
    );
    let live = Arc::new(Recorder::default());
    let dir = temp_dir("follow-answer");
    let notify = NotifyCenter::load(
        dir.join("notifications.json"),
        Arc::new(act_support::RecPusher::default()),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    let shared = shared(&[("media", 106, None)]);
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
    let s = |x: UiStep| {
        let d = driver.clone();
        async move { d.step("wsl", Scope::Operate, x).await }
    };
    let opened = s(open("answer-check", None)).await;
    assert_eq!(opened["ok"], true, "{opened}");
    let fields = opened["state"]["form"]["fields"]
        .as_array()
        .unwrap()
        .clone();
    let check = fields.iter().find(|f| f["id"] == "act-check").unwrap();
    assert_eq!(check["choices"], serde_json::json!(["c4bca102"]));
    let pick = |f: &str, v: &str| UiStep::Pick {
        field: f.into(),
        value: v.into(),
    };
    assert_eq!(s(pick("act-check", "c4bca102")).await["ok"], true);
    // fix-answer-days: the days are asked only with accept. Before it is
    // chosen the field is hidden (and says why), and typing in it is refused.
    let days = |v: &Value| {
        v["state"]["form"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["id"] == "act-days")
            .unwrap()
            .clone()
    };
    assert_eq!(days(&opened)["shown"], false);
    assert!(
        days(&opened)["hidden_why"]
            .as_str()
            .is_some_and(|w| w.contains("not ok, accepted for some days")),
        "{}",
        days(&opened)
    );
    let early = s(typed("act-days", "30")).await;
    let (_, why, _) = refused(&early);
    assert!(why.contains("act-days is not on screen"), "{why}");
    let picked = s(pick("act-verdict", "accept")).await;
    assert_eq!(picked["ok"], true);
    assert_eq!(days(&picked)["shown"], true);
    assert_eq!(days(&picked)["label"], "Number of days");
    // accept without days and reason: held by the form at next.
    let held = s(press("next")).await;
    assert!(
        held["refusal"]["why"]
            .as_str()
            .is_some_and(|w| w.contains("act-days") && w.contains("act-note")),
        "{held}"
    );
    assert_eq!(s(typed("act-days", "30")).await["ok"], true);
    assert_eq!(
        s(typed("act-note", "known, fixed next month")).await["ok"],
        true
    );
    s(press("next")).await;
    let pressed = s(press("confirm")).await;
    assert_eq!(pressed["ok"], true, "{pressed}");
    until("the answer was sent", || {
        host.ran()
            .iter()
            .any(|(_, n, _)| n == "answer_manual_check")
    })
    .await;
}

/// fix-185: `state.form.reads_repo` says whether a driven form's final
/// press reads the repository (`Needs::Spec`/`Needs::Apply`) — deploy and
/// the host-wide apply do, a backup does not, and a batch wrapping a
/// repo-reading action does too, by the action it wraps rather than its own
/// "batch" slug. `homelab ui` preflights a press only when this is set.
#[tokio::test]
async fn fix_185_reads_repo_names_the_actions_that_read_the_repository() {
    let w = world("reads-repo", MemFiles::default(), None);
    let s = |st| step(&w, st);

    let opened = s(open("deploy", Some("media"))).await;
    assert_eq!(opened["state"]["form"]["reads_repo"], true, "{opened}");
    s(press("close")).await;

    let opened = s(open("backup", Some("media"))).await;
    assert_eq!(opened["state"]["form"]["reads_repo"], false, "{opened}");
    s(press("close")).await;

    // The host-wide "apply" form (every declared stack against the host)
    // needs scope All.
    let opened = w.driver.step("wsl", Scope::All, open("apply", None)).await;
    assert_eq!(opened["state"]["form"]["reads_repo"], true, "{opened}");
    w.driver.step("wsl", Scope::All, press("close")).await;

    // A batch wrapping deploy reads the repository too, by the action it
    // wraps — a batch wrapping backup does not.
    let opened = s(UiStep::Open {
        form: "batch:deploy".into(),
        target: Some("media,drill".into()),
    })
    .await;
    assert_eq!(opened["state"]["form"]["reads_repo"], true, "{opened}");
    s(press("close")).await;
    let opened = s(UiStep::Open {
        form: "batch:backup".into(),
        target: Some("media,drill".into()),
    })
    .await;
    assert_eq!(opened["state"]["form"]["reads_repo"], false, "{opened}");
}
