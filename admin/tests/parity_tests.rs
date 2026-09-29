//! The TUI parity round (Kenny, 2026-09-28: "Wat nu in de TUI kan, moet nog
//! altijd kunnen in ons systeem"): the security-relevant paths and the new
//! logic. The deploy key on CT 120, the host's log stream (masked), "Update
//! host" against a fake GitHub, the install-native version gate, drift, the
//! templates list, and that the demo host stays out of a release build.

mod act_support;

use std::os::unix::fs::PermissionsExt as _;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{
    history, shared, temp_dir, until, MemFiles, MockHost, Recorder, Script, TestClock,
};
use homelab_admin::core::actions::{validate, ActionArgs, ActionKind, HOST_TARGET};
use homelab_admin::core::actions_config::GitConfig;
use homelab_admin::core::credentials::{decode_deploy_key, GITHUB_KNOWN_HOSTS};
use homelab_admin::core::drift::{drift_state, DriftState};
use homelab_admin::shell::actions::{Actions, ActionsDeps, JobState, Origin};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::hostlog::{After, HostLog};
use homelab_admin::shell::releases::Releases;
use homelab_admin::shell::workcopy::provision_credentials;
use homelab_proto::{Command, LogLevel, ServerMsg};

const KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ\n-----END OPENSSH PRIVATE KEY-----";

fn b64(s: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(s)
}

fn git(dir: &std::path::Path, remote: &str) -> GitConfig {
    GitConfig {
        remote: remote.into(),
        branch: "main".into(),
        key: dir.join("keys/deploy_key"),
        known_hosts: dir.join("keys/known_hosts"),
        author_name: "t".into(),
        author_email: "t@example.com".into(),
    }
}

