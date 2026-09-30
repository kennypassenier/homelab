//! Milestone follow (feat-platform-10), the edit forms: Claude drives the
//! firewall with its rule dialog, a stack's settings and its raw editor,
//! the new-stack wizard, the host settings, the batch dialog and the roll
//! back dialog. Against a local bare repository (never GitHub) and the act
//! mock host: every step is checked against the same description the
//! browser draws from, and each final press (the commit and push, host.toml,
//! the batch) runs once through the route's own function, whatever number
//! of tabs follow.

mod act_support;

use std::path::{Path, PathBuf};
use std::process::Command as Proc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{shared, temp_dir, MemFiles, MockHost, RecPusher, Recorder, Script, TestClock};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use homelab_admin::core::actions_config::GitConfig;
use homelab_admin::core::drive::{Field, FieldKind, Values};
use homelab_admin::core::driveedit;
use homelab_admin::shell::actions::{Actions, ActionsDeps, CommitInfo};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::drive::{router, Driver};
use homelab_admin::shell::edit::EditCtx;
use homelab_admin::shell::workcopy::WorkingCopy;
use homelab_proto::{Command, Scope, UiStep};
use serde_json::{json, Value};
use tower::ServiceExt as _;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Proc::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "seed")
        .env("GIT_AUTHOR_EMAIL", "seed@example.invalid")
        .env("GIT_COMMITTER_NAME", "seed")
        .env("GIT_COMMITTER_EMAIL", "seed@example.invalid")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy(&p, &to.join(e.file_name()));
        } else if !e.file_name().to_string_lossy().contains(".env") {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

/// A bare repository holding a few real stacks and two presets.
fn seed(root: &Path) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let bare = root.join("origin.git");
    git(
        root,
        &[
            "init",
            "--quiet",
            "--bare",
            "-b",
            "main",
            bare.to_str().unwrap(),
        ],
    );
    let seed = root.join("seed");
    git(
        root,
        &[
            "clone",
            "--quiet",
            bare.to_str().unwrap(),
            seed.to_str().unwrap(),
        ],
    );
    git(&seed, &["checkout", "--quiet", "-b", "main"]);
    for s in ["admin", "kp-soft", "gateway"] {
        copy(&src.join("stacks").join(s), &seed.join("stacks").join(s));
    }
    for p in ["custom", "mealie"] {
        copy(&src.join("presets").join(p), &seed.join("presets").join(p));
    }
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "--quiet", "-m", "seed [meta]"]);
    git(&seed, &["push", "--quiet", "origin", "main"]);
    bare
}

