//! milestone act against the mock host (act_support): the queue, the
//! progress events, a batch, the deploy guard, the notification center, the
//! scheduler and the routes. No real host, no kyu: pushes go to a recorder.

mod act_support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{
    MemFiles, MockHost, RecPusher, Recorder, Script, TestClock, history, shared, temp_dir, until,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use homelab_admin::core::actions::{ActionArgs, BatchRequest, validate};
use homelab_admin::core::schedule::{ScheduleInput, When, resolve_local};
use homelab_admin::shell::actions::{
    Actions, ActionsDeps, CommitInfo, JobState, Origin, PauseGate, Publish, router,
};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::scheduler::Scheduler;
use homelab_core::ops::deployguard::Ancestry;
use homelab_proto::Command;
use tower::ServiceExt as _;

struct World {
    clock: TestClock,
    host: Arc<MockHost>,
    live: Arc<Recorder>,
    pusher: Arc<RecPusher>,
    notify: Arc<NotifyCenter>,
    actions: Actions,
    incidents: Arc<Mutex<Vec<String>>>,
    dir: std::path::PathBuf,
}

fn world(tag: &str, files: MemFiles, behaviour: act_support::Behaviour) -> World {
    let clock = TestClock::at(1_790_000_000);
    let incidents = Arc::new(Mutex::new(Vec::new()));
    let host = MockHost::start(
        clock.clone(),
        behaviour,
        history(
            "deploy-media",
            &[
                (&[("pull", 100), ("up", 60), ("verify", 20)], true),
                (&[("pull", 140), ("up", 80), ("verify", 20)], true),
            ],
        ),
        incidents.clone(),
    );
    let live = Arc::new(Recorder::default());
    let pusher = Arc::new(RecPusher::default());
    let dir = temp_dir(tag);
    let notify = NotifyCenter::load(
        dir.join("notifications.json"),
        pusher.clone(),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: live.clone(),
        files: Arc::new(files),
        shared: shared(&[
            ("media", 106, Some("a1b2c3d4e5f6")),
            ("home", 107, None),
            ("admin", 120, None),
        ]),
        notify: notify.clone(),
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    });
    World {
        clock,
        host,
        live,
        pusher,
        notify,
        actions,
        incidents,
        dir,
    }
}

fn steady() -> act_support::Behaviour {
    Arc::new(|_c: &Command| Script::ok(&[("pull", 120), ("up", 70), ("verify", 20)]))
}

async fn finished(w: &World, job: u64) -> homelab_admin::shell::actions::JobView {
    until("the job to finish", || {
        w.actions.job(job).is_some_and(|j| j.state.finished())
    })
    .await;
    w.actions.job(job).unwrap()
}