/// Security: CT 120 gets the deploy key as HOMELAB_ADMIN_DEPLOY_KEY_B64; the
/// file is written once, mode 0600, never over an existing one, and
/// GitHub's pinned host keys go next to it. No refusal carries the key.
#[test]
fn parity_the_deploy_key_is_written_0600_from_the_environment() {
    let dir = temp_dir("cred");
    let g = git(&dir, "git@github.com:kennypassenier/homelab.git");
    let p = provision_credentials(&g, Some(&b64(KEY)));
    assert!(p.key_written && p.known_hosts_written, "{p:?}");
    assert!(p.problems.is_empty(), "{p:?}");
    let mode = std::fs::metadata(&g.key).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the key file is 0600");
    let text = std::fs::read_to_string(&g.key).unwrap();
    assert!(text.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
    assert!(text.ends_with('\n'), "ssh wants the last newline");
    let kh = std::fs::read_to_string(&g.known_hosts).unwrap();
    assert_eq!(kh, GITHUB_KNOWN_HOSTS);
    assert_eq!(
        kh.lines().filter(|l| l.starts_with("github.com ")).count(),
        3
    );

    // Present already: left alone, whatever the variable says now.
    std::fs::write(&g.key, "kept").unwrap();
    let p = provision_credentials(&g, Some(&b64("-----BEGIN X PRIVATE KEY-----\nother\n")));
    assert!(!p.key_written && !p.known_hosts_written);
    assert_eq!(std::fs::read_to_string(&g.key).unwrap(), "kept");
}

#[test]
fn parity_a_bad_deploy_key_is_refused_without_a_file_or_its_value_in_the_reason() {
    let dir = temp_dir("cred-bad");
    let g = git(&dir, "git@github.com:kennypassenier/homelab.git");
    let secretish = "c2VjcmV0LXRoYXQtaXMtbm90LWEta2V5";
    let p = provision_credentials(&g, Some(secretish));
    assert!(!p.key_written);
    assert!(
        !g.key.exists(),
        "nothing written for a value that is not a key"
    );
    assert_eq!(p.problems.len(), 1);
    assert!(!p.problems[0].contains(secretish), "{:?}", p.problems);
    assert!(decode_deploy_key("not base64 at all!")
        .unwrap_err()
        .contains("not base64"));
    assert!(!decode_deploy_key("!!!").unwrap_err().contains("!!!"));
    // No variable: nothing to do but the host keys.
    let dir = temp_dir("cred-none");
    let g = git(&dir, "git@github.com:kennypassenier/homelab.git");
    let p = provision_credentials(&g, None);
    assert!(!p.key_written && p.known_hosts_written && p.problems.is_empty());
    // A local remote needs neither.
    let dir = temp_dir("cred-local");
    let g = git(&dir, "/srv/git/homelab.git");
    let p = provision_credentials(&g, Some(&b64(KEY)));
    assert!(!p.key_written && !p.known_hosts_written);
    assert!(!g.key.exists() && !g.known_hosts.exists());
}

fn log(source: &str, msg: &str, by: Option<&str>) -> ServerMsg {
    ServerMsg::Log {
        level: LogLevel::Info,
        source: source.into(),
        msg: msg.into(),
        req: by.map(|_| 7),
        ts: Some(1_790_000_000),
        step: None,
        by: by.map(str::to_string),
    }
}

/// LOG_STREAM: every host line, whoever asked (a CLI session, the nightly
/// round), secrets masked before they leave the server, filtered per stack,
/// and a page catches up after the last line it has.
#[test]
fn parity_the_log_stream_carries_every_line_masked_and_filterable() {
    let live = Arc::new(Recorder::default());
    let clock = TestClock::at(1_790_000_000);
    let h = HostLog::new(live.clone(), clock.clock());
    h.take(&log("media", "[deploy] pull jellyfin", Some("wsl")));
    h.take(&log("NIGHT", "backup of home done", None));
    h.take(&log(
        "home",
        "curl -H 'Authorization: Bearer abcdef0123456789' https://x",
        Some("admin"),
    ));
    let all = h.lines(&After {
        after: 0,
        source: None,
        limit: 100,
    });
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].by.as_deref(), Some("wsl"));
    assert_eq!(all[1].by, None, "the nightly round's line has no session");
    assert!(!all[2].msg.contains("abcdef0123456789"), "{}", all[2].msg);
    let pushed = live.events("host_log");
    assert_eq!(pushed.len(), 3);
    assert!(!pushed[2].to_string().contains("abcdef0123456789"));
    let media = h.lines(&After {
        after: 0,
        source: Some("media".into()),
        limit: 100,
    });
    assert_eq!(media.len(), 1);
    let later = h.lines(&After {
        after: all[1].seq,
        source: None,
        limit: 100,
    });
    assert_eq!(later.len(), 1);
    assert_eq!(h.sources(), vec!["NIGHT", "home", "media"]);
    // A transfer's counter goes out, and its last one always.
    for done in [10u64, 20, 100] {
        h.take(&ServerMsg::Transfer {
            op: "backup-media".into(),
            label: "restic".into(),
            done,
            total: Some(100),
        });
    }
    let t = live.events("transfer");
    assert!(!t.is_empty() && t.len() <= 3);
    assert_eq!(t.last().unwrap()["done"], 100);
    assert_eq!(h.transfers().len(), 1);
    // CurrentOp's lines seed the ring once, not twice.
    let view = homelab_proto::CurrentOpView {
        holder: Some("deploy media".into()),
        started_unix: Some(1),
        lines: vec![log("media", "[deploy] pull jellyfin", Some("wsl"))],
    };
    h.seed(&view);
    assert_eq!(
        h.lines(&After {
            after: 0,
            source: None,
            limit: 100
        })
        .len(),
        3
    );
}

#[test]
fn parity_drift_says_only_what_was_compared() {
    let ok = |h: &str| Ok::<String, String>(h.to_string());
    assert_eq!(drift_state("abc", Some(&ok("abc"))), DriftState::Same);
    assert_eq!(drift_state("abc", Some(&ok("abd"))), DriftState::Changed);
    assert_eq!(drift_state("", Some(&ok("abc"))), DriftState::NeverApplied);
    assert_eq!(drift_state("abc", None), DriftState::NoLocalFiles);
    assert_eq!(
        drift_state("abc", Some(&Err("latch failed".into()))),
        DriftState::NotCompared
    );
    assert!(DriftState::Changed.label().starts_with("[CHANGED]"));
}

