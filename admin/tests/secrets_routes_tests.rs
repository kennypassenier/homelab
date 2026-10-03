//! feat-secrets-3/6 (redesign 3.71, Kenny 2026-10-03): the Secrets page's
//! routes against a local bare repository (never GitHub) and a mock host —
//! every stack's declared names in one read, a broken stack file named as
//! unreadable with its fix, and a reveal or copy that tells the host who
//! asked and why, asking an older host again in the shape it knows.

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
use homelab_admin::shell::edit::EditCtx;
use homelab_admin::shell::secrets::{SecretsCtx, router};
use homelab_admin::shell::workcopy::WorkingCopy;
use homelab_proto::{Command, RevealPurpose};
use tower::ServiceExt as _;

fn git(dir: &Path, args: &[&str]) {
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
}

/// A bare repository with three made-up stacks: two secrets, one latch
/// file, and one whose file does not parse. No real stack, no `.env`.
fn seed(root: &Path) -> PathBuf {
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
    let write = |stack: &str, text: &str| {
        std::fs::create_dir_all(seed.join("stacks").join(stack)).unwrap();
        std::fs::write(
            seed.join("stacks").join(stack).join("lxc-compose.yml"),
            text,
        )
        .unwrap();
    };
    write("edge", "stack_name: edge\nlatch_secrets: [proxy, tunnel]\n");
    write(
        "panel",
        "stack_name: panel\nlatch_files:\n  - from: panel/panel.env\n    dest: /etc/panel.env\n    mode: \"600\"\n",
    );
    write("broken", "stack_name: [unclosed\n");
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "--quiet", "-m", "seed [meta]"]);
    git(&seed, &["push", "--quiet", "origin", "main"]);
    bare
}

struct World {
    root: PathBuf,
    app: axum::Router,
    sent: Arc<Mutex<Vec<Command>>>,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// `old_host`: the mock answers like a host before 3.71.0, refusing the
/// `audit` field (fix-211) and revealing only without it.
async fn world(tag: &str, old_host: bool) -> World {
    let root = temp_dir(&format!("secrets-{tag}"));
    let bare = seed(&root);
    let clock = TestClock::at(1_800_000_000);
    let sent: Arc<Mutex<Vec<Command>>> = Arc::default();
    let s2 = sent.clone();
    let host = MockHost::start(
        clock.clone(),
        Arc::new(move |c: &Command| {
            s2.lock().unwrap().push(c.clone());
            match c {
                Command::RevealSecret { audit: Some(_), .. } if old_host => Script {
                    ok: false,
                    message: "refused: this host's build does not know audit — update the \
                              host ('homelab release-update') and try again, or drop the \
                              field if it was sent by mistake"
                        .into(),
                    ..Script::ok(&[])
                },
                Command::RevealSecret { .. } => Script {
                    message: "PROXY_TOKEN=made-up".into(),
                    ..Script::ok(&[])
                },
                _ => Script::ok(&[]),
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
    let shared = shared(&[("edge", 104, None), ("panel", 120, None)]);
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
    wc.sync().expect("the working copy clones the seed");
    let app = router(SecretsCtx {
        edit: EditCtx {
            wc,
            actions,
            host: host.clone(),
            shared,
            publish: live,
        },
        driver: None,
        viewer_name: Some("Kenny".into()),
    });
    World { root, app, sent }
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

/// The audits the host was sent, in order: (purpose, by), None for a
/// reveal sent without one.
fn audits(w: &World) -> Vec<Option<(RevealPurpose, String)>> {
    w.sent
        .lock()
        .unwrap()
        .iter()
        .filter_map(|c| match c {
            Command::RevealSecret { audit, .. } => {
                Some(audit.as_ref().map(|a| (a.purpose, a.by.clone())))
            }
            _ => None,
        })
        .collect()
}

/// covers: feat-secrets-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feat_secrets_3_every_stack_in_one_read_and_a_broken_file_is_named_not_empty() {
    let w = world("all", false).await;
    let (st, v) = call(&w.app, "GET", "/data/secrets", None).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(
        v["stacks"]["edge"]["secrets"],
        serde_json::json!(["proxy", "tunnel"])
    );
    assert_eq!(v["stacks"]["panel"]["files"][0]["from"], "panel/panel.env");
    let why = v["stacks"]["broken"]["unreadable"]
        .as_str()
        .expect("named unreadable");
    assert!(why.contains(" :: "), "why :: fix, {why}");
    assert!(v["stacks"]["edge"].get("unreadable").is_none(), "{v}");
    // One stack, and one the repository does not hold (declares none).
    let (st, one) = call(&w.app, "GET", "/data/secrets/edge", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(one["secrets"].as_array().unwrap().len(), 2);
    let (st, gone) = call(&w.app, "GET", "/data/secrets/films", None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(gone["secrets"], serde_json::json!([]));
}

/// covers: feat-secrets-6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feat_secrets_6_a_reveal_and_a_copy_tell_the_host_who_asked_and_why() {
    let w = world("audit", false).await;
    let secret = serde_json::json!({ "kind": "env", "app": "proxy" });
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/secrets/edge/reveal",
        Some(serde_json::json!({ "secret": secret })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["value"], "PROXY_TOKEN=made-up");
    let (st, _) = call(
        &w.app,
        "POST",
        "/data/secrets/edge/reveal",
        // `driven` without Live view driving is the person's own click.
        Some(serde_json::json!({ "secret": secret, "purpose": "copy", "driven": true })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        audits(&w),
        vec![
            Some((RevealPurpose::Reveal, "Kenny".into())),
            Some((RevealPurpose::Copy, "Kenny".into())),
        ]
    );
}

/// covers: feat-secrets-6
///
/// Invariant 14: a host before 3.71.0 refuses the unknown `audit` field;
/// the reveal still works, asked once more without it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feat_secrets_6_an_older_host_still_reveals_asked_again_without_the_audit() {
    let w = world("old", true).await;
    let (st, v) = call(
        &w.app,
        "POST",
        "/data/secrets/edge/reveal",
        Some(serde_json::json!({ "secret": { "kind": "env", "app": "proxy" } })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["value"], "PROXY_TOKEN=made-up");
    assert_eq!(
        audits(&w),
        vec![Some((RevealPurpose::Reveal, "Kenny".into())), None]
    );
}