struct World {
    root: PathBuf,
    bare: PathBuf,
    live: Arc<Recorder>,
    sent: Arc<Mutex<Vec<Command>>>,
    driver: Driver,
    actions: Actions,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn world(tag: &str) -> World {
    let root = temp_dir(&format!("follow-edit-{tag}"));
    let bare = seed(&root);
    let clock = TestClock::at(1_800_000_000);
    let sent: Arc<Mutex<Vec<Command>>> = Arc::default();
    let s2 = sent.clone();
    let host = MockHost::start(
        clock.clone(),
        Arc::new(move |c: &Command| {
            s2.lock().unwrap().push(c.clone());
            match c {
                Command::GetApplied { stack } if stack == "kp-soft" => Script {
                    message: json!([
                        {"path": "kp-soft/docker-compose.yml", "content": "old", "mode": null}
                    ])
                    .to_string(),
                    ..Script::ok(&[])
                },
                Command::GetApplied { .. } => Script {
                    message: "[]".into(),
                    ..Script::ok(&[])
                },
                Command::GetHostConfig => Script {
                    message: json!({
                        "path": "/etc/homelab/host.toml", "sha256": "b".repeat(64),
                        "values": {"backup_hour": 4, "listen": "0.0.0.0:8443"},
                        "secrets_set": ["token"], "unknown": []
                    })
                    .to_string(),
                    ..Script::ok(&[])
                },
                Command::SetHostConfig { .. } => Script {
                    message:
                        json!({"sha256": "c".repeat(64), "live": ["backup_hour"], "restart": []})
                            .to_string(),
                    ..Script::ok(&[])
                },
                _ => Script::ok(&[("run", 1)]),
            }
        }),
        json!({ "entries": [] }),
        Arc::default(),
    );
    let live = Arc::new(Recorder::default());
    let notify = NotifyCenter::load(
        root.join("notifications.json"),
        Arc::new(RecPusher::default()),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    let shared = shared(&[
        ("kp-soft", 116, None),
        ("admin", 120, None),
        ("gateway", 104, None),
    ]);
    shared.write().await.host_version = Some("3.63.0".into());
    let files = MemFiles {
        commits: vec![CommitInfo {
            commit: "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678".into(),
            at: 1_799_000_000,
            subject: "kp-soft 1.3 [feat-stacks-2]".into(),
        }],
        ..MemFiles::default()
    };
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: live.clone(),
        files: Arc::new(files),
        shared: shared.clone(),
        notify,
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    });
    let wc = Arc::new(WorkingCopy::new(
        root.join("data/repo"),
        GitConfig {
            remote: bare.display().to_string(),
            branch: "main".into(),
            key: root.join("data/deploy_key"),
            known_hosts: root.join("data/known_hosts"),
            author_name: "homelab-admin".into(),
            author_email: "homelab-admin@example.invalid".into(),
        },
        root.join("data/tmp"),
        Arc::new(|| 1_800_000_000),
    ));
    let edit = EditCtx {
        wc,
        actions: actions.clone(),
        host: host.clone(),
        shared: shared.clone(),
        publish: live.clone(),
    };
    let driver = Driver::with_edit(
        actions.clone(),
        shared,
        live.clone(),
        clock.clock(),
        Some(edit),
    );
    World {
        root,
        bare,
        live,
        sent,
        driver,
        actions,
    }
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
fn pick(f: &str, v: &str) -> UiStep {
    UiStep::Pick {
        field: f.into(),
        value: v.into(),
    }
}
fn row(op: &str, t: Option<&str>) -> UiStep {
    UiStep::Row {
        op: op.into(),
        target: t.map(str::to_string),
    }
}
fn checked(f: &str, on: bool) -> UiStep {
    UiStep::Check {
        field: f.into(),
        on,
    }
}

async fn step(w: &World, s: UiStep) -> Value {
    w.driver.step("wsl", Scope::Operate, s).await
}

async fn ok(w: &World, s: UiStep) -> Value {
    let v = step(w, s.clone()).await;
    assert_eq!(v["ok"], true, "{s:?}: {v}");
    v
}

fn refused(v: &Value) -> (String, String) {
    assert_eq!(v["ok"], false, "expected a refusal: {v}");
    let r = &v["refusal"];
    let why = r["why"].as_str().unwrap_or_default().to_string();
    let fix = r["fix"].as_str().unwrap_or_default().to_string();
    assert!(!why.is_empty() && !fix.is_empty(), "{v}");
    (why, fix)
}

fn commits(w: &World) -> usize {
    git(&w.bare, &["rev-list", "--count", "main"])
        .trim()
        .parse()
        .unwrap()
}

/// A tab catching up, as a tab that turns Live view on does.
async fn tab_reads(w: &World, tabs: usize) {
    let app = router(w.driver.clone());
    for _ in 0..tabs {
        let r = app
            .clone()
            .oneshot(Request::get("/data/drive").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
    }
}

/// feat-platform-10 (milestone follow), feat-firewall-1.
///
/// A firewall rule added through the rule dialog, then the plan, then the
/// commit: steps checked against the rule form's own words (a network with
/// host bits is held in the browser's words), and the commit and push run
/// once through the route's function, with zero, one or two tabs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_firewall_rule_add_plan_commit_runs_once_with_zero_one_or_two_tabs() {
    for tabs in 0..=2usize {
        let w = world(&format!("fw-{tabs}")).await;
        let opened = ok(&w, open("firewall", Some("admin"))).await;
        let f = &opened["state"]["form"];
        assert_eq!(opened["state"]["page"], "/app/stacks/admin/firewall");
        assert_eq!(f["step"], "rules");
        let n = f["edit"]["rows"].as_array().unwrap().len();
        assert!(n > 0, "{f}");
        // Nothing changed yet: the review button is not on screen.
        let (why, _) = refused(&step(&w, press("next")).await);
        assert!(why.contains("not on screen"), "{why}");
        // The rule dialog's fields are behind `row add`.
        let (why, _) = refused(&step(&w, typed("rule-peer", "10.10.10.4")).await);
        assert!(why.contains("no field rule-peer"), "{why}");
        ok(&w, row("add", None)).await;
        let (why, _) = refused(&step(&w, typed("fw-comment", "x")).await);
        assert!(why.contains("behind the open dialog"), "{why}");
        ok(&w, typed("rule-peer", "10.10.10.4/24")).await;
        ok(&w, typed("rule-dport", "9999")).await;
        let (why, _) = refused(&step(&w, pick("rule-proto", "sctp")).await);
        assert!(why.contains("not a choice"), "{why}");
        let held = step(&w, press("save")).await;
        let (why, _) = refused(&held);
        assert!(why.contains("rule-peer"), "{why}");
        assert_eq!(
            held["state"]["form"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["id"] == "rule-peer")
                .unwrap()["error"],
            "10.10.10.4/24 has host bits set: Proxmox reads it as the whole network. Write one address, or the network."
        );
        ok(&w, typed("rule-peer", "10.10.10.4")).await;
        ok(&w, typed("rule-note", "drive test")).await;
        let saved = ok(&w, press("save")).await;
        let rows = saved["state"]["form"]["edit"]["rows"].as_array().unwrap();
        assert_eq!(rows.len(), n + 1);
        assert!(
            rows[n]
                .as_str()
                .unwrap()
                .contains("IN ACCEPT from 10.10.10.4 tcp port 9999 (new)"),
            "{rows:?}"
        );
        // Moved up and back down: the table follows.
        ok(&w, row("up", Some(&(n + 1).to_string()))).await;
        let (why, _) = refused(&step(&w, row("down", Some("99"))).await);
        assert!(why.contains("no rule"), "{why}");
        ok(&w, row("down", Some(&n.to_string()))).await;
        let plan = ok(&w, press("next")).await;
        let p = &plan["state"]["form"];
        assert_eq!(p["step"], "plan");
        assert_eq!(p["edit"]["plan"]["valid"], true, "{p}");
        assert_eq!(
            p["edit"]["plan"]["files"][0]["path"],
            "stacks/admin/lxc-compose.yml"
        );
        tab_reads(&w, tabs).await;
        let at_commit = ok(&w, press("next")).await;
        assert_eq!(at_commit["state"]["form"]["step"], "commit");
        ok(&w, pick("edit-follow", "none")).await;
        ok(
            &w,
            typed("edit-subject", "admin: the drive test may reach 9999"),
        )
        .await;
        assert_eq!(commits(&w), 1, "nothing is written before the press");
        let done = ok(&w, press("confirm")).await;
        let r = &done["state"]["form"]["edit"]["result"];
        assert!(r["committed"]["commit"].is_string(), "{done}");
        tab_reads(&w, tabs).await;
        let (why, _) = refused(&step(&w, press("confirm")).await);
        assert!(why.contains("one press runs once"), "{why}");
        assert_eq!(commits(&w), 2, "{tabs} tab(s): exactly one commit");
        assert_eq!(
            git(&w.bare, &["log", "-1", "--format=%s", "main"]).trim(),
            "admin: the drive test may reach 9999 [feat-firewall-1]"
        );
        let file = git(&w.bare, &["show", "main:stacks/admin/lxc-compose.yml"]);
        assert!(file.contains("9999"), "{file}");
        assert!(!w.live.events("repo").is_empty());
        // Every step went to the tabs as one event with the whole state.
        assert!(w.live.events("drive").len() > 10);
    }
}

/// feat-platform-10 (milestone follow), feat-stacks-2.
///
/// The settings form checks a field in the form's words, sends only what
/// changed, and its commit's deploy is a job whose origin is Claude; the
/// raw editor holds a file that is as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_settings_commit_and_deploy_and_the_raw_editor() {
    let w = world("settings").await;
    let opened = ok(&w, open("settings", Some("kp-soft"))).await;
    assert_eq!(opened["state"]["page"], "/app/stacks/kp-soft/settings");
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("Nothing is changed yet."), "{why}");
    ok(&w, typed("edit-memory-mb", "64")).await;
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(
        why.contains("edit-memory-mb: Memory (MB) must be a whole number from 128 to 262144."),
        "{why}"
    );
    ok(&w, typed("edit-memory-mb", "3072")).await;
    ok(&w, typed("edit-order", "80")).await;
    let plan = ok(&w, press("next")).await;
    let p = &plan["state"]["form"]["edit"]["plan"];
    assert_eq!(p["valid"], true, "{p}");
    assert_eq!(p["follow_ups"], json!(["deploy", "resize"]));
    ok(&w, press("next")).await;
    let f = &step(&w, UiStep::State).await["state"]["form"];
    let follow = f["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == "edit-follow")
        .unwrap();
    assert_eq!(follow["value"], "deploy", "the plan's default follow-up");
    ok(&w, typed("edit-subject", "")).await;
    let (why, _) = refused(&step(&w, press("confirm")).await);
    assert!(why.contains("A commit needs a subject."), "{why}");
    ok(&w, typed("edit-subject", "kp-soft gets 3 GB")).await;
    let done = ok(&w, press("confirm")).await;
    let job = done["state"]["form"]["job"]["job"].as_u64().unwrap();
    let ev = w
        .live
        .events("action")
        .into_iter()
        .find(|e| e["job"] == job)
        .unwrap();
    assert_eq!(ev["action"], "deploy-commit");
    assert_eq!(ev["origin"], json!({"from": "claude", "by": "wsl"}));
    assert_eq!(
        ev["args"]["commit"],
        git(&w.bare, &["rev-parse", "main"]).trim()
    );
    assert_eq!(commits(&w), 2);
    ok(&w, UiStep::Close).await;

    let raw = ok(&w, open("raw", Some("kp-soft"))).await;
    let text = raw["state"]["form"]["values"]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("stack_name: kp-soft"), "{text}");
    ok(
        &w,
        UiStep::Edit {
            field: "raw-text".into(),
            text: text.clone(),
        },
    )
    .await;
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("The file is as it was."), "{why}");
    let (why, fix) = refused(&step(&w, typed("raw-file", "x")).await);
    assert!(
        why.contains("choice field") && fix.contains("pick"),
        "{why} {fix}"
    );
    assert_eq!(commits(&w), 2);
}