#[test]
fn parity_the_templates_answer_reads_into_two_lists() {
    let text = "clonable golden templates (fast):\nclone:996  debian-13-homelab-v4\nclone:995  debian-13-homelab-v4-priv\n\nOS templates (full bootstrap):\n  local:vztmpl/debian-13-standard_13.1-2_amd64.tar.zst\n  local:vztmpl/debian-12-standard_12.7-1_amd64.tar.zst\n";
    let t = homelab_admin::core::templates::parse(text);
    assert_eq!(
        t.clones,
        vec![
            (996, "debian-13-homelab-v4".into()),
            (995, "debian-13-homelab-v4-priv".into())
        ]
    );
    assert_eq!(t.os.len(), 2);
    assert!(t.os[0].starts_with("local:vztmpl/debian-13"));
    let none = homelab_admin::core::templates::parse(
        "clonable golden templates (fast):\n  (none — run 'homelab template-build')\n\nOS templates (full bootstrap):\n",
    );
    assert!(none.clones.is_empty() && none.os.is_empty());
}

#[test]
fn parity_versions_warn_about_an_update_and_an_older_dashboard() {
    use homelab_admin::core::hostversion::{
        at_least, dashboard_older, update_available, NEXT_RELEASE,
    };
    assert_eq!(NEXT_RELEASE, (3, 63, 0));
    assert!(update_available(Some("v3.63.0"), Some("3.62.2")));
    assert!(!update_available(Some("v3.62.2"), Some("3.62.2")));
    assert!(!update_available(None, Some("3.62.2")));
    assert!(dashboard_older("3.62.2", Some("3.63.0")));
    assert!(!dashboard_older("3.63.0", Some("3.63.0")));
    assert!(!dashboard_older("3.63.0", Some("demo")));
    assert!(at_least("x", Some("3.63.0"), NEXT_RELEASE, "f").is_ok());
    let r = at_least("x", Some("3.62.3"), NEXT_RELEASE, "f").unwrap_err();
    assert!(r.why.contains("3.63.0"), "{r}");
    assert!(at_least("x", None, NEXT_RELEASE, "f").is_err());
}

// ── the queue: Update host, install-native ──────────────────────────────

struct FakeGitHub {
    binary: Result<String, String>,
    asked: Mutex<Vec<String>>,
}

impl Releases for FakeGitHub {
    fn latest_tag<'a>(
        &'a self,
        repo: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        self.asked.lock().unwrap().push(format!("latest {repo}"));
        Box::pin(async { Ok("v3.63.0".to_string()) })
    }
    fn host_binary<'a>(
        &'a self,
        tag: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        self.asked.lock().unwrap().push(format!("binary {tag}"));
        let b = self.binary.clone();
        Box::pin(async move { b })
    }
}

fn queue(
    tag: &str,
    binary: Result<String, String>,
) -> (
    Actions,
    Arc<MockHost>,
    Arc<Recorder>,
    homelab_admin::shell::host_link::Shared,
    Arc<FakeGitHub>,
) {
    let clock = TestClock::at(1_790_000_000);
    let host = MockHost::start(
        clock.clone(),
        Arc::new(|_c: &Command| Script::ok(&[("restart", 1)])),
        history("x", &[]),
        Arc::new(Mutex::new(Vec::new())),
    );
    let live = Arc::new(Recorder::default());
    let dir = temp_dir(tag);
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
    let gh = Arc::new(FakeGitHub {
        binary,
        asked: Mutex::new(Vec::new()),
    });
    actions.set_releases(gh.clone());
    actions.set_reconnect_wait(Duration::from_secs(5));
    (actions, host, live, shared, gh)
}

