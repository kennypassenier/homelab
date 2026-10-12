//! The 3.74.0 open-issues round, server half (REGISTER.md fix-watch-1,
//! fix-batch-native-1, feat-batch-1, fix-live-1, fix-stale-refresh-1,
//! feat-logs-1).

mod act_support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{
    MemFiles, MockHost, RecPusher, Recorder, Script, TestClock, history, temp_dir, until,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use homelab_admin::core::actions::{
    ActionArgs, ActionKind, BatchRequest, native_batch_backups, validate, validate_batch,
};
use homelab_admin::core::notify::{Draft, Kind, Level, NotifyFile};
use homelab_admin::shell::actions::{Actions, ActionsDeps, JobState, Origin, PauseGate, router};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_proto::Command;
use tower::ServiceExt as _;

struct Gate {
    paused: Mutex<Option<String>>,
    notify: Arc<tokio::sync::Notify>,
}
impl PauseGate for Gate {
    fn paused_by(&self) -> Option<String> {
        self.paused.lock().unwrap().clone()
    }
    fn notified(&self) -> Arc<tokio::sync::Notify> {
        self.notify.clone()
    }
}

struct World {
    live: Arc<Recorder>,
    host: Arc<MockHost>,
    actions: Actions,
    notify: Arc<NotifyCenter>,
    pusher: Arc<RecPusher>,
    gate: Arc<Gate>,
}

/// `native`: the stacks the fleet says run native services. Starts paused,
/// so submitted jobs stay queued until `resume`.
fn world(tag: &str, native: &[&str]) -> World {
    let clock = TestClock::at(1_790_000_000);
    let host = MockHost::start(
        clock.clone(),
        Arc::new(|_c: &Command| Script::ok(&[("step", 5)])),
        history("deploy-media", &[(&[("up", 60)], true)]),
        Arc::new(Mutex::new(Vec::new())),
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
    let names = ["media", "home", "inbox", "kyu"];
    let state: homelab_proto::FleetState = serde_json::from_value(serde_json::json!({
        "host": {"name": "pve", "cpu_pct": 1, "ram_pct": 1, "disk_pct": 1, "tls_fingerprint": ""},
        "stacks": names.iter().enumerate().map(|(i, n)| serde_json::json!({
            "name": n, "vmid": 100 + i, "hostname": n, "apps": [], "drift": false,
            "env_sealed": true, "online": true, "enabled": true,
            "native": native.contains(n),
        })).collect::<Vec<_>>(),
    }))
    .unwrap();
    let shared = Arc::new(tokio::sync::RwLock::new(
        homelab_admin::shell::host_link::Snapshot {
            fleet: Some(homelab_admin::core::fleet::fleet_view(&state, 1)),
            ..Default::default()
        },
    ));
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: live.clone(),
        files: Arc::new(MemFiles::default()),
        shared,
        notify: notify.clone(),
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    });
    let gate = Arc::new(Gate {
        paused: Mutex::new(Some("test".into())),
        notify: Arc::new(tokio::sync::Notify::new()),
    });
    actions.set_pause_gate(gate.clone());
    World {
        live,
        host,
        actions,
        notify,
        pusher,
        gate,
    }
}

impl World {
    fn resume(&self) {
        *self.gate.paused.lock().unwrap() = None;
        self.gate.notify.notify_waiters();
    }
}

async fn call(app: axum::Router, method: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 22)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

fn down(key: &str, why: &str) -> Draft {
    let mut d = Draft::new(
        Kind::Down,
        &format!("watch-{key}"),
        "Proxmox does not answer",
        why,
    );
    d.detail.level = Level::Critical;
    d.detail.key = Some(key.to_string());
    d
}