/// feat-platform-10 (milestone follow), feat-stacks-9.
///
/// The settings-extension form (network, lxc flags, storage, on_demand,
/// retention): a field changed, the plan, the commit, once — the same
/// shape `follow_settings_commit_and_deploy_and_the_raw_editor` proves for
/// the first settings page.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_settings_ext_commits_once() {
    let w = world("settings-ext").await;
    let opened = ok(&w, open("settings-ext", Some("kp-soft"))).await;
    assert_eq!(opened["state"]["page"], "/app/stacks/kp-soft/settings");
    assert_eq!(opened["state"]["form"]["step"], "settings_ext");
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("Nothing is changed yet."), "{why}");
    ok(&w, typed("edit-network-vlan", "20")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(plan["state"]["form"]["edit"]["plan"]["valid"], true);
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "kp-soft moves to vlan 20")).await;
    let done = ok(&w, press("confirm")).await;
    assert!(
        done["state"]["form"]["edit"]["result"]["committed"]["commit"].is_string(),
        "{done}"
    );
    assert_eq!(commits(&w), 2);
    let file = git(&w.bare, &["show", "main:stacks/kp-soft/lxc-compose.yml"]);
    assert!(file.contains("vlan: 20"), "{file}");
}

/// feat-platform-10 (milestone follow), feat-stacks-10.
///
/// Apps & storage: the storage row dialog — the same "row add/edit" shape
/// the firewall rule dialog uses, generalised (`Sub.kind` is the list's
/// own name here, `"storage"`, rather than `"rule"`) — add, edit and
/// delete a row (deleted again before the commit: storage naming is
/// D25-owner-shaped and both of kp-soft's apps already have their
/// directory, so a synthetic third would fail the staged manifest's own
/// check — the deep rule this milestone leaves to `check_dir`, same as
/// every other edit); a blank app committed once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_apps_row_dialog_and_a_blank_app_commit_once() {
    let w = world("apps").await;
    let opened = ok(&w, open("apps", Some("kp-soft"))).await;
    assert_eq!(opened["state"]["form"]["step"], "apps");
    let n = opened["state"]["form"]["edit"]["rows"]
        .as_array()
        .unwrap()
        .len();
    // The row dialog's fields are behind `row add storage`.
    let (why, _) = refused(&step(&w, typed("storage-host-path", "/appdata/kp-soft/x")).await);
    assert!(why.contains("no field storage-host-path"), "{why}");
    ok(&w, row("add", Some("storage"))).await;
    // A relative path is refused in the row dialog's own words, before
    // anything is sent.
    let held = step(&w, press("save")).await;
    let (why, _) = refused(&held);
    assert!(
        why.contains("storage-host-path") && why.contains("storage-mount-point"),
        "{why}"
    );
    ok(
        &w,
        typed("storage-host-path", "/appdata/kp-soft/drive-test-config"),
    )
    .await;
    ok(
        &w,
        typed("storage-mount-point", "/appdata/kp-soft/drive-test-config"),
    )
    .await;
    let saved = ok(&w, press("save")).await;
    let rows = saved["state"]["form"]["edit"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), n + 1, "{rows:?}");
    assert!(
        rows.iter()
            .any(|r| r.as_str().unwrap_or("").contains("drive-test-config (new)")),
        "{rows:?}"
    );
    // Edited, then deleted again: the dialog re-opens with the row's
    // current values.
    ok(&w, row("edit", Some(&format!("storage:{}", n + 1)))).await;
    let f = &step(&w, UiStep::State).await["state"]["form"];
    let host_path = f["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == "storage-host-path")
        .unwrap()["value"]
        .clone();
    assert_eq!(host_path, json!("/appdata/kp-soft/drive-test-config"));
    ok(&w, press("save")).await;
    ok(&w, row("delete", Some(&format!("storage:{}", n + 1)))).await;
    let back = &step(&w, UiStep::State).await["state"]["form"]["edit"]["rows"];
    assert_eq!(back.as_array().unwrap().len(), n);
    ok(&w, typed("apps-add-blank", "drive-test-app")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(
        plan["state"]["form"]["edit"]["plan"]["valid"], true,
        "{plan}"
    );
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "kp-soft gets a blank app")).await;
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), 2);
    let file = git(&w.bare, &["show", "main:stacks/kp-soft/lxc-compose.yml"]);
    assert!(file.contains("drive-test-app"), "{file}");
    assert!(!file.contains("drive-test-config"), "{file}");
    let compose = git(
        &w.bare,
        &[
            "show",
            "main:stacks/kp-soft/drive-test-app/docker-compose.yml",
        ],
    );
    assert!(compose.contains("drive-test-app"), "{compose}");
}

/// feat-platform-10 (milestone follow), feat-stacks-11.
///
/// Latch: a secret app ticked and a latch_files row added — refused first
/// for a `${` placeholder (the known `latch --expand` trap), same as the
/// browser, then accepted once fixed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_latch_secret_and_a_file_row_refuse_dollar_then_commit_once() {
    let w = world("latch").await;
    let opened = ok(&w, open("latch", Some("kp-soft"))).await;
    assert_eq!(opened["state"]["form"]["step"], "latch");
    ok(&w, checked("latch-secret-jobtracker", true)).await;
    ok(&w, row("add", Some("latch_files"))).await;
    ok(&w, typed("latchfile-from", "kp-soft/unit.env")).await;
    ok(&w, typed("latchfile-dest", "/var/www/${TOKEN}")).await;
    ok(&w, typed("latchfile-mode", "640")).await;
    let held = step(&w, press("save")).await;
    let (why, _) = refused(&held);
    assert!(why.contains("latchfile-dest"), "{why}");
    ok(&w, typed("latchfile-dest", "/var/www/unit.env")).await;
    ok(&w, press("save")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(plan["state"]["form"]["edit"]["plan"]["valid"], true);
    ok(&w, press("next")).await;
    ok(
        &w,
        typed("edit-subject", "kp-soft: jobtracker latch secrets"),
    )
    .await;
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), 2);
    let file = git(&w.bare, &["show", "main:stacks/kp-soft/lxc-compose.yml"]);
    assert!(file.contains("jobtracker"), "{file}");
    assert!(file.contains("unit.env"), "{file}");
    assert!(!file.contains("${TOKEN}"), "{file}");
}