/// dash-host-update: the verified binary goes out, the line drops and comes
/// back as the new version, and only then is the job done (fix-121).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parity_update_host_waits_for_the_new_version_to_answer() {
    let (actions, host, _live, shared, gh) = queue("uh", Ok("QUJD".into()));
    shared.write().await.host_version = Some("3.62.2".into());
    let req = validate(HOST_TARGET, "update-host", ActionArgs::default()).unwrap();
    let job = actions.press(req, Origin::Manual).await.unwrap();
    until("the binary was sent", || {
        host.ran().iter().any(|(_, n, _)| n == "self_update_host")
    })
    .await;
    // The host restarts: the line drops, then the new one says Hello.
    shared.write().await.link_error = Some("the host closed the line".into());
    tokio::time::sleep(Duration::from_millis(700)).await;
    {
        let mut s = shared.write().await;
        s.link_error = None;
        s.host_version = Some("3.63.0".into());
    }
    until("the job ends", || {
        actions
            .job(job.job)
            .is_some_and(|j| j.state != JobState::Queued && j.state != JobState::Running)
    })
    .await;
    let j = actions.job(job.job).unwrap();
    assert_eq!(j.state, JobState::Done, "{:?}", j.message);
    assert!(j.message.unwrap_or_default().contains("3.63.0"));
    assert_eq!(j.cli.as_deref(), Some("homelab release-update v3.63.0"));
    assert_eq!(
        gh.asked.lock().unwrap().clone(),
        vec!["latest kennypassenier/homelab", "binary v3.63.0"]
    );
}

/// A release that does not verify is refused before anything is sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parity_update_host_sends_nothing_for_an_unsigned_release() {
    let (actions, host, _live, _shared, _gh) = queue(
        "uh-bad",
        Err("homelab-host v3.63.0: the release is not signed (no SHA256SUMS.minisig)".into()),
    );
    let req = validate(
        HOST_TARGET,
        "update-host",
        ActionArgs {
            tag: Some("v3.63.0".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let job = actions.press(req, Origin::Manual).await.unwrap();
    until("the job ends", || {
        actions
            .job(job.job)
            .is_some_and(|j| j.state == JobState::Refused)
    })
    .await;
    assert!(actions
        .job(job.job)
        .unwrap()
        .message
        .unwrap()
        .contains("not signed"));
    assert!(
        !host.ran().iter().any(|(_, n, _)| n == "self_update_host"),
        "{:?}",
        host.ran()
    );
}

/// install-native goes only to a host that knows the command (3.63.0).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parity_install_native_waits_for_a_host_that_knows_it() {
    let (actions, _host, _live, shared, _gh) = queue("inst", Ok(String::new()));
    shared.write().await.host_version = Some("3.62.2".into());
    let req = validate("admin", "install-native", ActionArgs::default()).unwrap();
    let r = actions.press(req, Origin::Manual).await.unwrap_err();
    assert!(r.why.contains("3.63.0"), "{r}");
    assert_eq!(
        ActionKind::InstallNative.scope(),
        homelab_proto::Scope::Operate
    );
}

/// Item 2: the demo host exists only in a `--features demo-host` build, and
/// `make release` builds without it.
#[test]
fn parity_the_demo_host_is_not_in_a_release_build() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let cargo = std::fs::read_to_string(root.join("admin/Cargo.toml")).unwrap();
    let table: toml::Value = toml::from_str(&cargo).unwrap();
    let features = table["features"].as_table().unwrap();
    assert!(features.contains_key("demo-host"));
    let default = features
        .get("default")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !default.iter().any(|f| f.as_str() == Some("demo-host")),
        "demo-host must not be a default feature"
    );
    let make = std::fs::read_to_string(root.join("Makefile")).unwrap();
    let binaries = make
        .split("release-binaries:")
        .nth(1)
        .unwrap()
        .split("\n\n")
        .next()
        .unwrap();
    assert!(binaries.contains("-p homelab-admin"));
    assert!(!binaries.contains("demo-host"), "{binaries}");
    assert!(!binaries.contains("--all-features"), "{binaries}");
}

