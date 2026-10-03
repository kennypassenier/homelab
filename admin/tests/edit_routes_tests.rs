//! milestone edit, the routes: a plan and a commit through the edit router
//! against a local bare repository (never GitHub) and a mock host; the
//! deploy after the commit goes into the act queue as exactly that commit;
//! the host settings are read and written with the session-only commands,
//! and arch-self keys never reach the host.

mod act_support;

use std::path::{Path, PathBuf};
use std::process::Command as Proc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{MemFiles, MockHost, RecPusher, Recorder, Script, TestClock, shared, temp_dir};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use homelab_admin::core::actions_config::GitConfig;
use homelab_admin::shell::actions::{Actions, ActionsDeps};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::edit::{EditCtx, router};
use homelab_admin::shell::workcopy::WorkingCopy;
use homelab_proto::Command;
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
    for s in ["admin", "kp-soft", "gateway", "registry", "metrics"] {
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
    host: Arc<MockHost>,
    live: Arc<Recorder>,
    app: axum::Router,
    sent: Arc<Mutex<Vec<Command>>>,
    shared: homelab_admin::shell::host_link::Shared,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn world(tag: &str) -> World {
    let root = temp_dir(&format!("edit-{tag}"));
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
                    message: serde_json::json!([
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
                    message: serde_json::json!({
                        "path": "/etc/homelab/host.toml", "sha256": "b".repeat(64),
                        "values": {"backup_hour": 4, "listen": "0.0.0.0:8443"},
                        "secrets_set": ["token"], "unknown": []
                    })
                    .to_string(),
                    ..Script::ok(&[])
                },
                Command::SetHostConfig { .. } => Script {
                    message: serde_json::json!({"sha256": "c".repeat(64), "live": ["backup_hour"], "restart": []}).to_string(),
                    ..Script::ok(&[])
                },
                Command::ApplyHostConfig { .. } => Script {
                    message: serde_json::json!({"sha256": "c".repeat(64), "live": ["backup_hour"], "restart": []}).to_string(),
                    ..Script::ok(&[])
                },
                _ => Script::ok(&[("run", 1)]),
            }
        }),
        serde_json::json!({ "entries": [] }),
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
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: live.clone(),
        files: Arc::new(MemFiles::default()),
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
    let app = router(EditCtx {
        wc,
        actions,
        host: host.clone(),
        shared: shared.clone(),
        publish: live.clone(),
    });
    World {
        root,
        bare,
        host,
        live,
        app,
        sent,
        shared,
    }
}