/// feat-platform-10 (milestone follow), feat-checks-1.
///
/// Checks: a check row added through its own dialog (refused first for a
/// missing blind spot below Application layer, same as the browser), the
/// busy check set, then plan and commit once — `checks.yml` is its own
/// tab, so this opens `checks-edit:<stack>`, not `stack-edit:<stack>`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_checks_row_add_and_busy_check_commit_once() {
    let w = world("checks").await;
    let opened = ok(&w, open("checks", Some("kp-soft/kp-soft"))).await;
    assert_eq!(opened["state"]["page"], "/app/stacks/kp-soft/checks");
    assert_eq!(opened["state"]["form"]["step"], "checks");
    let n = opened["state"]["form"]["edit"]["rows"]
        .as_array()
        .unwrap()
        .len();
    ok(&w, row("add", Some("checks"))).await;
    ok(&w, typed("check-name", "drive test")).await;
    ok(&w, typed("check-command", "echo 1")).await;
    ok(&w, pick("check-expect", "must_match")).await;
    ok(&w, pick("check-layer", "network")).await;
    let held = step(&w, press("save")).await;
    let (why, _) = refused(&held);
    assert!(why.contains("blind spot"), "{why}");
    ok(
        &w,
        typed("check-blind-spot", "does not prove the app itself is up"),
    )
    .await;
    let saved = ok(&w, press("save")).await;
    let rows = saved["state"]["form"]["edit"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), n + 1, "{rows:?}");
    assert!(
        rows.iter()
            .any(|r| r.as_str().unwrap_or("").contains("drive test (network)")),
        "{rows:?}"
    );
    ok(&w, typed("checks-busy", "echo busy")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(
        plan["state"]["form"]["edit"]["plan"]["valid"], true,
        "{plan}"
    );
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "kp-soft: a drive-tested check")).await;
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), 2);
    let file = git(&w.bare, &["show", "main:stacks/kp-soft/kp-soft/checks.yml"]);
    assert!(file.contains("drive test"), "{file}");
    assert!(file.contains("echo busy"), "{file}");
}

/// feat-platform-10 (milestone follow), feat-tiles-1.
///
/// Tiles: a tile edited (its own row dialog, `Sub.kind = "tiles"`) then a
/// second one deleted — deleting an EXISTING tile needs the sparse
/// `delete: true` tombstone `tilesBody`/`tiles_drive_body` build, not mere
/// omission (unlike storage/checks' full-list `Seq`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_tiles_edit_and_delete_commit_once() {
    let w = world("tiles").await;
    let opened = ok(&w, open("tiles", Some("gateway"))).await;
    assert_eq!(opened["state"]["page"], "/app/stacks/gateway/settings");
    let n = opened["state"]["form"]["edit"]["rows"]
        .as_array()
        .unwrap()
        .len();
    assert!(n >= 2, "{opened}");
    ok(&w, row("edit", Some("tiles:1"))).await;
    let f = &step(&w, UiStep::State).await["state"]["form"];
    let name = f["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == "tile-name")
        .unwrap()["value"]
        .clone();
    assert_eq!(name, json!("Home Assistant"));
    ok(&w, typed("tile-description", "drive test")).await;
    ok(&w, press("save")).await;
    ok(&w, row("delete", Some("tiles:2"))).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(
        plan["state"]["form"]["edit"]["plan"]["valid"], true,
        "{plan}"
    );
    ok(&w, press("next")).await;
    ok(
        &w,
        typed("edit-subject", "gateway: a drive-tested tile edit"),
    )
    .await;
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), 2);
    let file = git(&w.bare, &["show", "main:stacks/gateway/lxc-compose.yml"]);
    assert!(file.contains("drive test"), "{file}");
    assert!(!file.contains("opn.kp-soft.dev"), "{file}");
}

/// feat-platform-10 (milestone follow), feat-publish-1.
///
/// `homelab ui open publish <stack>/<app>` opens the Apps tab's dialog
/// directly (it is a click-opened dialog, not a page already showing it —
/// `editdrive.js::publishApp`), then plan and commit once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_publish_app_commits_once() {
    let w = world("publish").await;
    let opened = ok(&w, open("publish", Some("kp-soft/jobtracker"))).await;
    assert_eq!(opened["state"]["page"], "/app/stacks/kp-soft/apps");
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("is needed"), "{why}");
    ok(&w, typed("publish-hostname", "jobtracker.kp-soft.dev")).await;
    ok(&w, typed("publish-port", "8080")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(
        plan["state"]["form"]["edit"]["plan"]["valid"], true,
        "{plan}"
    );
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "kp-soft: jobtracker publishes")).await;
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), 2);
    let file = git(&w.bare, &["show", "main:stacks/kp-soft/traefik-routes.yml"]);
    assert!(file.contains("jobtracker.kp-soft.dev"), "{file}");
}

/// feat-platform-10 (milestone follow), feat-native-1.
///
/// The native step's second button, `remove` (`#native-remove`, beside
/// `next`) — refused here because `admin` is `native_only` with this its
/// one unit (removing it would leave no app and no native, which
/// `validate_manifest` refuses); the refusal proves the button reaches the
/// real `remove_native` edit and the real validator, not a stub.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_native_remove_reaches_the_real_edit_and_validator() {
    let w = world("native-remove").await;
    ok(&w, open("native", Some("admin"))).await;
    let plan = ok(&w, press("remove")).await;
    let p = &plan["state"]["form"]["edit"]["plan"];
    assert_eq!(p["valid"], false, "{p}");
    assert!(
        p["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x.as_str().unwrap_or("").contains("native")),
        "{p}"
    );
    assert_eq!(commits(&w), 1, "a refused plan writes nothing");
}