// ── fix-watch-1 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn fix_watch_1_a_down_still_open_is_not_added_again() {
    let w = world("watch-down", &[]);
    let a = w.notify.notify(down("tile:pve", "first reason")).await;
    let b = w.notify.notify(down("tile:pve", "second reason")).await;
    assert_eq!(a.id, b.id, "the open Down is kept, not repeated");
    let all = w.notify.snapshot().await;
    let downs: Vec<_> = all["notices"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["title"] == "Proxmox does not answer")
        .collect();
    assert_eq!(downs.len(), 1);
    assert_eq!(downs[0]["body"], "second reason", "the newest reason shows");
    assert_eq!(w.pusher.sent.lock().unwrap().len(), 1, "pushed once");
    // A different tile is its own Down; a read one may be announced again.
    w.notify.notify(down("tile:opnsense", "x")).await;
    assert_eq!(
        w.notify.snapshot().await["notices"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn fix_watch_1_a_read_down_may_be_announced_again() {
    let mut f = NotifyFile::default();
    let d = down("tile:pve", "r");
    assert!(f.refresh_open_down(&d).is_none(), "nothing open yet");
    f.add(
        d.clone(),
        1,
        homelab_admin::core::notify::PushOutcome::Sent,
        100,
        0,
    );
    assert!(f.refresh_open_down(&d).is_some());
    f.mark(None, true);
    assert!(
        f.refresh_open_down(&d).is_none(),
        "read: a new outage is news"
    );
}

// ── fix-batch-native-1 ──────────────────────────────────────────────────

#[test]
fn fix_batch_native_1_a_native_stack_backs_up_with_backup_native() {
    let reqs = validate_batch(BatchRequest {
        action: "backup".into(),
        stacks: vec!["media".into(), "inbox".into()],
        args: ActionArgs::default(),
        confirms: Default::default(),
    })
    .unwrap();
    let out = native_batch_backups(reqs, &|s| s == "inbox").unwrap();
    assert_eq!(out[0].action, ActionKind::Backup);
    assert_eq!(out[1].action, ActionKind::BackupNative);
    assert_eq!(out[1].stack, "inbox");
}

#[tokio::test]
async fn fix_batch_native_1_run_batch_sends_backup_native_to_the_host() {
    let w = world("batch-native", &["inbox", "kyu"]);
    w.resume();
    let v = w
        .actions
        .run_batch(BatchRequest {
            action: "backup".into(),
            stacks: vec!["inbox".into(), "kyu".into()],
            args: ActionArgs::default(),
            confirms: Default::default(),
        })
        .map_err(|e| e.1)
        .unwrap();
    for j in v["jobs"].as_array().unwrap() {
        let id = j["job"].as_u64().unwrap();
        until("job", || {
            w.actions.job(id).is_some_and(|j| j.state.finished())
        })
        .await;
        assert_eq!(w.actions.job(id).unwrap().action, ActionKind::BackupNative);
    }
    let ran = w.host.ran();
    assert!(
        ran.iter().all(|r| r.1.to_lowercase().contains("native")),
        "{ran:?}"
    );
}

// ── feat-batch-1 ────────────────────────────────────────────────────────

fn batch_of(w: &World, stacks: &[&str]) -> (u64, Vec<u64>) {
    let reqs = validate_batch(BatchRequest {
        action: "backup".into(),
        stacks: stacks.iter().map(|s| s.to_string()).collect(),
        args: ActionArgs::default(),
        confirms: Default::default(),
    })
    .unwrap();
    let (b, jobs) = w.actions.submit_batch(reqs);
    (b, jobs.iter().map(|j| j.job).collect())
}

#[tokio::test]
async fn feat_batch_1_get_batch_and_jobs_list_carry_the_batch() {
    let w = world("batch-get", &[]);
    let (b, jobs) = batch_of(&w, &["media", "home"]);
    let (st, v) = call(
        router(w.actions.clone()),
        "GET",
        &format!("/data/actions/batch/{b}"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["batch"], b);
    assert_eq!(v["jobs"].as_array().unwrap().len(), 2);
    assert_eq!(v["done"], false);
    let (st, l) = call(router(w.actions.clone()), "GET", "/data/actions/jobs").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(l["batches"][0]["batch"], b);
    let j = l["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["job"] == jobs[0])
        .unwrap();
    assert_eq!(
        j["origin"],
        serde_json::json!({"from": "batch", "batch": b})
    );
    let (st, _) = call(
        router(w.actions.clone()),
        "GET",
        "/data/actions/batch/999999999",
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feat_batch_1_cancel_one_queued_job_and_the_whole_batch() {
    let w = world("batch-cancel", &[]);
    let (b, jobs) = batch_of(&w, &["media", "home", "inbox"]);
    let before = w.live.events("action").len();
    let (st, v) = call(
        router(w.actions.clone()),
        "POST",
        &format!("/data/actions/jobs/{}/cancel", jobs[1]),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["state"], "refused");
    assert!(v["message"].as_str().unwrap().contains("cancelled"));
    assert!(
        w.live.events("action").len() > before,
        "an action event went out"
    );
    assert_eq!(
        w.live.events("action_batch").last().unwrap()["jobs"][1]["state"],
        "refused"
    );
    // Cancelling it again: not queued any more.
    let (st, _) = call(
        router(w.actions.clone()),
        "POST",
        &format!("/data/actions/jobs/{}/cancel", jobs[1]),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, _) = call(
        router(w.actions.clone()),
        "POST",
        "/data/actions/jobs/999999999/cancel",
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // The whole batch: the rest of the queued jobs.
    let (st, v) = call(
        router(w.actions.clone()),
        "POST",
        &format!("/data/actions/batch/{b}/cancel"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(v["cancelled"], serde_json::json!([jobs[0], jobs[2]]));
    w.resume();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(w.host.ran().is_empty(), "no cancelled job reached the host");
    assert_eq!(w.live.events("action_batch").last().unwrap()["done"], true);
    let (st, _) = call(
        router(w.actions.clone()),
        "POST",
        "/data/actions/batch/999999999/cancel",
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

// ── feat-logs-1 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn feat_logs_1_the_whole_log_of_a_job_is_retrievable() {
    let w = world("job-log", &[]);
    w.resume();
    let v = w.actions.submit(
        validate("media", "backup", ActionArgs::default()).unwrap(),
        Origin::Manual,
    );
    until("job", || {
        w.actions.job(v.job).is_some_and(|j| j.state.finished())
    })
    .await;
    let live: Vec<_> = w
        .live
        .events("action_log")
        .into_iter()
        .filter(|l| l["job"] == v.job)
        .collect();
    let (st, l) = call(
        router(w.actions.clone()),
        "GET",
        &format!("/data/actions/jobs/{}/log", v.job),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(l["lines"].as_array().unwrap(), &live);
    assert_eq!(l["max"], homelab_admin::shell::actions::JOB_LOG_MAX);
    let (st, _) = call(
        router(w.actions.clone()),
        "GET",
        "/data/actions/jobs/999999999/log",
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}
const _: () = assert!(homelab_admin::shell::actions::JOB_LOG_MAX > 5000);

// ── fix-stale-refresh-1 ─────────────────────────────────────────────────

fn check_answer() -> serde_json::Value {
    serde_json::json!({
        "measured_at": 1,
        "findings": [
            {"subject": "cloud/cloudflared, media/jellyfin", "what": "pinned to 2026.9.0; upstream x released 2026.10.0 on 2026-10-01"},
            {"subject": "cloud/other", "what": "something else"},
            {"subject": "cloud/tunnel", "what": "pinned to 1; upstream y released 2"},
        ],
    })
}

#[tokio::test]
async fn fix_stale_refresh_1_a_settled_stack_drops_its_stale_rows_and_announces() {
    let live = Arc::new(Recorder::default());
    let read = homelab_admin::shell::slow::SlowRead::announced(
        "the fleet check",
        "fleet-check",
        live.clone(),
    );
    let first = read
        .read(None, Duration::from_secs(2), || async {
            (StatusCode::OK, check_answer())
        })
        .await;
    assert_eq!(first.status(), StatusCode::OK);
    let before = live.events("slow_read").len();
    assert!(read.drop_stale_rows_of("cloud"));
    let ev = live.events("slow_read");
    assert_eq!(ev.len(), before + 1);
    assert_eq!(ev.last().unwrap()["read"], "fleet-check");
    assert_eq!(ev.last().unwrap()["stack"], "cloud");
    let kept = axum::body::to_bytes(read.kept().into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&kept).unwrap();
    let f = v["findings"].as_array().unwrap();
    assert_eq!(f.len(), 2, "cloud's pin rows are gone, other findings stay");
    assert_eq!(f[0]["subject"], "media/jellyfin");
    assert_eq!(f[1]["what"], "something else");
    assert_eq!(
        v["read_run"],
        ev.last().unwrap()["run"],
        "a newer run id than the check's"
    );
    assert!(
        !read.drop_stale_rows_of("cloud"),
        "nothing left to drop, no event"
    );
}

#[tokio::test]
async fn fix_stale_refresh_1_a_deploy_that_finished_well_tells_the_listener() {
    let w = world("settled", &[]);
    let told = Arc::new(Mutex::new(Vec::<String>::new()));
    let t = told.clone();
    w.actions
        .on_stack_settled(Arc::new(move |s| t.lock().unwrap().push(s.to_string())));
    w.resume();
    let a = w.actions.submit(
        validate("media", "backup", ActionArgs::default()).unwrap(),
        Origin::Manual,
    );
    until("backup", || {
        w.actions.job(a.job).is_some_and(|j| j.state.finished())
    })
    .await;
    assert!(told.lock().unwrap().is_empty(), "a backup changes nothing");
    let d = w.actions.submit(
        validate("media", "update", ActionArgs::default()).unwrap(),
        Origin::Manual,
    );
    until("update", || {
        w.actions.job(d.job).is_some_and(|j| j.state.finished())
    })
    .await;
    if w.actions.job(d.job).unwrap().state == JobState::Done {
        assert_eq!(*told.lock().unwrap(), vec!["media".to_string()]);
    }
}

#[test]
fn fix_stale_refresh_1_the_update_refusal_offers_the_deploy() {
    use homelab_admin::core::stackedit::StackTexts;
    let mut t = StackTexts::new();
    t.insert(
        "cloud/docker-compose.yml".into(),
        "services:\n  cloudflared:\n    image: cloudflare/cloudflared:2026.10.0@sha256:abc\n"
            .into(),
    );
    let resolve: Box<homelab_admin::shell::pinupdate::ResolveFn> =
        Box::new(|_, _, _| Ok(Some("sha256:x".into())));
    let r = homelab_admin::shell::pinupdate::plan_target(
        "cloud",
        &t,
        "cloud/cloudflared",
        "2026.10.0",
        &*resolve,
    )
    .unwrap_err();
    assert!(r.why.contains("already names 2026.10.0"), "{}", r.why);
    let fix = r.fix_action.as_ref().expect("a deploy button");
    assert_eq!(
        (fix.action.as_str(), fix.stack.as_str()),
        ("deploy", "cloud")
    );
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(
        json["fix_action"],
        serde_json::json!({"action": "deploy", "stack": "cloud"})
    );
}