async fn call(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let resp = app
        .clone()
        .oneshot(
            req.body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feat_stacks_2_plan_then_commit_then_deploy_that_commit() {
    let w = world("stack").await;
    let (st, v) = call(&w.app, "GET", "/data/stacks/kp-soft/edit", None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["manifest"]["resources"]["memory_mb"], 2048);
    assert!(
        v["texts"]["lxc-compose.yml"]
            .as_str()
            .unwrap()
            .contains("stack_name: kp-soft")
    );
    let edit = serde_json::json!({ "kind": "settings", "memory_mb": 3072, "order": 80 });
    let (st, plan) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/plan",
        Some(serde_json::json!({ "edit": edit })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{plan}");
    assert_eq!(plan["valid"], true, "{plan}");
    assert_eq!(plan["files"][0]["path"], "stacks/kp-soft/lxc-compose.yml");
    assert_eq!(plan["follow_ups"], serde_json::json!(["deploy", "resize"]));
    assert_eq!(
        plan["subject"],
        "stacks/kp-soft: memory 3 GB, boot order 80 [feat-stacks-2]"
    );
    // The host's applied files against what a deploy would send now.
    let applied = plan["applied"]["changes"].as_array().unwrap();
    assert!(
        applied.iter().any(|c| c == "~ kp-soft/docker-compose.yml"),
        "{plan}"
    );
    // A plan writes nothing.
    assert_eq!(git(&w.bare, &["rev-list", "--count", "main"]).trim(), "1");

    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/commit",
        Some(
            serde_json::json!({ "edit": edit, "subject": "kp-soft gets 3 GB", "follow": "deploy" }),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let commit = v["committed"]["commit"].as_str().unwrap().to_string();
    assert_eq!(git(&w.bare, &["rev-parse", "main"]).trim(), commit);
    assert_eq!(
        git(&w.bare, &["log", "-1", "--format=%s", "main"]).trim(),
        "kp-soft gets 3 GB [feat-stacks-2]"
    );
    let body = git(&w.bare, &["log", "-1", "--format=%b", "main"]);
    assert!(
        body.contains("Resize applies the raised resources"),
        "{body}"
    );
    assert!(v["follow"]["job"].is_u64(), "{v}");
    let jobs = w.live.events("action");
    let job = jobs
        .iter()
        .find(|j| j["job"] == v["follow"]["job"])
        .unwrap();
    assert_eq!(job["action"], "deploy-commit");
    assert_eq!(job["args"]["commit"], commit);
    assert!(!w.live.events("repo").is_empty());

    // The plan made before is stale now: the same edit changes nothing.
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/commit",
        Some(serde_json::json!({ "edit": edit })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert!(
        v["why"].as_str().unwrap().contains("nothing changes"),
        "{v}"
    );
    let _ = &w.host;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feat_firewall_1_a_bad_rule_is_refused_and_the_matrix_reads_the_repo() {
    let w = world("fw").await;
    let (_, v) = call(&w.app, "GET", "/data/stacks/admin/edit", None).await;
    let fw = v["manifest"]["firewall"].clone();
    let mut rules: Vec<serde_json::Value> = fw["rules"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, r)| serde_json::json!({ "origin": i, "rule": r }))
        .collect();
    // 10.10.10.4/24 means the whole VLAN to Proxmox: refused by homelab-core.
    rules.push(serde_json::json!({ "rule": { "dir": "in", "action": "ACCEPT", "source": "10.10.10.4/24", "proto": "tcp", "dport": "22" } }));
    let edit = serde_json::json!({
        "kind": "firewall", "enabled": true, "comment": fw["comment"], "policy_in": "DROP",
        "policy_out": "ACCEPT", "management_open": null, "rules": rules,
    });
    let (st, plan) = call(
        &w.app,
        "POST",
        "/data/stacks/admin/plan",
        Some(serde_json::json!({ "edit": edit })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(plan["valid"], false);
    assert!(
        plan["problems"][0]
            .as_str()
            .unwrap()
            .contains("host bits set"),
        "{plan}"
    );
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/admin/commit",
        Some(serde_json::json!({ "edit": edit })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert!(v["why"].as_str().unwrap().contains("host bits"));
    assert_eq!(git(&w.bare, &["rev-list", "--count", "main"]).trim(), "1");

    let (st, m) = call(&w.app, "GET", "/data/firewall", None).await;
    assert_eq!(st, StatusCode::OK, "{m}");
    assert!(m["matrix"]["rules"].as_array().unwrap().len() > 10);
    assert!(
        m["stacks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["stack"] == "admin" && s["enabled"] == true)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feat_stacks_3_a_new_stack_is_scaffolded_committed_and_only_its_directory() {
    let w = world("new").await;
    let (st, p) = call(&w.app, "GET", "/data/presets", None).await;
    assert_eq!(st, StatusCode::OK);
    let names: Vec<&str> = p["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["mealie", "custom"]);
    // redesign-presets-1: the gallery says "default" where the preset
    // leaves a size to the fleet, so the read says which ones it sets.
    assert_eq!(p["presets"][0]["cores_set"], false, "{p}");
    assert_eq!(p["presets"][0]["disk_set"], false, "{p}");
    // Clones on the way (the repo route or a plan clones it first).
    let v = p["suggest_vmid"].as_u64().unwrap();
    assert!(v >= 104 && ![104, 116, 120].contains(&v));
    let req = serde_json::json!({ "name": "recipes", "vmid": 121, "preset": "mealie", "ram_mb": 512, "cores": 1, "disk_gb": 8 });
    let (st, paths) = call(
        &w.app,
        "POST",
        "/data/stacks-new/appdata",
        Some(serde_json::json!({ "preset": "mealie", "name": "recipes", "vmid": 121 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        paths["appdata"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str().unwrap().starts_with("/appdata/recipes/")),
        "{paths}"
    );
    let (st, plan) = call(&w.app, "POST", "/data/stacks-new/plan", Some(req.clone())).await;
    assert_eq!(st, StatusCode::OK, "{plan}");
    assert_eq!(plan["valid"], true, "{plan}");
    assert!(
        plan["effects"][0]["what"]
            .as_str()
            .unwrap()
            .contains("creates CT 121")
    );
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks-new/commit",
        Some(serde_json::json!({ "stack": req })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let files = git(&w.bare, &["show", "--name-only", "--format=", "main"]);
    assert!(
        files.lines().all(|l| l.starts_with("stacks/recipes/")),
        "{files}"
    );
    assert!(files.contains("stacks/recipes/lxc-compose.yml"));
    // Taken now: the same name is refused.
    let (st, v) = call(&w.app, "POST", "/data/stacks-new/plan", Some(req)).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert!(
        v["why"].as_str().unwrap().contains("recipes already"),
        "{v}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feat_settings_1_read_and_write_with_the_session_commands_only() {
    let w = world("settings").await;
    // A host older than the commands is not asked (it would drop them).
    // The gate is the next release, 3.63.0: 3.62.3 is refused too.
    w.shared.write().await.host_version = Some("3.62.3".into());
    let (st, v) = call(&w.app, "GET", "/data/host-settings", None).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert!(v["why"].as_str().unwrap().contains("3.62.3"));
    assert!(v["why"].as_str().unwrap().contains("3.63.0"), "{v}");
    assert!(w.sent.lock().unwrap().is_empty());

    w.shared.write().await.host_version = Some("3.63.0".into());
    let (st, v) = call(&w.app, "GET", "/data/host-settings", None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let fields = v["page"]["fields"].as_array().unwrap();
    let f = |k: &str| fields.iter().find(|x| x["key"] == k).unwrap().clone();
    assert_eq!(f("backup_hour")["value"], 4);
    assert_eq!(f("listen")["access"], "locked");
    assert_eq!(f("token")["set"], true);
    assert!(f("token")["value"].is_null());

    // arch-self: a locked key never reaches the host.
    let (st, v) = call(
        &w.app,
        "PUT",
        "/data/host-settings",
        Some(serde_json::json!({ "expect_sha256": "b".repeat(64), "values": { "listen": "0.0.0.0:9443" } })),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(v["why"].as_str().unwrap().contains("arch-self"), "{v}");
    let (st, v) = call(
        &w.app,
        "PUT",
        "/data/host-settings",
        Some(
            serde_json::json!({ "expect_sha256": "b".repeat(64), "values": { "backup_hour": 5 } }),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["saved"]["live"][0], "backup_hour");
    assert!(v["committed"]["commit"].as_str().is_some(), "{v}");
    let sent = w.sent.lock().unwrap().clone();
    let names: Vec<&str> = sent.iter().map(|c| c.name()).collect();
    // fix-110: the change is committed to config/host.toml in the working
    // copy first (no RPC for that — it is a local git transaction, like a
    // stack's commit); the dashboard then reads host.toml's sha256 fresh
    // (not the page's possibly-stale one) and sends the whole file.
    // Never GetConfig/SetConfig: their frames go to every session.
    assert_eq!(
        names,
        vec!["get_host_config", "get_host_config", "apply_host_config"]
    );
    match &sent[2] {
        Command::ApplyHostConfig {
            toml,
            expect_sha256,
            ..
        } => {
            assert!(toml.contains("backup_hour = 5"), "{toml}");
            assert_eq!(expect_sha256.as_deref(), Some("b".repeat(64).as_str()));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(w.live.events("host_settings").len(), 1);
    // The commit landed in the repository's working copy, under
    // config/host.toml, same as a stack's edit under stacks/<stack>/.
    let text = std::fs::read_to_string(w.root.join("data/repo/config/host.toml")).unwrap();
    assert!(text.contains("backup_hour = 5"), "{text}");
}

/// TUI parity (`homelab import`): a stack's export bundle becomes a new
/// stack through the plan and the commit every edit ends in; its identity
/// is the new one, a taken name or a bundle that is not one is refused
/// before anything is written, and a bundle carrying a .env is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parity_an_export_bundle_imports_as_a_new_stack() {
    let w = world("import").await;
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks/registry");
    let (bundle, n) = homelab_client::spec::bundle_text(&src).unwrap();
    assert!(n > 0);
    let req = serde_json::json!({ "bundle": bundle, "name": "registry2", "vmid": 197 });
    let (st, plan) = call(
        &w.app,
        "POST",
        "/data/stacks-import/plan",
        Some(req.clone()),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{plan}");
    assert_eq!(plan["valid"], true, "{plan}");
    assert!(
        plan["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|f| f["path"].as_str().unwrap().starts_with("stacks/registry2/"))
    );
    assert_eq!(plan["follow_ups"], serde_json::json!(["deploy"]), "{plan}");
    assert_eq!(git(&w.bare, &["rev-list", "--count", "main"]).trim(), "1");

    // Refusals change nothing.
    for (body, why) in [
        (
            serde_json::json!({ "bundle": bundle, "name": "registry", "vmid": 197 }),
            "already",
        ),
        (
            serde_json::json!({ "bundle": bundle, "name": "registry2", "vmid": 101 }),
            "no-touch",
        ),
        (
            serde_json::json!({ "bundle": "not: a bundle", "name": "registry2", "vmid": 197 }),
            "bundle",
        ),
        (
            serde_json::json!({
                "bundle": bundle.replacen("files:\n", "files:\n- path: registry/.env\n  content: SECRET=1\n", 1),
                "name": "registry2", "vmid": 197
            }),
            "secrets file",
        ),
    ] {
        let (st, v) = call(&w.app, "POST", "/data/stacks-import/plan", Some(body)).await;
        assert_eq!(st, StatusCode::CONFLICT, "{v}");
        assert!(v["why"].as_str().unwrap().contains(why), "{why}: {v}");
    }
    assert_eq!(git(&w.bare, &["rev-list", "--count", "main"]).trim(), "1");

    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks-import/commit",
        Some(serde_json::json!({ "edit": req, "subject": "registry2 from the registry bundle" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let commit = v["committed"]["commit"].as_str().unwrap().to_string();
    assert_eq!(git(&w.bare, &["rev-parse", "main"]).trim(), commit);
    let manifest = git(&w.bare, &["show", "main:stacks/registry2/lxc-compose.yml"]);
    assert!(manifest.contains("stack_name: registry2"), "{manifest}");
    assert!(manifest.contains("vmid: 197"), "{manifest}");
    let changed = git(&w.bare, &["show", "--name-only", "--format=", "main"]);
    assert!(
        changed.lines().all(|l| l.starts_with("stacks/registry2/")),
        "{changed}"
    );
}

/// feat-stacks-files: the general file editor's create, delete and rename,
/// through the very `/plan` and `/commit` routes every edit ends in — and
/// checks.yml now validated as `homelab_core::checks::ServiceChecks`
/// rather than only as YAML.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feat_stacks_files_create_delete_rename_and_checks_schema() {
    let w = world("files").await;

    // Create: a brand new file under the stack, template content and all.
    let create = serde_json::json!({
        "kind": "files", "op": "create",
        "path": "new-note.yml", "content": "hello: world\n"
    });
    let (st, plan) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/plan",
        Some(serde_json::json!({ "edit": create })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{plan}");
    assert_eq!(plan["valid"], true, "{plan}");
    assert_eq!(plan["files"][0]["status"], "added", "{plan}");
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/commit",
        Some(serde_json::json!({ "edit": create, "subject": "a new note" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let commit1 = v["committed"]["commit"].as_str().unwrap().to_string();
    assert_eq!(
        git(&w.bare, &["show", "main:stacks/kp-soft/new-note.yml"]),
        "hello: world\n"
    );

    // Create refuses a path that already exists. `changes()` refuses
    // before any staging, so this is a CONFLICT with a Refusal body, the
    // same shape every other pre-stage refusal in this router takes (see
    // the import test's "Refusals change nothing" block above).
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/plan",
        Some(serde_json::json!({ "edit": {
            "kind": "files", "op": "create",
            "path": "new-note.yml", "content": "x"
        } })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    assert!(v["why"].as_str().unwrap().contains("already"), "{v}");

    // Create refuses a secret and a path escaping the stack.
    for bad in ["kp-soft/.env", "../escape.yml"] {
        let (st, v) = call(
            &w.app,
            "POST",
            "/data/stacks/kp-soft/plan",
            Some(serde_json::json!({ "edit": {
                "kind": "files", "op": "create", "path": bad, "content": "x"
            } })),
        )
        .await;
        assert_eq!(st, StatusCode::CONFLICT, "{bad}: {v}");
    }

    // Rename: the new note moves, old path gone, new one holds its text.
    let rename = serde_json::json!({
        "kind": "files", "op": "rename",
        "from": "new-note.yml", "to": "renamed-note.yml"
    });
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/commit",
        Some(serde_json::json!({ "edit": rename, "subject": "rename the note" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(
        git(&w.bare, &["show", "main:stacks/kp-soft/renamed-note.yml"]),
        "hello: world\n"
    );
    let (st, _) = call(&w.app, "GET", "/data/stacks/kp-soft/edit", None).await;
    assert_eq!(st, StatusCode::OK);

    // Delete: the renamed note goes away; the manifest may not be deleted.
    let delete = serde_json::json!({ "kind": "files", "op": "delete", "path": "renamed-note.yml" });
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/commit",
        Some(serde_json::json!({ "edit": delete, "subject": "drop the note" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let ls = git(&w.bare, &["ls-tree", "-r", "--name-only", "main"]);
    // The positive twin: the delete removed the one file, not the whole
    // tree — the stack's own compose file is still there.
    assert!(ls.contains("stacks/kp-soft/lxc-compose.yml"), "{ls}");
    assert!(!ls.contains("renamed-note.yml"), "{ls}");

    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/plan",
        Some(serde_json::json!({ "edit": {
            "kind": "files", "op": "delete", "path": "lxc-compose.yml"
        } })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "{v}");
    assert!(v["why"].as_str().unwrap().contains("manifest"), "{v}");

    // checks.yml is now judged as homelab_core::checks::ServiceChecks, not
    // only as YAML: a value of the wrong shape is refused before commit.
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks/kp-soft/plan",
        Some(serde_json::json!({ "edit": {
            "kind": "raw", "path": "kp-soft/checks.yml",
            "content": "checks:\n  - name: x\n    command: echo x\n    expect: not_a_real_expect\n    layer: process\n"
        } })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["valid"], false, "{v}");
    assert!(
        v["problems"][0]
            .as_str()
            .unwrap()
            .contains("kp-soft/checks.yml"),
        "{v}"
    );
    let _ = commit1;
}

/// redesign-stacks-8: New stack's third route, "Empty" (FLOWS.md §1.5, §2,
/// §3 #6): the same plan and commit routes scaffold a stack with no apps
/// when no preset is named — no preset file needed — and the commit holds
/// only that stack's directory with an empty app list.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn redesign_stacks_8_an_empty_stack_is_scaffolded_with_no_apps_and_no_preset() {
    let w = world("empty").await;
    let (st, _) = call(&w.app, "GET", "/data/presets", None).await;
    assert_eq!(st, StatusCode::OK);
    let req = serde_json::json!({ "name": "blank", "vmid": 122, "preset": "", "ram_mb": 1024, "cores": 1, "disk_gb": 8 });
    let (st, paths) = call(
        &w.app,
        "POST",
        "/data/stacks-new/appdata",
        Some(serde_json::json!({ "preset": "", "name": "blank", "vmid": 122 })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(paths["appdata"], serde_json::json!([]), "{paths}");
    let (st, plan) = call(&w.app, "POST", "/data/stacks-new/plan", Some(req.clone())).await;
    assert_eq!(st, StatusCode::OK, "{plan}");
    assert_eq!(plan["valid"], true, "{plan}");
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/stacks-new/commit",
        Some(serde_json::json!({ "stack": req })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let files = git(&w.bare, &["show", "--name-only", "--format=", "main"]);
    assert_eq!(files.trim(), "stacks/blank/lxc-compose.yml", "{files}");
    let manifest = git(&w.bare, &["show", "main:stacks/blank/lxc-compose.yml"]);
    assert!(
        manifest.contains("apps: []") && manifest.contains("no_apps_yet: true"),
        "an empty stack declares no apps:\n{manifest}"
    );
}