/// feat-platform-10 (milestone follow), feat-stacks-files.
///
/// `homelab ui open raw <stack>` does what the Files card's own New file /
/// Rename / Delete buttons do: the `op` field (`files-op`, picked, not
/// typed — it is a choice field like `raw-file`) switches what the rest of
/// the form means, and the commit still runs once per op, through the
/// very `StackEdit::Files` the click path builds (stackedit_files.rs).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_the_raw_editor_creates_renames_and_deletes_a_file() {
    let w = world("files").await;

    // Create: op picked, a new path typed, the starting text edited in.
    ok(&w, open("raw", Some("kp-soft"))).await;
    ok(&w, pick("files-op", "create")).await;
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("New path is needed."), "{why}");
    ok(&w, typed("files-new-path", "kp-soft/new-note.yml")).await;
    ok(
        &w,
        UiStep::Edit {
            field: "raw-text".into(),
            text: "hello: world\n".into(),
        },
    )
    .await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(
        plan["state"]["form"]["edit"]["plan"]["valid"], true,
        "{plan}"
    );
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "a new note")).await;
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), 2);
    assert_eq!(
        git(&w.bare, &["show", "main:stacks/kp-soft/new-note.yml"]),
        "hello: world\n"
    );
    ok(&w, UiStep::Close).await;

    // Rename: op picked, the existing file picked (from the choices the
    // reopened form reads fresh, so the file just created is on the list),
    // the new path typed.
    ok(&w, open("raw", Some("kp-soft"))).await;
    ok(&w, pick("files-op", "rename")).await;
    ok(&w, pick("raw-file", "kp-soft/new-note.yml")).await;
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("Rename to is needed."), "{why}");
    ok(&w, typed("files-rename-to", "kp-soft/renamed-note.yml")).await;
    ok(&w, press("next")).await;
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "rename the note")).await;
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), 3);
    assert_eq!(
        git(&w.bare, &["show", "main:stacks/kp-soft/renamed-note.yml"]),
        "hello: world\n"
    );
    ok(&w, UiStep::Close).await;

    // Delete: op and file picked, nothing else to fill in.
    ok(&w, open("raw", Some("kp-soft"))).await;
    ok(&w, pick("files-op", "delete")).await;
    ok(&w, pick("raw-file", "kp-soft/renamed-note.yml")).await;
    ok(&w, press("next")).await;
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "drop the note")).await;
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), 4);
    let ls = git(&w.bare, &["ls-tree", "-r", "--name-only", "main"]);
    assert!(!ls.contains("renamed-note.yml"), "{ls}");

    // The stack's own manifest may not be deleted or renamed this way.
    ok(&w, open("raw", Some("kp-soft"))).await;
    ok(&w, pick("files-op", "delete")).await;
    ok(&w, pick("raw-file", "lxc-compose.yml")).await;
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("manifest"), "{why}");
    assert_eq!(commits(&w), 4);
}

/// feat-platform-10 (milestone follow), feat-stacks-3.
///
/// The new-stack wizard step by step: the preset's size follows the pick,
/// a taken name is held in the wizard's words, the data step reads the
/// preset's folders, and the commit writes only the new directory, once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_the_new_stack_wizard_commits_once() {
    let w = world("new").await;
    let (why, _) = refused(&step(&w, open("new-stack", Some("kp-soft"))).await);
    assert!(why.contains("takes no stack"), "{why}");
    let opened = ok(&w, open("new-stack", None)).await;
    assert_eq!(opened["state"]["form"]["step"], "preset");
    ok(&w, pick("new-preset", "custom")).await;
    ok(&w, pick("new-preset", "mealie")).await;
    ok(&w, press("next")).await;
    ok(&w, typed("new-name", "kp-soft")).await;
    ok(&w, typed("new-vmid", "121")).await;
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(
        why.contains("There is a stack called kp-soft already."),
        "{why}"
    );
    ok(&w, typed("new-name", "recipes")).await;
    ok(&w, press("next")).await;
    ok(&w, typed("new-ram-mb", "512")).await;
    let data = ok(&w, press("next")).await;
    let f = &data["state"]["form"];
    assert_eq!(f["step"], "data");
    assert!(
        f["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["id"] == "new-nodata-0"),
        "{f}"
    );
    let tile_step = ok(&w, press("next")).await;
    assert_eq!(tile_step["state"]["form"]["step"], "tile");
    // Left blank: feat-tiles-3's "no tile" answer.
    let plan = ok(&w, press("next")).await;
    let p = &plan["state"]["form"];
    assert_eq!(p["edit"]["plan"]["valid"], true, "{p}");
    assert_eq!(p["buttons"], json!(["back", "confirm", "close"]));
    ok(&w, pick("edit-follow", "none")).await;
    ok(&w, press("confirm")).await;
    let (why, _) = refused(&step(&w, press("confirm")).await);
    assert!(why.contains("one press runs once"), "{why}");
    assert_eq!(commits(&w), 2);
    let files = git(&w.bare, &["show", "--name-only", "--format=", "main"]);
    assert!(
        files.lines().all(|l| l.starts_with("stacks/recipes/")),
        "{files}"
    );
}

/// feat-platform-10 (milestone follow), feat-tiles-3.
///
/// The new-stack wizard's Tile step folds into the SAME commit as the
/// stack itself — `newstack.rs::NewStack.tile`, applied to the staged
/// manifest before it is ever written (`edit::apply_new_stack_tile`) —
/// rather than a second, best-effort `StackEdit::Tiles` commit once the
/// stack already exists: exactly one commit, and the tile is in the very
/// file that commit writes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_new_stack_tile_folds_into_the_one_commit() {
    let w = world("new-tile").await;
    ok(&w, open("new-stack", None)).await;
    ok(&w, pick("new-preset", "mealie")).await;
    ok(&w, press("next")).await;
    ok(&w, typed("new-name", "recipes")).await;
    ok(&w, typed("new-vmid", "121")).await;
    ok(&w, press("next")).await;
    ok(&w, press("next")).await;
    ok(&w, press("next")).await;
    let tile_step = &step(&w, UiStep::State).await["state"]["form"];
    assert_eq!(tile_step["step"], "tile");
    ok(&w, typed("new-tile-hostname", "recipes.kp-soft.dev")).await;
    ok(&w, typed("new-tile-group", "Household")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(
        plan["state"]["form"]["edit"]["plan"]["valid"], true,
        "{plan}"
    );
    ok(&w, press("next")).await;
    ok(
        &w,
        typed("edit-subject", "recipes: a new stack with a tile"),
    )
    .await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(
        commits(&w),
        before + 1,
        "exactly one commit, not a second one for the tile"
    );
    let file = git(&w.bare, &["show", "main:stacks/recipes/lxc-compose.yml"]);
    assert!(file.contains("recipes.kp-soft.dev"), "{file}");
    assert!(file.contains("Household"), "{file}");
}

/// feat-platform-10 (milestone follow), feat-tiles-3.
///
/// Add-app's own per-app tile hostname (`add-app-tile-<app>`) folds into
/// the SAME commit as the app it names — `StackEdit::AddApp.tiles`,
/// applied to the staged manifest alongside `apps:`/`storage:` — rather
/// than a second, best-effort `StackEdit::Tiles` commit after the app
/// already exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_add_app_tile_folds_into_the_one_commit() {
    let w = world("add-app-tile").await;
    ok(&w, open("add-app", Some("kp-soft"))).await;
    // `(EditKind::AddApp, "preset")`'s after_set rebuilds the step's
    // fields for whichever preset is picked, the same as the browser's
    // own `renderTiles` on a preset change.
    let picked = ok(&w, pick("add-app-preset", "mealie")).await;
    let fields = picked["state"]["form"]["fields"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        fields.iter().any(|f| f["id"] == "add-app-tile-mealie"),
        "{fields:?}"
    );
    ok(&w, typed("add-app-tile-mealie", "mealie.kp-soft.dev")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(
        plan["state"]["form"]["edit"]["plan"]["valid"], true,
        "{plan}"
    );
    ok(&w, press("next")).await;
    ok(
        &w,
        typed("edit-subject", "kp-soft: mealie joins, with a tile"),
    )
    .await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(
        commits(&w),
        before + 1,
        "exactly one commit, not a second one for the tile"
    );
    let file = git(&w.bare, &["show", "main:stacks/kp-soft/lxc-compose.yml"]);
    assert!(file.contains("mealie.kp-soft.dev"), "{file}");
    assert!(file.contains("mealie"), "{file}");
}