#[tokio::test]
async fn feat_stacks_4_a_press_runs_on_the_host_and_its_lines_and_progress_arrive() {
    let w = world("press", MemFiles::default(), steady());
    let req = validate("media", "deploy", ActionArgs::default()).unwrap();
    let queued = w.actions.submit(req, Origin::Manual);
    assert_eq!(queued.state, JobState::Queued);
    let done = finished(&w, queued.job).await;
    assert_eq!(done.state, JobState::Done, "{:?}", done.message);
    assert_eq!(done.cli.as_deref(), Some("homelab deploy media"));
    let ran = w.host.ran();
    assert_eq!(ran.len(), 1);
    assert_eq!(
        (ran[0].1.as_str(), ran[0].2.as_str()),
        ("deploy_stack", "media")
    );
    assert_eq!(done.reqs, vec![ran[0].0]);
    // Only this request's lines: the CLI's line (req 1) is not in the job.
    let logs = w.live.events("action_log");
    assert!(!logs.is_empty());
    assert!(
        logs.iter()
            .all(|l| l["req"] == ran[0].0 && l["job"] == done.job)
    );
    assert!(logs.iter().any(|l| l["msg"] == "working on up"));
    // feat-ops-6: step n/m with the medians of the two past runs.
    let progress = w.live.events("action_progress");
    let first = &progress[0]["progress"];
    assert_eq!(
        (first["n"].as_u64(), first["m"].as_u64()),
        (Some(1), Some(3))
    );
    assert_eq!(first["expected_step_s"], 120, "median of 100 and 140");
    assert_eq!(first["expected_total_s"], 210, "median of 180 and 240");
    let last = &progress.last().unwrap()["progress"];
    assert_eq!(
        (last["n"].as_u64(), last["finished"].as_bool()),
        (Some(3), Some(true))
    );
    assert_eq!(last["elapsed_s"], 210);
    // The states went out in order.
    let states: Vec<String> = w
        .live
        .events("action")
        .iter()
        .filter(|a| a["job"] == done.job)
        .map(|a| a["state"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(states.first().map(String::as_str), Some("queued"));
    assert_eq!(states.last().map(String::as_str), Some("done"));
    assert!(states.contains(&"running".to_string()));
    // Decision notify-routing (2026-09-30) replaced feat-ops-8's "a long
    // action pushes": a success lands in the list only.
    until("the notice", || !w.live.events("notification").is_empty()).await;
    assert!(w.pusher.sent.lock().unwrap().is_empty());
    let n = &w.notify.snapshot().await["notices"][0];
    assert_eq!(n["level"], "ok");
    assert_eq!(n["link"], "/stacks/media");
}

#[tokio::test]
async fn feat_stacks_4_a_short_action_lands_in_the_list_without_a_push() {
    let w = world(
        "short",
        MemFiles::default(),
        Arc::new(|_c: &Command| Script::ok(&[("flip", 1)])),
    );
    let job = w.actions.submit(
        validate("home", "disable", ActionArgs::default()).unwrap(),
        Origin::Manual,
    );
    assert_eq!(finished(&w, job.job).await.state, JobState::Done);
    until("the notice", || !w.live.events("notification").is_empty()).await;
    assert!(w.pusher.sent.lock().unwrap().is_empty());
    let list = w.notify.snapshot().await;
    assert_eq!(list["unread"], 1);
    assert_eq!(list["notices"][0]["push"]["state"], "skipped");
    assert_eq!(list["notices"][0]["job"], job.job);
    // arch-state: the list is on disk and reads back.
    let reread = NotifyCenter::load(
        w.dir.join("notifications.json"),
        w.pusher.clone(),
        w.live.clone(),
        w.clock.clock(),
    )
    .unwrap();
    assert_eq!(reread.snapshot().await["unread"], 1);
    let raw = std::fs::read_to_string(w.dir.join("notifications.json")).unwrap();
    assert!(raw.contains("\"schema_version\": 1"), "{raw}");
}

#[tokio::test]
async fn feat_stacks_5_one_failure_in_a_batch_does_not_hide_the_others() {
    let w = world(
        "batch",
        MemFiles::default(),
        Arc::new(|c: &Command| {
            if act_support::stack_of(c) == "home" {
                Script::failed(&[("snapshot", 5)], "restic: repository locked")
            } else {
                Script::ok(&[("snapshot", 5)])
            }
        }),
    );
    let reqs = homelab_admin::core::actions::validate_batch(BatchRequest {
        action: "backup".into(),
        stacks: vec!["media".into(), "home".into(), "admin".into()],
        args: ActionArgs::default(),
        confirms: Default::default(),
    })
    .unwrap();
    let (batch, jobs) = w.actions.submit_batch(reqs);
    for j in &jobs {
        finished(&w, j.job).await;
    }
    let ran: Vec<String> = w.host.ran().into_iter().map(|r| r.2).collect();
    assert_eq!(ran, vec!["media", "home", "admin"], "in the order given");
    let states: Vec<JobState> = jobs
        .iter()
        .map(|j| w.actions.job(j.job).unwrap().state)
        .collect();
    assert_eq!(
        states,
        vec![JobState::Done, JobState::Failed, JobState::Done]
    );
    let last = w.live.events("action_batch").last().cloned().unwrap();
    assert_eq!(last["batch"], batch);
    assert_eq!(
        (
            last["done"].as_bool(),
            last["ok"].as_u64(),
            last["failed"].as_u64()
        ),
        (Some(true), Some(2), Some(1))
    );
    assert!(
        w.actions
            .job(jobs[1].job)
            .unwrap()
            .message
            .unwrap()
            .contains("locked")
    );
}

/// fix-172 (Kenny, 2026-10-02 06:55): a viewer's Pause in Live view during a
/// driven batch ("Install newest release" on kyu and almanac) did nothing —
/// the batch ran to its end regardless, and Continue afterwards changed
/// nothing because there was nothing actually held. A stand-in for the
/// driver (the real gate lives in `shell::drive::Driver`, which `mount`
/// wires up; this test only needs the queue's side of the contract).
struct FakeGate {
    paused: Mutex<Option<String>>,
    notify: Arc<tokio::sync::Notify>,
}

impl FakeGate {
    fn new() -> Arc<Self> {
        Arc::new(FakeGate {
            paused: Mutex::new(None),
            notify: Arc::new(tokio::sync::Notify::new()),
        })
    }
    fn pause(&self, who: &str) {
        *self.paused.lock().unwrap() = Some(who.into());
        self.notify.notify_waiters();
    }
    fn resume(&self) {
        *self.paused.lock().unwrap() = None;
        self.notify.notify_waiters();
    }
}

impl PauseGate for FakeGate {
    fn paused_by(&self) -> Option<String> {
        self.paused.lock().unwrap().clone()
    }
    fn notified(&self) -> Arc<tokio::sync::Notify> {
        self.notify.clone()
    }
}

#[tokio::test]
async fn fix_172_pause_holds_a_batch_at_the_boundary_before_its_next_job() {
    let w = world(
        "pause-batch",
        MemFiles::default(),
        Arc::new(|_c: &Command| Script::ok(&[("snapshot", 5)])),
    );
    let gate = FakeGate::new();
    w.actions.set_pause_gate(gate.clone());
    // Paused before the batch is even submitted: on the OLD code (no wait
    // in the worker loop) this made no difference and every job ran at
    // once, so this assertion fails without the fix.
    gate.pause("the viewer kenny");
    let reqs = homelab_admin::core::actions::validate_batch(BatchRequest {
        action: "backup".into(),
        stacks: vec!["media".into(), "home".into()],
        args: ActionArgs::default(),
        confirms: Default::default(),
    })
    .unwrap();
    let (_batch, jobs) = w.actions.submit_batch(reqs);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        w.host.ran().is_empty(),
        "paused: the queue must not have run a single job yet"
    );
    assert_eq!(w.actions.job(jobs[0].job).unwrap().state, JobState::Queued);

    // Continue releases it: both jobs run, in order, same as an unpaused
    // batch would.
    gate.resume();
    for j in &jobs {
        finished(&w, j.job).await;
    }
    let ran: Vec<String> = w.host.ran().into_iter().map(|r| r.2).collect();
    assert_eq!(ran, vec!["media", "home"]);
    let states: Vec<JobState> = jobs
        .iter()
        .map(|j| w.actions.job(j.job).unwrap().state)
        .collect();
    assert_eq!(states, vec![JobState::Done, JobState::Done]);
}

/// fix-172: a queue nobody ever registered a gate for (every other test in
/// this file, and a dashboard before `mount` wires the driver in) must
/// never wait — `PauseGate` is optional, not a new way to deadlock the
/// queue.
#[tokio::test]
async fn fix_172_a_queue_without_a_pause_gate_never_waits() {
    let w = world("no-gate", MemFiles::default(), steady());
    let view = w.actions.submit(
        homelab_admin::core::actions::ActionRequest {
            stack: "media".into(),
            action: homelab_admin::core::actions::ActionKind::Backup,
            args: ActionArgs::default(),
        },
        Origin::Manual,
    );
    let j = finished(&w, view.job).await;
    assert_eq!(j.state, JobState::Done);
}

#[tokio::test]
async fn arch_deploy_guard_refuses_over_a_commit_the_working_copy_lacks_unless_forced() {
    let w = world(
        "guard",
        MemFiles {
            ancestry: Ancestry::Unknown,
            ..Default::default()
        },
        steady(),
    );
    let job = w.actions.submit(
        validate("media", "deploy", ActionArgs::default()).unwrap(),
        Origin::Manual,
    );
    let v = finished(&w, job.job).await;
    assert_eq!(v.state, JobState::Refused);
    assert!(v.message.unwrap().contains("a1b2c3d4e5f6"));
    assert!(w.host.ran().is_empty(), "nothing reached the host");
    let forced = w.actions.submit(
        validate(
            "media",
            "deploy",
            ActionArgs {
                force: true,
                ..Default::default()
            },
        )
        .unwrap(),
        Origin::Manual,
    );
    let v = finished(&w, forced.job).await;
    assert_eq!(v.state, JobState::Done);
    assert_eq!(v.cli.as_deref(), Some("homelab deploy media --force"));
    assert_eq!(w.host.ran().len(), 1);
}

#[tokio::test]
async fn arch_host_link_a_dropped_line_is_outcome_unknown_never_resent() {
    let w = world(
        "drop",
        MemFiles::default(),
        Arc::new(|_c: &Command| Script {
            drop_line: true,
            ..Script::ok(&[("pull", 5)])
        }),
    );
    let job = w.actions.submit(
        validate("media", "update", ActionArgs::default()).unwrap(),
        Origin::Manual,
    );
    let v = finished(&w, job.job).await;
    assert_eq!(v.state, JobState::Unknown);
    assert_eq!(w.host.ran().len(), 1, "sent once");
}

#[tokio::test]
async fn feat_stacks_6_roll_back_to_a_commit_or_the_kept_binary() {
    let w = world(
        "rollback",
        MemFiles {
            commits: vec![
                CommitInfo {
                    commit: "a1b2c3d4e5f6a7b8c9d0a1b2c3d4e5f6a7b8c9d0".into(),
                    at: 1_790_000_000,
                    subject: "media: pin jellyfin".into(),
                },
                CommitInfo {
                    commit: "0f0e0d0c0b0a09080706050403020100ffeeddcc".into(),
                    at: 1_789_000_000,
                    subject: "media: first".into(),
                },
            ],
            ..Default::default()
        },
        steady(),
    );
    let o = w.actions.rollback_options("media".into()).await.unwrap();
    assert_eq!(o["applied_commit"], "a1b2c3d4e5f6");
    assert_eq!(o["commits"][0]["applied"], true);
    assert_eq!(o["commits"][1]["applied"], false);
    assert!(o["missing"].as_array().unwrap().len() >= 2);
    let job = w.actions.submit(
        validate(
            "media",
            "deploy-commit",
            ActionArgs {
                commit: Some("0f0e0d0c0b0a".into()),
                ..Default::default()
            },
        )
        .unwrap(),
        Origin::Manual,
    );
    let v = finished(&w, job.job).await;
    assert_eq!(v.state, JobState::Done);
    assert_eq!(
        v.cli.as_deref(),
        Some("git checkout 0f0e0d0c0b0a -- stacks/media && homelab deploy media")
    );
    let native = w.actions.submit(
        validate("admin", "rollback-native", ActionArgs::default()).unwrap(),
        Origin::Manual,
    );
    assert!(native.restarts_dashboard);
    finished(&w, native.job).await;
    assert_eq!(w.host.ran().last().unwrap().1, "rollback_native");
}

#[tokio::test]
async fn feat_overview_5_new_incidents_become_notices_old_ones_do_not() {
    let w = world("incidents", MemFiles::default(), steady());
    w.incidents
        .lock()
        .unwrap()
        .push("1790000000-deploy-media".into());
    assert_eq!(
        w.notify
            .incidents(vec!["1790000000-deploy-media".into()])
            .await,
        0
    );
    assert_eq!(
        w.notify
            .incidents(vec![
                "1790000000-deploy-media".into(),
                "1790000500-backup-home".into()
            ])
            .await,
        1
    );
    let list = w.notify.snapshot().await;
    assert_eq!(list["notices"][0]["kind"], "incident");
    // Decision notify-routing (2026-09-30): the host pushed the failure
    // itself; the incident is not pushed a second time.
    assert!(w.pusher.sent.lock().unwrap().is_empty());
    assert!(
        list["notices"][0]["push"]["why"]
            .as_str()
            .unwrap()
            .contains("host")
    );
    // Snoozed: stored, not pushed, not popped up.
    w.notify.snooze(3_600).await.unwrap();
    w.notify
        .incidents(vec!["1790000900-update-media".into()])
        .await;
    assert!(w.pusher.sent.lock().unwrap().is_empty());
    let pops = w.live.events("notification");
    assert_eq!(pops.last().unwrap()["pop_up"], false);
    assert_eq!(w.notify.snapshot().await["snoozed"], true);
    assert_eq!(w.notify.mark(None, true).await, 2);
}

#[tokio::test]
async fn arch_schedule_a_due_slot_is_queued_a_missed_one_is_notified() {
    let w = world("sched", MemFiles::default(), steady());
    let slot = resolve_local(2026, 9, 28, 3, 30).instant();
    w.clock.set(slot - 3_600);
    let s = Scheduler::load(
        w.dir.join("schedules.json"),
        w.actions.clone(),
        w.notify.clone(),
        w.live.clone(),
        w.clock.clock(),
        300,
    )
    .unwrap();
    let created = s
        .create(ScheduleInput {
            stack: "media".into(),
            action: "backup".into(),
            args: ActionArgs::default(),
            when: When::Day { at: "03:30".into() },
            enabled: true,
            note: "nightly extra".into(),
        })
        .await
        .unwrap();
    assert_eq!(created["next_run"], slot);
    assert_eq!(created["next_run_local"], "2026-09-28 03:30");
    let id = created["schedule"]["id"].as_str().unwrap().to_string();
    // Before the slot: nothing.
    s.tick().await;
    assert!(w.host.ran().is_empty());
    // Just after: queued as an ordinary job.
    w.clock.set(slot + 5);
    s.tick().await;
    until("the scheduled backup", || !w.host.ran().is_empty()).await;
    assert_eq!(w.host.ran()[0].1, "backup_stack");
    let job = w.actions.jobs().into_iter().next().unwrap();
    assert!(
        matches!(job.origin, Origin::Schedule { ref schedule, slot: s2 } if *schedule == id && s2 == slot)
    );
    finished(&w, job.job).await;
    // Down over the next slot: skipped and notified, never run late.
    w.clock.set(slot + 86_400 + 7_200);
    s.tick().await;
    assert_eq!(w.host.ran().len(), 1, "a missed slot is not caught up");
    until("the missed notice", || {
        w.live
            .events("notification")
            .iter()
            .any(|n| n["notice"]["kind"] == "schedule_missed")
    })
    .await;
    // arch-state: the file reads back with the schedule and its progress.
    let reread = Scheduler::load(
        w.dir.join("schedules.json"),
        w.actions.clone(),
        w.notify.clone(),
        w.live.clone(),
        w.clock.clock(),
        300,
    )
    .unwrap();
    let list = reread.list().await;
    assert_eq!(list["zone"], "Europe/Brussels");
    assert_eq!(
        list["schedules"][0]["schedule"]["handled_until"],
        slot + 86_400 + 7_200
    );
    assert_eq!(list["schedules"][0]["schedule"]["last_run"]["slot"], slot);
    // A typed-name action cannot be planned; a schedule can be removed.
    assert!(
        s.create(ScheduleInput {
            stack: "media".into(),
            action: "destroy".into(),
            args: ActionArgs {
                confirm: Some("media".into()),
                ..Default::default()
            },
            when: When::Day { at: "03:30".into() },
            enabled: true,
            note: String::new(),
        })
        .await
        .is_err()
    );
    s.delete(&id).await.unwrap();
    assert!(s.delete(&id).await.is_err());
}

/// redesign-schedules-2/3: a skipped slot is neither run nor notified, and
/// the newest slot that passed without a run is remembered with its reason
/// for the page's "Last run" column.
/// covers: redesign-schedules-2, redesign-schedules-3
#[tokio::test]
async fn redesign_schedules_a_skipped_slot_is_quiet_and_a_missed_one_is_remembered() {
    let w = world("sched-skip", MemFiles::default(), steady());
    let slot = resolve_local(2026, 9, 28, 3, 30).instant();
    w.clock.set(slot - 3_600);
    let s = Scheduler::load(
        w.dir.join("schedules.json"),
        w.actions.clone(),
        w.notify.clone(),
        w.live.clone(),
        w.clock.clock(),
        300,
    )
    .unwrap();
    let created = s
        .create(ScheduleInput {
            stack: "media".into(),
            action: "backup".into(),
            args: ActionArgs::default(),
            when: When::Day { at: "03:30".into() },
            enabled: true,
            note: String::new(),
        })
        .await
        .unwrap();
    let id = created["schedule"]["id"].as_str().unwrap().to_string();
    // Only the next slot can be skipped.
    assert!(s.skip(&id, slot + 86_400).await.is_err());
    let view = s.skip(&id, slot).await.unwrap();
    assert_eq!(view["next_run"], slot + 86_400);
    w.clock.set(slot + 5);
    s.tick().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(w.host.ran().is_empty(), "a skipped slot ran");
    assert!(
        !w.live
            .events("notification")
            .iter()
            .any(|n| n["notice"]["kind"] == "schedule_missed"),
        "a skipped slot was notified as missed"
    );
    let list = s.list().await;
    assert!(list["schedules"][0]["schedule"]["last_missed"].is_null());
    // Down over the next slot: remembered as missed, and why.
    w.clock.set(slot + 86_400 + 7_200);
    s.tick().await;
    let list = s.list().await;
    assert_eq!(
        list["schedules"][0]["schedule"]["last_missed"]["slot"],
        slot + 86_400
    );
    assert_eq!(
        list["schedules"][0]["schedule"]["last_missed"]["why"],
        "down"
    );
    assert!(s.skip("nope", slot).await.is_err());
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let resp = app
        .oneshot(
            req.body(Body::from(body.unwrap_or("").to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn feat_stacks_4_the_routes_answer_at_once_or_say_what_why_and_fix() {
    let w = world("routes", MemFiles::default(), steady());
    let app = router(w.actions.clone());
    let (st, v) = call(app.clone(), "GET", "/data/actions/catalog", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["host_target"], "_host");
    assert!(v["actions"].as_array().unwrap().len() >= 20);
    let (st, v) = call(app.clone(), "POST", "/data/actions/media/backup", None).await;
    assert_eq!(st, StatusCode::ACCEPTED, "{v}");
    assert!(v["job"].is_u64() && v["state"] == "queued");
    finished(&w, v["job"].as_u64().unwrap()).await;
    let (st, v) = call(
        app.clone(),
        "POST",
        "/data/actions/admin/destroy",
        Some(r#"{"confirm":"admin"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(v["why"].as_str().unwrap().contains("arch-self"));
    let (st, v) = call(
        app.clone(),
        "POST",
        "/data/actions/media/deploy",
        Some("{nope"),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    for k in ["what", "why", "fix"] {
        assert!(v[k].is_string(), "{k} in {v}");
    }
    let (st, v) = call(
        app.clone(),
        "POST",
        "/data/actions/media/backup/preview",
        Some("{}"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["cli"], "homelab backup media");
    let (st, v) = call(
        app.clone(),
        "POST",
        "/data/actions/batch",
        Some(r#"{"action":"update","stacks":["media","home"]}"#),
    )
    .await;
    assert_eq!(st, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["jobs"].as_array().unwrap().len(), 2);
    let (st, v) = call(app.clone(), "GET", "/data/actions/jobs", None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(v["jobs"].as_array().unwrap().len() >= 3);
    let (st, v) = call(app, "GET", "/data/actions/media/rollback-options", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["stack"], "media");
}

#[tokio::test]
async fn feat_stacks_4_without_a_working_copy_file_actions_are_refused_at_once() {
    let w = world(
        "norepo",
        MemFiles {
            present: false,
            ..Default::default()
        },
        steady(),
    );
    let app = router(w.actions.clone());
    let (st, v) = call(app.clone(), "POST", "/data/actions/media/backup", None).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert!(v["why"].as_str().unwrap().contains("working copy"));
    // Actions that need only the name still run.
    let (st, _) = call(app, "POST", "/data/actions/kyu/backup-native", None).await;
    assert_eq!(st, StatusCode::ACCEPTED);
}

#[test]
fn recorder_is_a_publish() {
    let r = Recorder::default();
    r.publish("x", serde_json::json!(1));
    assert_eq!(r.events("x").len(), 1);
}