/// Kenny, 2026-09-29 ("Today seems to load, but nothing loads"): today and
/// the fleet check take ~90 s, past the 30 s request guard and near
/// Cloudflare's 100 s. A slow read runs once; each request waits a bounded
/// time and says 202 "still running" with the run's id; a second page joins
/// the run instead of asking the host again; the answer comes by id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parity_a_slow_read_answers_202_until_its_one_run_is_done() {
    use axum::http::StatusCode;
    use homelab_admin::shell::slow::SlowRead;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn parts(r: axum::response::Response) -> (StatusCode, serde_json::Value) {
        let status = r.status();
        let body = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    let read = SlowRead::new("today");
    let started = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let rx = Arc::new(Mutex::new(Some(rx)));
    let short = Duration::from_millis(50);
    let work = |started: Arc<AtomicUsize>,
                rx: Arc<Mutex<Option<tokio::sync::oneshot::Receiver<()>>>>| {
        move || {
            started.fetch_add(1, Ordering::SeqCst);
            let rx = rx.lock().unwrap().take();
            async move {
                if let Some(rx) = rx {
                    let _ = rx.await;
                }
                (StatusCode::OK, serde_json::json!({ "verdict": "fine" }))
            }
        }
    };

    let (s, b) = parts(
        read.read(None, short, work(started.clone(), rx.clone()))
            .await,
    )
    .await;
    assert_eq!(s, StatusCode::ACCEPTED, "{b}");
    assert_eq!(b["running"], true);
    let run = b["run"].as_u64().unwrap();

    // A second page joins the run on its way: the host is not asked again.
    let (s, b) = parts(
        read.read(None, short, work(started.clone(), rx.clone()))
            .await,
    )
    .await;
    assert_eq!((s, b["run"].as_u64()), (StatusCode::ACCEPTED, Some(run)));
    assert_eq!(started.load(Ordering::SeqCst), 1);

    // The run finishes while a page waits for it: that page gets the answer.
    let waiting = {
        let read = read.clone();
        let (started, rx) = (started.clone(), rx.clone());
        tokio::spawn(async move {
            read.read(Some(run), Duration::from_secs(5), work(started, rx))
                .await
        })
    };
    tx.send(()).unwrap();
    let (s, b) = parts(waiting.await.unwrap()).await;
    assert_eq!((s, b["verdict"].as_str()), (StatusCode::OK, Some("fine")));
    // Asked again by id, the same answer; an id it never had is 410 with
    // what, why and fix.
    let (s, _) = parts(
        read.read(Some(run), short, work(started.clone(), rx.clone()))
            .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, b) = parts(
        read.read(Some(run + 40), short, work(started.clone(), rx.clone()))
            .await,
    )
    .await;
    assert_eq!(s, StatusCode::GONE);
    assert!(
        b["what"].is_string() && b["why"].is_string() && b["fix"].is_string(),
        "{b}"
    );
    // Without an id after the run is done: a fresh run.
    let (s, b) = parts(
        read.read(
            None,
            Duration::from_secs(5),
            work(started.clone(), rx.clone()),
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(started.load(Ordering::SeqCst), 2);
}

/// slow-reads (Kenny, 2026-09-29, form "Trage pagina's", changed at 13:52:
/// "doe het als ik op die pagina kom"; no timer): the last result is kept.
/// Opening the page gets it at once with when it was read and the run now
/// reading again; a second page joins that run (never two at once); a
/// failed run does not replace the last good answer; every finished run is
/// announced on the live channel, and fetching it by id starts nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_reads_the_last_result_is_served_at_once_while_one_run_reads_again() {
    use axum::http::StatusCode;
    use homelab_admin::shell::slow::SlowRead;
    use std::sync::atomic::{AtomicUsize, Ordering};

    async fn parts(r: axum::response::Response) -> (StatusCode, serde_json::Value) {
        let status = r.status();
        let body = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    let live = Arc::new(Recorder::default());
    let read = SlowRead::announced("today", "today", live.clone());
    let started = Arc::new(AtomicUsize::new(0));
    // Each run answers with its own number, or fails when told to; it waits
    // for a go so a test can hold it open.
    type Gate = Arc<Mutex<Option<tokio::sync::oneshot::Receiver<bool>>>>;
    let work = |started: Arc<AtomicUsize>, gate: Gate| {
        move || {
            let n = started.fetch_add(1, Ordering::SeqCst) + 1;
            let rx = gate.lock().unwrap().take();
            async move {
                let ok = match rx {
                    Some(rx) => rx.await.unwrap_or(true),
                    None => true,
                };
                if ok {
                    (StatusCode::OK, serde_json::json!({ "n": n }))
                } else {
                    (
                        StatusCode::BAD_GATEWAY,
                        serde_json::json!({ "what": "today", "why": "down", "fix": "later" }),
                    )
                }
            }
        }
    };
    let short = Duration::from_millis(50);
    let long = Duration::from_secs(5);

    // Before any result: as before, the run and then its answer.
    let (s, b) = parts(
        read.read(None, long, work(started.clone(), Arc::default()))
            .await,
    )
    .await;
    assert_eq!((s, b["n"].as_u64()), (StatusCode::OK, Some(1)), "{b}");
    assert!(b["read_at"].as_u64().is_some(), "{b}");
    assert!(b.get("refreshing").is_none(), "{b}");

    // Opening the page again: run 1's answer AT ONCE (a short wait is
    // enough, nothing is awaited), and a new run reading again.
    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
    let gate: Gate = Arc::new(Mutex::new(Some(rx)));
    let t0 = std::time::Instant::now();
    let (s, b) = parts(
        read.read(None, long, work(started.clone(), gate.clone()))
            .await,
    )
    .await;
    assert!(t0.elapsed() < Duration::from_secs(1), "served at once");
    assert_eq!((s, b["n"].as_u64()), (StatusCode::OK, Some(1)), "{b}");
    assert!(b["read_at"].as_u64().is_some(), "{b}");
    let run = b["refreshing"]["run"]
        .as_u64()
        .expect("the run reading again");
    assert!(b["refreshing"]["started_at"].as_u64().is_some(), "{b}");
    assert_eq!(started.load(Ordering::SeqCst), 2);

    // A second page (or tab) while it runs: the same last answer, the same
    // run; the host is not asked a third time.
    let (_, b) = parts(
        read.read(None, short, work(started.clone(), gate.clone()))
            .await,
    )
    .await;
    assert_eq!(b["refreshing"]["run"].as_u64(), Some(run), "{b}");
    assert_eq!(started.load(Ordering::SeqCst), 2, "never two runs at once");

    // The run answers: its page gets it by id, and the live channel says so.
    let waiting = {
        let (read, started, gate) = (read.clone(), started.clone(), gate.clone());
        tokio::spawn(async move { read.read(Some(run), long, work(started, gate)).await })
    };
    tx.send(true).unwrap();
    let (s, b) = parts(waiting.await.unwrap()).await;
    assert_eq!((s, b["n"].as_u64()), (StatusCode::OK, Some(2)), "{b}");
    assert_eq!(b["read_run"].as_u64(), Some(run), "which run this is: {b}");
    until("run 2 announced", || live.events("slow_read").len() == 2).await;
    assert_eq!(
        live.events("slow_read")[1],
        serde_json::json!({ "read": "today", "run": run, "ok": true })
    );
    // Another tab hears the event and fetches that run by id: no new run.
    let (s, b) = parts(
        read.read(Some(run), short, work(started.clone(), Arc::default()))
            .await,
    )
    .await;
    assert_eq!((s, b["n"].as_u64()), (StatusCode::OK, Some(2)));
    assert_eq!(started.load(Ordering::SeqCst), 2);

    // A run that fails is its own answer, but the last good one stays first.
    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
    let gate: Gate = Arc::new(Mutex::new(Some(rx)));
    let (_, b) = parts(
        read.read(None, short, work(started.clone(), gate.clone()))
            .await,
    )
    .await;
    assert_eq!(b["n"].as_u64(), Some(2), "{b}");
    let failing = b["refreshing"]["run"].as_u64().unwrap();
    tx.send(false).unwrap();
    let (s, _) = parts(
        read.read(Some(failing), long, work(started.clone(), gate))
            .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    until("the failed run announced", || {
        live.events("slow_read").len() == 3
    })
    .await;
    assert_eq!(live.events("slow_read")[2]["ok"], false);
    let (s, b) = parts(
        read.read(None, short, work(started.clone(), Arc::default()))
            .await,
    )
    .await;
    assert_eq!((s, b["n"].as_u64()), (StatusCode::OK, Some(2)), "{b}");
    assert!(b["refreshing"]["run"].as_u64().is_some());
}