/// feat-platform-10 (milestone follow), feat-preset-1.
///
/// The presets editor's Files card: a file created, then renamed, then
/// deleted, then the whole preset removed — each its own small
/// plan/commit on the "meta" step's extra buttons
/// (`save-file`/`rename-file`/`delete-file`/`remove-preset`, beside
/// `next`), driven the same way the row dialogs and native-remove are.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_preset_files_and_removal_each_commit_once() {
    let w = world("preset-files").await;
    let opened = ok(&w, open("preset", Some("mealie"))).await;
    assert_eq!(opened["state"]["form"]["step"], "meta");
    assert_eq!(
        opened["state"]["form"]["buttons"],
        json!([
            "next",
            "save-file",
            "rename-file",
            "delete-file",
            "remove-preset"
        ])
    );
    // Create a file.
    ok(&w, typed("preset-file-path", "mealie/README.md")).await;
    ok(&w, typed("preset-file-text", "drive test")).await;
    let plan = ok(&w, press("save-file")).await;
    assert_eq!(
        plan["state"]["form"]["edit"]["plan"]["valid"], true,
        "{plan}"
    );
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "mealie: a drive-tested file")).await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), before + 1);
    let file = git(&w.bare, &["show", "main:presets/mealie/README.md"]);
    assert_eq!(file, "drive test");
    ok(&w, UiStep::Close).await;

    // Rename it.
    ok(&w, open("preset", Some("mealie"))).await;
    ok(&w, pick("preset-file-select", "mealie/README.md")).await;
    let f = &step(&w, UiStep::State).await["state"]["form"];
    assert_eq!(
        f["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == "preset-file-path")
            .unwrap()["value"],
        json!("mealie/README.md"),
        "{f}"
    );
    ok(&w, typed("preset-file-rename-to", "mealie/NOTES.md")).await;
    ok(&w, press("rename-file")).await;
    ok(&w, press("next")).await;
    ok(
        &w,
        typed("edit-subject", "mealie: rename the drive-tested file"),
    )
    .await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), before + 1);
    assert!(git(
        &w.bare,
        &["ls-tree", "-r", "--name-only", "main", "presets/mealie/"]
    )
    .contains("NOTES.md"));
    ok(&w, UiStep::Close).await;

    // Delete it.
    ok(&w, open("preset", Some("mealie"))).await;
    ok(&w, pick("preset-file-select", "mealie/NOTES.md")).await;
    ok(&w, press("delete-file")).await;
    ok(&w, press("next")).await;
    ok(
        &w,
        typed("edit-subject", "mealie: remove the drive-tested file"),
    )
    .await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), before + 1);
    assert!(!git(
        &w.bare,
        &["ls-tree", "-r", "--name-only", "main", "presets/mealie/"]
    )
    .contains("NOTES.md"));
    ok(&w, UiStep::Close).await;

    // Remove the whole preset.
    ok(&w, open("preset", Some("mealie"))).await;
    ok(&w, press("remove-preset")).await;
    let removed_plan = ok(&w, press("next")).await;
    assert_eq!(removed_plan["state"]["form"]["edit"]["plan"]["valid"], true);
    ok(&w, typed("edit-subject", "remove the mealie preset")).await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), before + 1);
    assert!(git(&w.bare, &["ls-tree", "main", "presets/"])
        .lines()
        .all(|l| !l.contains("mealie")));
}

/// feat-platform-10 (milestone follow), feat-settings-1.
///
/// host.toml needs a token of scope all; a locked key is refused at the
/// row; a key is kept in its dialog, then written once with the session
/// command the page uses.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_host_settings_write_once_within_scope() {
    let w = world("host").await;
    let (why, _) = refused(&step(&w, open("host-settings", None)).await);
    assert!(why.contains("needs scope All"), "{why}");
    let all = |s| w.driver.step("wsl-all", Scope::All, s);
    let opened = all(open("host-settings", None)).await;
    assert_eq!(opened["ok"], true, "{opened}");
    assert_eq!(opened["state"]["page"], "/app/settings");
    let (why, _) = refused(&all(row("edit", Some("listen"))).await);
    assert!(why.contains("ssh only"), "{why}");
    assert_eq!(all(row("edit", Some("backup_hour"))).await["ok"], true);
    assert_eq!(all(typed("key-backup-hour", "25")).await["ok"], true);
    let (why, _) = refused(&all(press("save")).await);
    assert!(why.contains("A whole number from"), "{why}");
    assert_eq!(all(typed("key-backup-hour", "5")).await["ok"], true);
    assert_eq!(all(press("save")).await["ok"], true);
    assert_eq!(all(press("next")).await["ok"], true);
    let done = all(press("confirm")).await;
    assert_eq!(done["ok"], true, "{done}");
    assert_eq!(
        done["state"]["form"]["edit"]["result"]["saved"]["live"][0],
        "backup_hour"
    );
    refused(&all(press("confirm")).await);
    let sent = w.sent.lock().unwrap().clone();
    let writes: Vec<&Command> = sent
        .iter()
        .filter(|c| c.name() == "set_host_config")
        .collect();
    assert_eq!(writes.len(), 1, "{sent:?}");
    match writes[0] {
        Command::SetHostConfig { changes, .. } => assert_eq!(changes["backup_hour"], 5),
        other => panic!("{other:?}"),
    }
    assert_eq!(w.live.events("host_settings").len(), 1);
}

/// feat-platform-10 (milestone follow), feat-stacks-5, feat-stacks-6.
///
/// The batch dialog runs one batch through the route's function; the roll
/// back dialog hands over to the deploy-commit form with the commit picked,
/// as its row's button does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_the_batch_and_the_roll_back_dialogs() {
    let w = world("batch").await;
    let (why, _) = refused(&step(&w, open("batch:deploy-commit", Some("kp-soft"))).await);
    assert!(why.contains("several stacks"), "{why}");
    let (why, _) = refused(&step(&w, open("batch:backup", Some("kp-soft,ghost"))).await);
    assert!(why.contains("no stack ghost"), "{why}");
    let opened = ok(&w, open("batch:backup", Some("kp-soft,gateway"))).await;
    assert_eq!(opened["state"]["form"]["title"], "Back up · 2 stacks");
    let done = ok(&w, press("confirm")).await;
    let batch = done["state"]["form"]["edit"]["result"]["batch"]
        .as_u64()
        .unwrap();
    refused(&step(&w, press("confirm")).await);
    let batches = w.live.events("action_batch");
    assert!(batches.iter().all(|b| b["batch"] == batch), "{batches:?}");
    let jobs = w
        .live
        .events("action")
        .into_iter()
        .filter(|e| e["origin"]["batch"] == batch)
        .map(|e| e["job"].as_u64().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(jobs.len(), 2, "one job per stack, one batch");
    ok(&w, UiStep::Close).await;

    let r = ok(&w, open("rollback", Some("kp-soft"))).await;
    assert_eq!(r["state"]["page"], "/app/stacks/kp-soft");
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("Choose a commit"), "{why}");
    ok(
        &w,
        pick(
            "rollback-commit",
            "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678",
        ),
    )
    .await;
    let handed = ok(&w, press("next")).await;
    let f = &handed["state"]["form"];
    assert_eq!(f["action"], "deploy-commit", "{f}");
    assert_eq!(
        f["values"]["commit"],
        "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678"
    );
    let _ = &w.actions;
}

fn select(stacks: &[&str]) -> UiStep {
    UiStep::Select {
        stacks: stacks.iter().map(|s| s.to_string()).collect(),
    }
}

/// Owner decision 2026-09-30: `homelab ui select` ticks the Overview
/// table's own multiselect, and `open batch <action>` with no stacks named
/// opens the batch dialog from that selection — the CLI driving the same
/// bulk action a click would.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_select_ticks_the_fleet_table_for_a_batch_opened_from_it() {
    let w = world("batch").await;
    // The selection lives on the Overview page only.
    ok(&w, open("deploy", Some("kp-soft"))).await;
    ok(&w, UiStep::Close).await;
    let (why, _) = refused(&step(&w, select(&["kp-soft"])).await);
    assert!(why.contains("Overview"), "{why}");
    ok(
        &w,
        UiStep::Goto {
            path: "/app/".into(),
        },
    )
    .await;
    let (why, _) = refused(&step(&w, select(&["nope"])).await);
    assert!(why.contains("no stack nope"), "{why}");
    // No selection: the batch dialog refuses, naming both fixes.
    let (why, fix) = refused(&step(&w, open("batch:backup", None)).await);
    assert!(why.contains("selection is empty"), "{why}");
    assert!(fix.contains("homelab ui select"), "{fix}");
    let ticked = ok(&w, select(&["kp-soft", "gateway"])).await;
    assert_eq!(
        ticked["state"]["selected"],
        json!(["gateway", "kp-soft"]),
        "sorted and deduplicated"
    );
    let opened = ok(&w, open("batch:backup", None)).await;
    assert_eq!(opened["state"]["form"]["title"], "Back up · 2 stacks");
    assert_eq!(opened["state"]["form"]["stack"], "gateway,kp-soft");
    ok(&w, UiStep::Close).await;
    // `select none` clears it.
    let cleared = ok(&w, select(&[])).await;
    assert_eq!(cleared["state"]["selected"], json!([]));
}

/// feat-platform-10 (milestone follow).
///
/// The edit forms' checks are the browser's: the same values give the same
/// words on both sides (the node suite runs the same cases file).
#[test]
fn follow_the_edit_checks_match_the_browser_cases() {
    let raw = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("web/test/formspec-cases.json"),
    )
    .unwrap();
    let cases: Value = serde_json::from_str(&raw).unwrap();
    for c in cases["edit_cases"].as_array().unwrap() {
        let values: Values = serde_json::from_value(c["values"].clone()).unwrap();
        let want = &c["errors"];
        let got = match c["check"].as_str().unwrap() {
            "settings" => {
                let fields = driveedit::settings_fields(&c["manifest"], &json!({}));
                json!(driveedit::check_fields(&fields, &values))
            }
            "rule" => {
                let fields = driveedit::rule_fields(None);
                let mut e = driveedit::check_fields(&fields, &values);
                e.extend(driveedit::rule_problems(&values));
                json!(e)
            }
            "tile" => json!(driveedit::tile_problems(&values)),
            "settings_ext" => {
                let fields = driveedit::settings_ext_fields(&c["manifest"]);
                let mut e = driveedit::check_fields(&fields, &values);
                e.extend(driveedit::settings_ext_problems(&values));
                json!(e)
            }
            "latch_file" => {
                let natives: Vec<String> = c["natives"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
                let fields = driveedit::latch_file_fields(&natives, None);
                let mut e = driveedit::check_fields(&fields, &values);
                e.extend(driveedit::latch_file_problems(&values));
                json!(e)
            }
            "key" => match driveedit::parse_key(&c["kind"], &c["values"]["value"]) {
                Ok(v) => json!({ "value": v }),
                Err(why) => json!({ "why": why }),
            },
            other => panic!("unknown check {other}"),
        };
        assert_eq!(&got, want, "{c}");
    }
    // Every edit field's kind reads, and the ids the browser draws are
    // the ids a step names.
    let f: Vec<Field> = driveedit::rule_fields(None);
    assert!(f
        .iter()
        .any(|x| x.id == "rule-peer" && x.kind == FieldKind::Text));
    let body = driveedit::rule_from_values(
        &serde_json::from_value(json!({
            "dir": "in", "action": "ACCEPT", "peer": " 10.10.10.7 ", "proto": "icmp",
            "dport": "", "note": "", "comment": ""
        }))
        .unwrap(),
    );
    assert_eq!(
        body,
        json!({ "dir": "in", "action": "ACCEPT", "source": "10.10.10.7", "proto": "icmp" })
    );
}

/// feat-platform-10 (milestone follow).
///
/// Every browser module is served: the dashboard embeds its files one by
/// one, and a module left out breaks every page that imports it (a new
/// file of this milestone once was).
#[test]
fn follow_every_browser_module_is_embedded() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let main: String = std::fs::read_to_string(root.join("src/main.rs"))
        .unwrap()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let mut missing = Vec::new();
    for dir in ["js", "js/pages"] {
        for e in std::fs::read_dir(root.join("web").join(dir))
            .unwrap()
            .flatten()
        {
            let name = e.file_name().to_string_lossy().to_string();
            if !(name.ends_with(".js") || name.ends_with(".json")) {
                continue;
            }
            let path = format!("{dir}/{name}");
            if !main.contains(&format!("(\"{path}\",include_bytes!(\"../web/{path}\")")) {
                missing.push(path);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "not embedded in src/main.rs: {missing:?}"
    );
}

/// TUI parity (`homelab import`): the import form, driven. The bundle is
/// one multi-line field (`homelab ui edit import-bundle <file>`), the name
/// and the number are held in the new-stack wizard's words, then the plan
/// and the commit, once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parity_the_import_form_commits_once() {
    let w = world("import").await;
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks/uptime");
    let (bundle, _) = homelab_client::spec::bundle_text(&src).unwrap();
    let opened = ok(&w, open("import", None)).await;
    assert_eq!(opened["state"]["form"]["step"], "bundle");
    ok(
        &w,
        UiStep::Edit {
            field: "import-bundle".into(),
            text: bundle,
        },
    )
    .await;
    ok(&w, typed("import-name", "kp-soft")).await;
    ok(&w, typed("import-vmid", "197")).await;
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(
        why.contains("There is a stack called kp-soft already."),
        "{why}"
    );
    ok(&w, typed("import-name", "uptime2")).await;
    let plan = ok(&w, press("next")).await;
    let p = &plan["state"]["form"];
    assert_eq!(p["step"], "plan");
    assert_eq!(p["edit"]["plan"]["valid"], true, "{p}");
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "uptime2 from a bundle")).await;
    ok(&w, pick("edit-follow", "none")).await;
    ok(&w, press("confirm")).await;
    let (why, _) = refused(&step(&w, press("confirm")).await);
    assert!(why.contains("one press runs once"), "{why}");
    assert_eq!(commits(&w), 2);
    let files = git(&w.bare, &["show", "--name-only", "--format=", "main"]);
    assert!(
        files.lines().all(|l| l.starts_with("stacks/uptime2/")),
        "{files}"
    );
}

/// feat-platform-10 (milestone follow), feat-native-1.
///
/// `homelab ui open native <stack>[/<unit>]` edits one native unit's
/// `service.yml` — admin's own (`stacks/admin/service.yml`, its one native
/// unit, at the stack's root) — through the generic stack-edit plan and
/// commit every other stack-edit form uses (`EditKind::stack_edit`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_native_edit_commits_once() {
    let w = world("native").await;
    let opened = ok(&w, open("native", Some("admin"))).await;
    assert_eq!(opened["state"]["page"], "/app/stacks/admin/settings");
    let fields = opened["state"]["form"]["fields"]
        .as_array()
        .unwrap()
        .clone();
    let value_of = |id: &str| {
        fields
            .iter()
            .find(|x| x["id"] == id)
            .map(|x| x["value"].clone())
            .unwrap_or_default()
    };
    assert_eq!(value_of("native-unit"), "admin");
    assert_eq!(
        value_of("native-binary"),
        "/opt/homelab-admin/bin/homelab-admin"
    );
    // admin/service.yml already names a release_repo, so switching from
    // `manual` to `auto` is a valid change (`validate_native` refuses
    // `auto` without one).
    ok(&w, pick("native-update-policy", "auto")).await;
    let plan = ok(&w, press("next")).await;
    let p = &plan["state"]["form"]["edit"]["plan"];
    assert_eq!(p["valid"], true, "{p}");
    ok(&w, press("next")).await;
    ok(
        &w,
        typed("edit-subject", "admin's updates run through the homelab"),
    )
    .await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), before + 1);
    let head = git(&w.bare, &["show", "main:stacks/admin/service.yml"]);
    assert!(head.contains("update_policy: auto"), "{head}");
    ok(&w, UiStep::Close).await;
}

/// feat-platform-10 (milestone follow), feat-native-1.
///
/// `homelab ui open add-native <stack>` writes a new unit's `service.yml`
/// and a generic systemd unit file under `<unit>/`, and appends it to
/// `natives:` — one commit, the same plan/commit flow.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_add_native_unit_commits_once() {
    let w = world("add-native").await;
    ok(&w, open("add-native", Some("admin"))).await;
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("is needed"), "{why}");
    ok(&w, typed("add-native-unit", "worker")).await;
    ok(&w, typed("add-native-binary", "/opt/worker/bin/worker")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(plan["state"]["form"]["edit"]["plan"]["valid"], true);
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "add the worker native unit")).await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), before + 1);
    let manifest = git(&w.bare, &["show", "main:stacks/admin/lxc-compose.yml"]);
    assert!(manifest.contains("worker"), "{manifest}");
    let svc = git(&w.bare, &["show", "main:stacks/admin/worker/service.yml"]);
    assert!(svc.contains("unit: worker"), "{svc}");
    ok(&w, UiStep::Close).await;
}

/// feat-platform-10 (milestone follow), feat-preset-1.
///
/// `homelab ui open preset <name>` edits `presets/<name>/preset.yml`
/// through its own plan and commit (`/data/presets/plan`,
/// `/data/presets/commit` — not a stack's), on the seeded `mealie` preset.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_preset_meta_commits_once() {
    let w = world("preset").await;
    let opened = ok(&w, open("preset", Some("mealie"))).await;
    assert_eq!(opened["state"]["page"], "/app/presets");
    let fields = opened["state"]["form"]["fields"]
        .as_array()
        .unwrap()
        .clone();
    let value_of = |id: &str| {
        fields
            .iter()
            .find(|x| x["id"] == id)
            .map(|x| x["value"].clone())
            .unwrap_or_default()
    };
    assert_eq!(value_of("preset-description"), "Recipes + meal planning");
    assert_eq!(value_of("preset-ram-mb"), "512");
    ok(&w, typed("preset-ram-mb", "1024")).await;
    let plan = ok(&w, press("next")).await;
    assert_eq!(plan["state"]["form"]["edit"]["plan"]["valid"], true);
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "mealie gets more memory")).await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), before + 1);
    let meta = git(&w.bare, &["show", "main:presets/mealie/preset.yml"]);
    assert!(meta.contains("ram_mb: 1024"), "{meta}");
    ok(&w, UiStep::Close).await;
}

/// feat-platform-10 (milestone follow), feat-preset-1.
///
/// `homelab ui open new-preset` names the preset in the form itself
/// (`new-preset-name`), then the same `preset_meta` fields; the plan
/// writes a brand-new `presets/<name>/preset.yml`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_new_preset_commits_once() {
    let w = world("new-preset").await;
    let opened = ok(&w, open("new-preset", None)).await;
    assert_eq!(opened["state"]["page"], "/app/presets");
    let (why, _) = refused(&step(&w, press("next")).await);
    assert!(why.contains("Preset name"), "{why}");
    ok(&w, typed("new-preset-name", "demo2")).await;
    ok(&w, typed("preset-description", "A demo preset")).await;
    ok(&w, typed("preset-ram-mb", "2048")).await;
    let plan = ok(&w, press("next")).await;
    let p = &plan["state"]["form"]["edit"]["plan"];
    assert_eq!(p["valid"], true, "{p}");
    ok(&w, press("next")).await;
    ok(&w, typed("edit-subject", "a demo preset")).await;
    let before = commits(&w);
    ok(&w, press("confirm")).await;
    assert_eq!(commits(&w), before + 1);
    let meta = git(&w.bare, &["show", "main:presets/demo2/preset.yml"]);
    assert!(meta.contains("A demo preset"), "{meta}");
    ok(&w, UiStep::Close).await;
}
