//! Tests moved out of main.rs unchanged (host-modules); they exercise the
//! whole host through the crate root.

// restore-two-sources-1: "local" points the settings at the mounted
// local set (and drops its own local_dataset, so a safety copy does not
// copy the repository onto itself); none or "remote" leaves them alone;
// anything else, or "local" without a local set, is refused.
#[tokio::test]
async fn a_restore_source_picks_the_repository_set() {
    use homelab_core::executor::{CmdOutput, MockExecutor};
    use homelab_core::ops::backup::BackupCfg;
    let exec = MockExecutor::new();
    exec.respond_always(
        "zfs list -H -o mounted,mountpoint HDD4TB/restic",
        CmdOutput::ok("yes\t/HDD4TB/restic\n"),
    );
    let base = BackupCfg {
        local_dataset: Some("HDD4TB/restic".into()),
        ..BackupCfg::default()
    };
    let local = super::backup_cfg_for_source(&exec, base.clone(), Some("local"))
        .await
        .unwrap();
    assert_eq!(local.restic_base, "/HDD4TB/restic");
    assert!(local.local_dataset.is_none());
    let remote = super::backup_cfg_for_source(&exec, base.clone(), None)
        .await
        .unwrap();
    assert_eq!(remote.restic_base, base.restic_base);
    assert!(
        super::backup_cfg_for_source(&exec, base, Some("usb"))
            .await
            .is_err()
    );
    assert!(
        super::backup_cfg_for_source(&exec, BackupCfg::default(), Some("local"))
            .await
            .is_err()
    );
}

// backup-stall (correction 2026-10-22): a command whose status lines
// keep coming while nothing moves is stopped once the watch's limit
// passes; one whose progress keeps moving runs to its end.
#[tokio::test]
async fn a_stalled_streaming_command_is_stopped_and_a_moving_one_is_not() {
    use homelab_core::executor::{Cmd, Executor, Stall};
    let ticking = Cmd::new(
        "sh",
        &["-c", "while true; do echo tick; sleep 0.2; done"],
        60,
    )
    .stall(Stall::new(2));
    let started = std::time::Instant::now();
    let err = super::RealExecutor
        .run_streaming(&ticking, &|_l: &str| false)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            homelab_core::error::CoreError::Stalled { seconds: 2, .. }
        ),
        "{err:?}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(10));

    let stall = Stall::new(2);
    let moving = Cmd::new(
        "sh",
        &["-c", "for i in 1 2 3 4 5 6; do echo $i; sleep 0.7; done"],
        60,
    )
    .stall(stall.clone());
    let out = super::RealExecutor
        .run_streaming(&moving, &|_l: &str| {
            stall.progressed();
            false
        })
        .await
        .unwrap();
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout.lines().count(), 6);
}

/// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): the
/// Hello every client prints names the build, not only the version.
#[test]
fn fix_141_the_hello_names_the_build() {
    match super::hello() {
        homelab_proto::ServerMsg::Hello { version, build, .. } => {
            assert_eq!(version, env!("CARGO_PKG_VERSION"));
            assert_eq!(build.as_deref(), option_env!("HOMELAB_BUILD"));
            assert!(build.is_some_and(|b| !b.is_empty()));
        }
        other => panic!("not a Hello: {other:?}"),
    }
}

/// covers: fix-94, fix-156
///
/// The home address is read again after every gateway deploy that
/// finished; another stack's deploy and a failed gateway deploy leave it
/// alone. (Every night and at the host's start it is read regardless.)
#[test]
fn fix_94_the_home_address_follows_gateway_deploys() {
    use super::home_address_after_deploy;
    assert!(home_address_after_deploy(104, true, 104));
    assert!(!home_address_after_deploy(104, false, 104));
    assert!(!home_address_after_deploy(115, true, 104));
}

/// fix-51 (see REGISTER.md): concurrent writes of one path must all
/// succeed and leave one whole version behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fix_51_concurrent_writes_of_one_path_all_succeed_and_stay_whole() {
    use homelab_core::executor::Executor;
    let dir = std::env::temp_dir().join(format!("homelab-fix51-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.json").to_string_lossy().into_owned();
    for round in 0..20 {
        let writes = (0..8).map(|i| {
            let path = path.clone();
            tokio::spawn(async move {
                let body = format!("{}-{};", round, i).repeat(20_000);
                super::RealExecutor.write_file(&path, &body, 0o644).await
            })
        });
        for w in writes.collect::<Vec<_>>() {
            w.await
                .unwrap()
                .expect("every concurrent write of one path succeeds");
        }
        let got = std::fs::read_to_string(&path).unwrap();
        let piece = &got[..=got.find(';').expect("content present")];
        assert_eq!(got, piece.repeat(20_000), "one writer's content, whole");
    }
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "state.json")
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(leftovers.is_empty(), "no temp files left: {:?}", leftovers);
}

/// fix-52 (see REGISTER.md): a dead scheduler must end the process with
/// a failure, so systemd restarts it with a live one.
#[tokio::test]
async fn fix_52_a_dead_scheduler_ends_the_daemon_with_a_failure() {
    let scheduler = tokio::spawn(async { panic!("scheduler bug") });
    let code = tokio::time::timeout(
        Duration::from_secs(5),
        super::supervise(
            std::future::pending::<std::io::Result<()>>(),
            scheduler,
            std::future::pending::<()>(),
            Arc::new(Mutex::new(())),
            Duration::from_secs(1),
        ),
    )
    .await
    .expect("supervision ends when the scheduler dies");
    assert_ne!(code, 0, "and it ends as a failure");
}

/// fix-52 and adoption norm N1: SIGTERM lets the running operation finish
/// (up to a bound) and then exits 0, instead of killing a step mid-way.
#[tokio::test]
async fn fix_52_sigterm_waits_for_the_running_operation_then_exits_zero() {
    let op_lock = Arc::new(Mutex::new(()));
    let held = op_lock.clone().lock_owned().await;
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let f2 = finished.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        f2.store(true, std::sync::atomic::Ordering::SeqCst);
        drop(held);
    });
    let scheduler = tokio::spawn(std::future::pending::<()>());
    let code = tokio::time::timeout(
        Duration::from_secs(5),
        super::supervise(
            std::future::pending::<std::io::Result<()>>(),
            scheduler,
            std::future::ready(()),
            op_lock.clone(),
            Duration::from_secs(3),
        ),
    )
    .await
    .expect("supervision ends on SIGTERM");
    assert_eq!(code, 0);
    assert!(
        finished.load(std::sync::atomic::Ordering::SeqCst),
        "it waited for the operation holding the lock"
    );
    // Having waited, it keeps the lock: nothing queued after the signal
    // starts before the process exits.
    assert!(op_lock.try_lock().is_err(), "the lock stays held");
    // A hung operation does not hold the exit forever.
    let op_lock = Arc::new(Mutex::new(()));
    let _stuck = op_lock.clone().lock_owned().await;
    let scheduler = tokio::spawn(std::future::pending::<()>());
    let code = tokio::time::timeout(
        Duration::from_secs(5),
        super::supervise(
            std::future::pending::<std::io::Result<()>>(),
            scheduler,
            std::future::ready(()),
            op_lock,
            Duration::from_millis(100),
        ),
    )
    .await
    .expect("the drain wait is bounded");
    assert_eq!(code, 0);
}

/// fix-52 residual (2026-10-01): the 10 s ping only ever proved that its
/// own loop was still scheduled, not that the scheduler was making any
/// progress — a nightly round wedged inside one `await` (not a panic,
/// which the test above already covers) kept being fed forever. Now the
/// ping is withheld once the scheduler's heartbeat is older than
/// `SCHEDULER_WATCHDOG_STALE_S`, so systemd's `WatchdogSec` eventually
/// restarts a daemon whose scheduler stopped making progress.
#[test]
fn fix_52_residual_a_stale_scheduler_heartbeat_withholds_the_watchdog_ping() {
    let stale_after = super::SCHEDULER_WATCHDOG_STALE_S;
    // A heartbeat from a few seconds ago: still alive.
    assert!(super::scheduler_is_alive(1_000_000, 999_990, stale_after));
    // Exactly at the edge still counts.
    assert!(super::scheduler_is_alive(
        1_000_000,
        1_000_000 - stale_after,
        stale_after
    ));
    // One second past the edge: the scheduler has made no progress in
    // longer than the tolerance, so the ping must stop.
    assert!(!super::scheduler_is_alive(
        1_000_000,
        1_000_000 - stale_after - 1,
        stale_after
    ));
    // A heartbeat from "the future" (clock skew on a fresh AppState,
    // or `now` ticking backwards) never reads as stale.
    assert!(super::scheduler_is_alive(1_000_000, 1_000_100, stale_after));
}

/// fix-53 (expert panel, timeout-leaves-container-work-running,
/// 2026-09-27): a timeout killed only the direct child (`pct`), not what
/// it had started (`lxc-attach` and the script), so a "timed out" step
/// went on changing the container. Everything the command started must
/// be gone once the timeout is reported.
#[tokio::test]
async fn fix_53_a_timeout_kills_everything_the_command_started() {
    use homelab_core::executor::Executor;
    let pidfile = std::env::temp_dir().join(format!("homelab-fix53-{}.pid", std::process::id()));
    let _ = std::fs::remove_file(&pidfile);
    let script = format!("sleep 30 & echo $! > '{}'; wait", pidfile.display());
    let got = super::RealExecutor
        .run(&Cmd::new("sh", &["-c", &script], 1))
        .await;
    assert!(
        matches!(got, Err(CoreError::Timeout { .. })),
        "the wait ends as a timeout: {:?}",
        got
    );
    let pid = std::fs::read_to_string(&pidfile)
        .expect("the script recorded its child")
        .trim()
        .to_string();
    let _ = std::fs::remove_file(&pidfile);
    // An exit is not instant; a zombie awaiting its reaper counts as gone.
    let mut alive = true;
    for _ in 0..20 {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).unwrap_or_default();
        alive = !stat.is_empty() && !stat.contains(") Z ");
        if !alive {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if alive {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid])
            .status();
    }
    assert!(
        !alive,
        "the command's own child {} outlived the timeout",
        pid
    );
}

/// fix-55 (expert panel, unparseable-frame-logged-whole, 2026-09-27): a
/// frame the host could not parse was logged whole. A deploy frame carries
/// every `.env` value of its stack (`DeploySpec.env` is the secrets
/// channel) and a staging frame tens of MB of base64, so one version skew
/// put a stack's secrets, or a whole binary, into one pve journal line.
#[test]
fn fix_55_an_unparseable_frame_is_logged_without_its_body() {
    let secret = "hunter2-db-password";
    let frame = format!(
        r#"{{"id":7,"cmd":"deploy_stack","manifest":{{"vmid":"not-a-number"}},"env":{{"app":{{"DB_PASSWORD":"{}"}}}},"blob":"{}"}}"#,
        secret,
        "A".repeat(100_000)
    );
    let e = serde_json::from_str::<RpcRequest>(&frame).unwrap_err();
    let line = super::unparseable_frame_line(&e, &frame);
    assert!(!line.contains(secret), "{}", line);
    assert!(line.len() < 500, "{} bytes", line.len());
    assert!(line.contains("deploy_stack"), "names the method: {}", line);
    assert!(
        line.contains(&frame.len().to_string()),
        "and the size: {}",
        line
    );
    // serde's own message quotes the offending value; that value may be a
    // secret too, so only its position is kept.
    let e =
        serde_json::from_str::<RpcRequest>(r#"{"id":"s3cr3t-token","cmd":"ping"}"#).unwrap_err();
    let line = super::unparseable_frame_line(&e, r#"{"id":"s3cr3t-token","cmd":"ping"}"#);
    assert!(!line.contains("s3cr3t-token"), "{}", line);
    // The positive twin: still a real diagnostic line, not an empty one
    // that would also pass "no secret in it".
    assert!(!line.is_empty(), "the frame still gets a log line");
}

fn parsed(text: &str) -> (serde_json::Value, RpcRequest) {
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    let req: RpcRequest = serde_json::from_value(value.clone()).unwrap();
    (value, req)
}

/// covers: fix-211
///
/// (1) a mutating command carrying a field this host's own build has
/// no struct field for ("force_wipe" is not a field of
/// `set_stack_enabled`) is refused, naming that field's exact path,
/// before it ever reaches a handler.
#[test]
fn fix_211_a_field_the_host_does_not_know_is_refused_by_name() {
    let (value, req) = parsed(
        r#"{"id":1,"cmd":"set_stack_enabled","stack":"media","enabled":true,"force_wipe":true}"#,
    );
    let why = super::unknown_field_refusal(&value, &req).expect("refused");
    assert!(why.contains("force_wipe"), "{}", why);
    assert!(why.contains("release-update"), "{}", why);
}

/// covers: fix-211
///
/// (2) a mutating command whose JSON carries only fields this build
/// knows is never refused by this check.
#[test]
fn fix_211_a_command_with_only_known_fields_is_never_refused() {
    let (value, req) =
        parsed(r#"{"id":1,"cmd":"set_stack_enabled","stack":"media","enabled":true}"#);
    assert!(super::unknown_field_refusal(&value, &req).is_none());
}

/// covers: fix-211
///
/// A READ-ONLY command carrying a field this build does not know is
/// never refused: it changes nothing, so an unknown field in it
/// cannot repeat the 2026-08-31 incident.
#[test]
fn fix_211_a_read_only_command_is_never_refused_for_an_unknown_field() {
    let (value, req) = parsed(r#"{"id":1,"cmd":"status","mystery_field":true}"#);
    assert!(req.command.is_read_only());
    assert!(super::unknown_field_refusal(&value, &req).is_none());
}

/// Load a config from `raw` through a file of its own. Tests run as
/// parallel threads of one process, so neither a shared path nor
/// `HOMELAB_CONFIG` may be used (rust-code-hygiene, 2026-09-27: one test
/// set that variable and wrote a fixed /tmp path while others ran).
fn config_from_text(raw: &str) -> Config {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "homelab-host-test-{}-{}.toml",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, raw).unwrap();
    let cfg = load_config_from(path.display().to_string());
    let _ = std::fs::remove_file(&path);
    cfg
}

/// F8: host.toml may only ADD to the no-touch list. Assigning used to be
/// possible, which meant one line in a hand-edited file could drop VM 100
/// and VM 101 out of protection without a word.
#[test]
fn the_config_can_widen_the_no_touch_list_but_never_shrink_it() {
    let raw = "token = \"0123456789abcdef0123\"\nno_touch = [200, 201]\n";
    let cfg = config_from_text(raw);

    for compiled in homelab_core::safety::DEFAULT_NO_TOUCH {
        assert!(
            cfg.safety.no_touch.contains(compiled),
            "config dropped {} out of the no-touch list",
            compiled
        );
    }
    assert!(cfg.safety.no_touch.contains(&200));
    assert!(cfg.safety.no_touch.contains(&201));
}

/// fix-36: a vmid host.toml adds to the no-touch list is refused by
/// remote exec too, not only by the operations.
#[test]
fn fix_36_remote_exec_refuses_a_vmid_the_config_added_to_the_no_touch_list() {
    let raw = "token = \"0123456789abcdef0123\"\nexec_enabled = true\nno_touch = [200]\n";
    let cfg = config_from_text(raw);
    assert!(
        exec_allowed(&cfg, 200).is_err(),
        "vmid 200 is on the configured no-touch list; exec must refuse it"
    );
    assert!(exec_allowed(&cfg, 101).is_err());
    assert!(exec_allowed(&cfg, 150).is_ok());
}

/// fix-120 (expert panel, api-token-is-root, 2026-09-27): without the two
/// keys the host runs with what the fleet uses today, so the policy lands
/// without refusing a single existing deploy; with them it runs with
/// exactly what the file says.
#[test]
fn fix_120_host_policy_defaults_to_the_fleet_and_follows_host_toml() {
    let dir = std::env::temp_dir().join(format!("homelab-fix120-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bare = dir.join("bare.toml");
    std::fs::write(&bare, "token = \"0123456789abcdef0123\"\n").unwrap();
    let cfg = load_config_from(bare.to_string_lossy().into_owned());
    assert_eq!(
        cfg.safety.privileged_vmids.as_deref(),
        Some(homelab_core::safety::FLEET_PRIVILEGED_VMIDS)
    );
    assert_eq!(
        cfg.safety.data_mount_roots,
        Some(
            homelab_core::safety::FLEET_DATA_MOUNT_ROOTS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        )
    );
    let set = dir.join("set.toml");
    std::fs::write(
        &set,
        "token = \"0123456789abcdef0123\"\nprivileged_vmids = [106]\n\
         data_mount_roots = [\"/HDD18TB/media\"]\n",
    )
    .unwrap();
    let cfg = load_config_from(set.to_string_lossy().into_owned());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(cfg.safety.privileged_vmids, Some(vec![106]));
    assert_eq!(
        cfg.safety.data_mount_roots,
        Some(vec!["/HDD18TB/media".to_string()])
    );
}

/// fix-120: the token compare is the digest compare, and a wrong token of
/// any length is refused.
#[test]
fn fix_120_the_bearer_check_refuses_every_near_miss() {
    let token = "0123456789abcdef0123";
    assert!(bearer_ok(Some("Bearer 0123456789abcdef0123"), token));
    for bad in [
        "Bearer 0123456789abcdef012",
        "Bearer 0123456789abcdef01234",
        "Bearer 0123456789abcdef0124",
        "Bearer ",
        "bearer 0123456789abcdef0123",
        "",
    ] {
        assert!(!bearer_ok(Some(bad), token), "{:?}", bad);
    }
}

/// fix-120: `random_token` makes a 64-hex-char token and never repeats
/// (a weak source would have shown up as a collision across a thousand
/// draws).
#[test]
fn fix_120_random_token_is_64_hex_and_does_not_repeat() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..1000 {
        let t = random_token().unwrap();
        assert_eq!(t.len(), 64, "{}", t);
        assert!(t.bytes().all(|b| b.is_ascii_hexdigit()), "{}", t);
        assert!(seen.insert(t), "random_token repeated");
    }
}

/// fix-120: issuing a token adds exactly one `[[tokens]]` entry, whose
/// SHA-256 is of the plaintext handed in — never the plaintext itself.
#[test]
fn fix_120_issue_token_adds_one_entry_hashed_not_plain() {
    let (text, tokens) =
        issue_token_in("", "wsl", homelab_proto::Scope::Operate, "a-fresh-token").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, "wsl");
    assert_eq!(tokens[0].scope, homelab_proto::Scope::Operate);
    use sha2::{Digest, Sha256};
    let want: String = Sha256::digest(b"a-fresh-token")
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    assert_eq!(tokens[0].sha256, want);
    assert!(!text.contains("a-fresh-token"), "{}", text);
    assert!(text.contains(&want), "{}", text);
    // Issuing a second, differently-named token keeps the first.
    let (_, tokens2) =
        issue_token_in(&text, "ct120", homelab_proto::Scope::All, "another-token").unwrap();
    assert_eq!(tokens2.len(), 2);
}

/// fix-120: a duplicate name, an empty name and the reserved name
/// "legacy" are refused rather than silently applied.
#[test]
fn fix_120_issue_token_refuses_bad_names() {
    assert!(issue_token_in("", "", homelab_proto::Scope::Read, "x").is_err());
    assert!(issue_token_in("", "legacy", homelab_proto::Scope::Read, "x").is_err());
    let (text, _) = issue_token_in("", "wsl", homelab_proto::Scope::Read, "x").unwrap();
    assert!(issue_token_in(&text, "wsl", homelab_proto::Scope::All, "y").is_err());
}

/// fix-120: revoking removes only the named entry, and leaves the rest
/// untouched — the whole point of per-machine tokens is that revoking
/// one never affects another.
#[test]
fn fix_120_revoke_token_removes_only_that_one() {
    let (text, _) = issue_token_in("", "wsl", homelab_proto::Scope::Operate, "a").unwrap();
    let (text, _) = issue_token_in(&text, "ct120", homelab_proto::Scope::All, "b").unwrap();
    let (text, tokens) = revoke_token_in(&text, "wsl").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, "ct120");
    assert!(!text.contains("\"wsl\""), "{}", text);
    // The positive twin: the other token's entry is still there.
    assert!(text.contains("\"ct120\""), "{}", text);
}

/// fix-120: revoking a name that is not there, or "legacy" (the single
/// `token` key, not a `[[tokens]]` entry), is refused.
#[test]
fn fix_120_revoke_token_refuses_unknown_and_legacy() {
    assert!(revoke_token_in("", "legacy").is_err());
    assert!(revoke_token_in("", "nobody").is_err());
}

/// fix-126: an `https://` notify route with no pin configured is
/// refused at start, before any notification is ever attempted —
/// `http://` stays accepted (migration is stepwise).
#[test]
fn fix_126_an_https_notify_route_with_no_pin_is_refused_at_start() {
    let raw = "token = \"0123456789abcdef0123\"\nnotify_webhook = \"https://kyu.lan/x\"\n";
    let file: FileConfig = toml::from_str(raw).unwrap();
    let problems = startup_problems(&file);
    assert!(
        problems.iter().any(|p| p.contains("notify_tls_cert")),
        "{:?}",
        problems
    );

    let raw_http = "token = \"0123456789abcdef0123\"\nnotify_webhook = \"http://kyu.lan/x\"\n";
    let file_http: FileConfig = toml::from_str(raw_http).unwrap();
    assert!(
        startup_problems(&file_http)
            .iter()
            .all(|p| !p.contains("notify_tls_cert")),
        "plain http must not need a pin"
    );
}

/// fix-126: `pinned_cacert` hands back the path only when the file on
/// disk hashes to the configured fingerprint — a stale pin (the hub's
/// certificate was regenerated since) is refused rather than silently
/// trusting whatever is on disk now, and a plain `http://` url needs no
/// pin at all.
#[test]
fn fix_126_pinned_cacert_checks_the_file_against_the_fingerprint() {
    use sha2::{Digest, Sha256};
    let path = std::env::temp_dir().join(format!(
        "homelab-host-test-cert-{}-{}.pem",
        std::process::id(),
        line!()
    ));
    let pem = "-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----\n";
    std::fs::write(&path, pem).unwrap();
    let good: String = Sha256::digest(b"hello")
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    let raw = format!(
        "token = \"0123456789abcdef0123\"\nnotify_tls_cert = \"{}\"\nnotify_tls_fingerprint = \"{}\"\n",
        path.display(),
        good
    );
    let cfg = config_from_text(&raw);
    assert_eq!(
        pinned_cacert(&cfg, "https://kyu.lan/x").unwrap(),
        Some(path.display().to_string())
    );
    // http:// needs no pin at all, even with one configured.
    assert_eq!(pinned_cacert(&cfg, "http://kyu.lan/x").unwrap(), None);

    let raw_wrong = format!(
        "token = \"0123456789abcdef0123\"\nnotify_tls_cert = \"{}\"\nnotify_tls_fingerprint = \"deadbeef\"\n",
        path.display()
    );
    let cfg_wrong = config_from_text(&raw_wrong);
    assert!(pinned_cacert(&cfg_wrong, "https://kyu.lan/x").is_err());
    let _ = std::fs::remove_file(&path);
}

/// fix-143: no `cloudflare_token` is reported as "not configured", not
/// as a failure — the nightly round must never turn an unasked question
/// into a finding, same rule `prometheus_url`/`loki_url` already
/// follow.
#[tokio::test]
async fn fix_143_the_nightly_edge_check_reports_not_configured_without_a_token() {
    let dir = std::env::temp_dir().join(format!("homelab-fix143-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let raw = format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    );
    let cfg = config_from_text(&raw);
    let state_dir = cfg.state_dir.clone();
    let app = AppState::new(cfg, tokio::sync::broadcast::channel(16).0);
    let exec = RealExecutor;
    run_nightly_edge_check(&app, &exec, 1_800_000_000).await;
    let store = homelab_core::state::StateStore::new(&exec, &state_dir);
    let s = store.load().await.unwrap();
    assert_eq!(s.last_edge_check, 1_800_000_000);
    assert_eq!(s.last_edge_findings, 0);
    assert_eq!(
        s.last_edge_error.as_deref(),
        Some("not configured (no cloudflare_token in host.toml)")
    );
}

/// fix-120 (see REGISTER.md): every refusal is counted with the address
/// it came from, for `homelab doctor`.
#[tokio::test]
async fn fix_120_a_refused_connection_is_counted_with_its_peer() {
    let path = format!("/tmp/homelab-fix120-router-{}.toml", std::process::id());
    std::fs::write(&path, "token = \"0123456789abcdef0123\"\n").unwrap();
    let state = test_state(load_config_from(path.clone()));
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = app_router(state.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let refused = tokio_tungstenite::connect_async(format!("ws://{}/api/ws", addr)).await;
    assert!(refused.is_err(), "no token, no session");
    let seen = state.auth_failures.snapshot();
    assert_eq!(seen.count, 1, "the refusal is counted");
    assert!(
        seen.last_peer
            .as_deref()
            .unwrap_or("")
            .starts_with("127.0.0.1"),
        "and names where it came from: {:?}",
        seen.last_peer
    );
}

/// F186: the exact file that was live on 2026-09-02 (see REGISTER.md).
#[test]
fn misplaced_keys_are_reported_not_swallowed() {
    let raw = r#"
token = "0123456789abcdef0123"
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"

[[zfs_jobs]]
source = "HDD2TB"
target = "HDD18TB/replica/HDD2TB"

[registry_cache]
host = "10.10.10.17"
pull_timeout_secs = 180

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003

opnsense_url = "https://10.10.5.1"
opnsense_cred_file = "/var/lib/homelab/secrets/opnsense.cred"
"#;
    let t: toml::Table = toml::from_str(raw).unwrap();

    // Serde's own view: the file parses cleanly and says nothing. That
    // silence is what made the fault invisible, and it is still the
    // behaviour — which is why the check below has to exist. (The two
    // keys were removed from the daemon altogether on 2026-09-02, form
    // V4; the fixture stays as it was found, because the trap it
    // demonstrates belongs to TOML, not to those particular settings.)
    let _parsed: FileConfig = t.clone().try_into().unwrap();

    assert_eq!(
        unknown_keys(&t),
        vec![
            "registry_cache.upstreams[0].opnsense_cred_file".to_string(),
            "registry_cache.upstreams[0].opnsense_url".to_string(),
        ]
    );
}

/// The same settings written in the right place: nothing to report, and
/// the daemon actually gets them.
#[test]
fn correctly_placed_keys_are_clean_and_read() {
    let raw = r#"
token = "0123456789abcdef0123"
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"
tile_watch_source = "10.10.10.20"

[registry_cache]
host = "10.10.10.17"

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003
"#;
    let t: toml::Table = toml::from_str(raw).unwrap();
    assert!(unknown_keys(&t).is_empty(), "{:?}", unknown_keys(&t));

    let parsed: FileConfig = t.try_into().unwrap();
    assert!(parsed.metrics_targets_dir.is_some());
    assert!(parsed.tile_watch_source.is_some());
}

/// A plain typo is loud too — the direction that costs a warning rather
/// than a silence.
#[test]
fn a_typo_is_reported() {
    let t: toml::Table = toml::from_str("metrics_target_dir = \"/x\"\n").unwrap();
    assert_eq!(unknown_keys(&t), vec!["metrics_target_dir".to_string()]);
}

/// config-four-representations (see REGISTER.md fix-134): the known keys
/// are whatever the one struct reads.
#[test]
fn a_key_the_struct_does_not_read_is_reported_even_where_a_list_named_it() {
    let raw = "[[retention]]\nevery_days = 1\nspan_days = 7\nkeep = 3\n";
    let t: toml::Table = toml::from_str(raw).unwrap();
    assert_eq!(unknown_keys(&t), vec!["retention[0].keep".to_string()]);
}

/// config-four-representations (see REGISTER.md fix-134): a settings
/// save writes back the file it read, changing only what the settings
/// tab changed. Nested tables write their defaults out (`max_age_hours`,
/// `pull_timeout_secs`), so the fixture states them.
///
/// covers: F208
#[test]
fn a_settings_save_writes_back_the_file_it_read() {
    let raw = r#"
token = "0123456789abcdef0123"
listen = "0.0.0.0:8443"
backup_hour = 4
notify_webhook = "http://ha/webhook/x"
notify_auth_bearer = "a-token-that-must-survive-a-save"
notify_fallback_webhook = "http://10.10.5.101:8123/api/webhook/homelab"
exec_enabled = true
no_touch = [200]
gateway_vmid = 112
restic_base = "rclone:hdd:homelab-backups"
restic_restore_timeout_s = 9999
logs_window = "6h"
ask_timeout_s = 300
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"

[[retention]]
every_days = 1
span_days = 7

[[zfs_jobs]]
source = "HDD2TB"
target = "HDD18TB/replica/HDD2TB"

[[watched_backups]]
name = "opnsense-config"
rclone_path = "gdrive:homelab-backups/OPNSense-backups"
max_age_hours = 26

[[device_backups]]
name = "opnsense"
url = "https://10.10.10.1/api/core/backup/download/this"
cred_file = "/var/lib/homelab/secrets/opnsense-backup.conf"
filename = "config.xml"
pin = "sha256//abc"

[registry_cache]
host = "10.10.10.17"
pull_timeout_secs = 180

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003
"#;
    let config = config_from_text(raw);

    let mut settings = config.initial_settings.clone();
    settings.backup_hour = Some(5);
    let rendered = render_settings_toml(&config, &settings).expect("render");

    let mut want: toml::Table = toml::from_str(raw).unwrap();
    want.insert("backup_hour".into(), toml::Value::Integer(5));
    let got: toml::Table = toml::from_str(&rendered).unwrap();
    assert_eq!(got, want, "rendered:\n{rendered}");
}

/// fix-134 (see REGISTER.md): `config_from_text` deletes its temp file
/// the moment `load_config_from` returns, so it cannot prove anything
/// about a later read of the same path; this test keeps the file on
/// disk across the save, the way a real settings save finds it.
///
/// covers: fix-134
#[test]
fn fix_134_a_settings_save_keeps_the_file_s_own_comments() {
    let raw = "\
# the token nobody is allowed to rotate without telling Kenny first\n\
token = \"0123456789abcdef0123\"\n\
# backed up at 4am because that is when the drive is quietest\n\
backup_hour = 4\n\
\n\
[[retention]]\n\
every_days = 1\n\
span_days = 7\n";

    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "homelab-host-test-fix134-{}-{}.toml",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, raw).unwrap();
    let config = load_config_from(path.display().to_string());

    let mut settings = config.initial_settings.clone();
    settings.backup_hour = Some(5);
    let rendered = render_settings_toml(&config, &settings).expect("render");
    let _ = std::fs::remove_file(&path);

    assert!(
        rendered.contains("# the token nobody is allowed to rotate without telling Kenny first"),
        "a comment on an untouched key must survive a save:\n{rendered}"
    );
    assert!(
        rendered.contains("# backed up at 4am because that is when the drive is quietest"),
        "a comment on the CHANGED key must survive too — only the value moves:\n{rendered}"
    );
    // And the value changed as asked.
    let got: toml::Table = toml::from_str(&rendered).unwrap();
    assert_eq!(got["backup_hour"].as_integer(), Some(5));
}

/// The dashboard's key table and the file the host reads cannot drift:
/// every field of `FileConfig` has a row (a new field without one fails
/// here), and every row names a field.
#[test]
fn feat_settings_1_every_host_toml_key_is_described_once() {
    let json = serde_json::to_value(FileConfig::default()).unwrap();
    let mut fields: Vec<String> = json.as_object().unwrap().keys().cloned().collect();
    // Skipped when absent, so it is not in the default's JSON.
    fields.push("tokens".into());
    fields.sort();
    let mut rows: Vec<String> = homelab_core::hostconfig::KEYS
        .iter()
        .map(|k| k.key.to_string())
        .collect();
    rows.sort();
    assert_eq!(fields, rows);
}

/// GetHostConfig: every key the file sets, a secret only as "set", a
/// scoped token without its hash, the keys the host does not read, and
/// the file's hash for the write that follows.
#[test]
fn feat_settings_1_the_view_hides_secrets_and_hashes() {
    let raw = "token = \"0123456789abcdef0123\"\nbackup_hour = 4\nnotify_auth_bearer = \"s3cret-bearer\"\n\
               loki_url = \"http://10.10.10.13:3100\"\nbogus_key = 1\n\
               [[tokens]]\nname = \"admin\"\nscope = \"all\"\nsha256 = \"{h}\"\n"
        .replace("{h}", &"a".repeat(64));
    let v = host_config_view("/etc/homelab/host.toml", &raw).unwrap();
    let json = serde_json::to_string(&v).unwrap();
    assert!(!json.contains("0123456789abcdef0123"), "{json}");
    assert!(!json.contains("s3cret-bearer"), "{json}");
    assert!(!json.contains(&"a".repeat(64)), "{json}");
    assert_eq!(v.values["backup_hour"], 4);
    assert_eq!(v.values["tokens"][0]["name"], "admin");
    assert_eq!(v.secrets_set, vec!["notify_auth_bearer", "token"]);
    assert_eq!(v.unknown, vec!["bogus_key"]);
    assert_eq!(v.sha256, homelab_core::manifest::sha256_hex(raw.as_bytes()));
}

/// fix-false-alarm (2026-09-30): what to check before a `watch_url`
/// save is accepted — absent or empty needs no request (turning the
/// watch off is always allowed), a non-empty value does.
#[test]
fn fix_false_alarm_watch_url_to_check() {
    use std::collections::BTreeMap;
    assert_eq!(super::watch_url_to_check(&BTreeMap::new()), None);
    assert_eq!(
        super::watch_url_to_check(&BTreeMap::from([(
            "watch_url".to_string(),
            serde_json::json!("")
        )])),
        None
    );
    assert_eq!(
        super::watch_url_to_check(&BTreeMap::from([(
            "watch_url".to_string(),
            serde_json::json!("  ")
        )])),
        None
    );
    assert_eq!(
        super::watch_url_to_check(&BTreeMap::from([(
            "backup_hour".to_string(),
            serde_json::json!(4)
        )])),
        None,
        "a save of some other key must not run the probe"
    );
    assert_eq!(
        super::watch_url_to_check(&BTreeMap::from([(
            "watch_url".to_string(),
            serde_json::json!("http://10.10.10.20:8080/health")
        )])),
        Some("http://10.10.10.20:8080/health".to_string())
    );
}

/// fix-false-alarm: the save is refused with a reason a person can act
/// on when the host's own request failed, and let through otherwise.
#[test]
fn fix_false_alarm_watch_url_refusal() {
    assert_eq!(
        super::watch_url_refusal("http://10.10.10.20:8080/health", true),
        None
    );
    let why = super::watch_url_refusal("http://10.10.10.20:8080/health", false)
        .expect("a failed request refuses the save");
    assert!(why.contains("10.10.10.20:8080/health"), "{why}");
    assert!(why.contains("firewall"), "{why}");
}

/// fix-184: the down notice must wait for the full `down_after_s` of
/// unbroken failure; a single failed probe (elapsed 0, or anything short
/// of the threshold) must not declare down.
#[test]
fn fix_184_down_notice_waits_for_the_full_threshold() {
    // One failed probe, measured the instant it failed: elapsed is 0.
    assert!(!super::watch_should_declare_down(0, 300));
    // Still short of five minutes.
    assert!(!super::watch_should_declare_down(299, 300));
    // Exactly, and past, the threshold.
    assert!(super::watch_should_declare_down(300, 300));
    assert!(super::watch_should_declare_down(301, 300));
}

/// fix-184: a planned operation on the stack `watch_url` is declared for
/// suppresses the down timer; an operation on any other stack, or no
/// operation at all, does not — and the check never hardcodes which
/// stack that is, it only compares against `watch_stack`.
#[test]
fn fix_184_outage_expected_is_declared_not_hardcoded() {
    let holder_for = |stack: &str| homelab_core::oplock::Holder {
        what: "deploy".to_string(),
        started_unix: 0,
        done: 0,
        total: 0,
        stack: Some(stack.to_string()),
    };
    // Nothing running: never a known outage.
    assert!(!super::outage_expected_for(None, Some("dashboard-stack")));
    // The declared stack is deploying: suppressed.
    let busy = holder_for("dashboard-stack");
    assert!(super::outage_expected_for(
        Some(&busy),
        Some("dashboard-stack")
    ));
    // Some unrelated stack is deploying: not suppressed — this is the
    // generic form a hardcoded `"admin"` literal could never express for
    // an arbitrary configured name.
    let other = holder_for("some-other-stack");
    assert!(!super::outage_expected_for(
        Some(&other),
        Some("dashboard-stack")
    ));
    // No watch_stack declared at all: a stack-scoped op never suppresses
    // (only a host-wide self-update/restart does).
    assert!(!super::outage_expected_for(Some(&busy), None));
    // The host's own update or restart always suppresses, regardless of
    // watch_stack.
    let self_update = homelab_core::oplock::Holder {
        what: "self-update".to_string(),
        started_unix: 0,
        done: 0,
        total: 0,
        stack: None,
    };
    assert!(super::outage_expected_for(Some(&self_update), None));
    assert!(super::outage_expected_for(
        Some(&self_update),
        Some("dashboard-stack")
    ));
}

/// fix-184: the recovery notice is urgent (reaches `notify_webhook`)
/// only when the outage it closes was itself pushed out; a recovery
/// after a down notice that stayed centre-only (too short-lived, or
/// damped) must not suddenly become the first urgent push of the pair.
#[test]
fn fix_184_recovery_is_urgent_only_if_the_outage_was_pushed() {
    let down = super::watch_facts(false, 0, "http://10.10.10.20/health", false);
    assert!(down.urgency.urgent, "going down is always urgent");

    let recovery_after_push = super::watch_facts(true, 0, "http://10.10.10.20/health", true);
    assert!(
        recovery_after_push.urgency.urgent,
        "a recovery closing a pushed outage may be pushed"
    );

    let recovery_without_push = super::watch_facts(true, 0, "http://10.10.10.20/health", false);
    assert!(
        !recovery_without_push.urgency.urgent,
        "a recovery after an outage nobody was pushed about stays centre-only"
    );
}

/// fix-191: a whole-file apply may move only the keys it names. The
/// dashboard's Settings save names the one key it edited; a stale
/// `config/host.toml` that lacks `zfs_jobs` must then be refused, not
/// allowed to drop the host's replication.
#[test]
fn fix_191_a_whole_file_apply_moves_only_the_named_keys() {
    let raw = "token = \"0123456789abcdef0123\"\nbackup_hour = 4\nexec_enabled = false\n\
               zfs_jobs = [{ source = \"HDD2TB\", target = \"HDD18TB/replica/HDD2TB\" }]\n";
    let stale = "backup_hour = 5\nexec_enabled = false\n";
    let err = apply_host_config_whole(raw, stale, None, &["backup_hour".to_string()]).unwrap_err();
    assert!(err.contains("zfs_jobs"), "{err}");
    let err = apply_host_config_whole(raw, stale, None, &[]).unwrap_err();
    assert!(
        err.contains("backup_hour") && err.contains("zfs_jobs"),
        "{err}"
    );
    // Named: it goes through, and the secret token is kept.
    let (text, _) = apply_host_config_whole(
        raw,
        stale,
        None,
        &["backup_hour".to_string(), "zfs_jobs".to_string()],
    )
    .unwrap();
    assert!(
        text.contains("backup_hour = 5") && !text.contains("zfs_jobs"),
        "{text}"
    );
    assert!(text.contains("0123456789abcdef0123"));
}

/// SetHostConfig: a change lands with every other key kept; a file that
/// moved meanwhile, a locked or ssh-only key, a secret, a stray sub-key
/// and a value the start-up validation refuses are each refused before
/// anything is written.
#[test]
fn feat_settings_1_a_change_is_checked_like_a_start() {
    use std::collections::BTreeMap;
    let raw = "token = \"0123456789abcdef0123\"\nbackup_hour = 4\nexec_enabled = false\n";
    let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
    let one = |k: &str, v: serde_json::Value| BTreeMap::from([(k.to_string(), v)]);

    let (text, saved) = apply_host_config_changes(
        raw,
        &BTreeMap::from([
            ("backup_hour".to_string(), serde_json::json!(5)),
            ("status_interval_s".to_string(), serde_json::json!(30)),
        ]),
        &sha,
    )
    .unwrap();
    let t: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(t["backup_hour"].as_integer(), Some(5));
    assert_eq!(t["status_interval_s"].as_integer(), Some(30));
    assert_eq!(t["token"].as_str(), Some("0123456789abcdef0123"));
    assert_eq!(saved.live, vec!["backup_hour"]);
    assert_eq!(saved.restart, vec!["status_interval_s"]);
    assert_eq!(
        saved.sha256,
        homelab_core::manifest::sha256_hex(text.as_bytes())
    );

    // null removes the key: the host takes its default.
    let (text, _) =
        apply_host_config_changes(raw, &one("backup_hour", serde_json::Value::Null), &sha).unwrap();
    assert!(!text.contains("backup_hour"), "{text}");
    // The positive twin: the rest of the file is still there, not an
    // empty file that would also pass "no backup_hour".
    assert!(text.contains("token"), "{text}");

    let refused = |changes: BTreeMap<String, serde_json::Value>, expect: &str| {
        apply_host_config_changes(raw, &changes, expect).unwrap_err()
    };
    assert!(refused(one("backup_hour", serde_json::json!(5)), "stale").contains("changed since"));
    for key in [
        "listen",
        "state_dir",
        "tokens",
        "token",
        "exec_enabled",
        "no_touch",
    ] {
        let e = refused(one(key, serde_json::json!("x")), &sha);
        assert!(e.contains("ssh"), "{key}: {e}");
    }
    assert!(refused(one("backup_hour", serde_json::json!(24)), &sha).contains("0 to 23"));
    assert!(refused(one("status_interval_s", serde_json::json!(5)), &sha).contains("10"));
    let e = refused(
        one(
            "registry_cache",
            serde_json::json!({"host": "10.10.10.17", "upstreams": [], "hots": 1}),
        ),
        &sha,
    );
    assert!(e.contains("registry_cache.hots"), "{e}");
    let e = refused(
        one(
            "zfs_jobs",
            serde_json::json!([{"source": "HDD2TB", "target": "HDD2TB"}]),
        ),
        &sha,
    );
    assert!(e.contains("zfs_jobs"), "{e}");
    assert!(refused(one("nonsense", serde_json::json!(1)), &sha).contains("not a setting"));
}

/// fix-122 (AR15's runtime debug toggle, Kenny's go 2026-10-01): a
/// directive `EnvFilter` cannot parse is refused the same way an
/// out-of-range `backup_hour` is — at the save, not discovered on the
/// next restart when the daemon refuses to start at all.
#[test]
fn fix_122_an_unparsable_log_level_is_refused() {
    let raw = "token = \"0123456789abcdef0123\"\n";
    let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
    let changes = std::collections::BTreeMap::from([(
        "log_level".to_string(),
        serde_json::json!("not a directive!!"),
    )]);
    let e = apply_host_config_changes(raw, &changes, &sha).unwrap_err();
    assert!(e.contains("log_level"), "{e}");
    assert!(e.contains("EnvFilter"), "{e}");
}

/// fix-122: a valid directive saves and is live (G8), the same way
/// `backup_hour` is — `saved.live` names it, not `saved.restart`.
#[test]
fn fix_122_a_valid_log_level_saves_as_a_live_key() {
    let raw = "token = \"0123456789abcdef0123\"\n";
    let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
    let changes =
        std::collections::BTreeMap::from([("log_level".to_string(), serde_json::json!("debug"))]);
    let (text, saved) = apply_host_config_changes(raw, &changes, &sha).unwrap();
    let t: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(t["log_level"].as_str(), Some("debug"));
    assert_eq!(saved.live, vec!["log_level"]);
}

use super::*;

/// H8: the probe layer feeds real data — stale backups and a dead
/// offsite remote must surface, healthy state must not.
///
/// fix-218: the backup-age reading now comes from the host's
/// `SnapshotCache` (the same one `homelab snapshots` answers from), not
/// `StackState::last_backup` — so this test deliberately sets
/// `last_backup` to `now` (the live fault's exact shape: a cached
/// timestamp that reads as fresh) and proves the age comes out stale
/// anyway, from the repository's own newest snapshot.
#[tokio::test]
async fn doctor_probes_surface_stale_backup_and_dead_offsite() {
    use homelab_core::executor::{CmdOutput, MockExecutor};
    let now = 1_800_000_000u64;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &format!(
            r#"{{"schema_version":1,"stacks":{{"synctest":{{"vmid":108,"hostname":"108-app-synctest","apps":["syncthing"],"applied_at":1,"last_backup":{now},"natives":[{{"stack_name":"synctest","vmid":108,"hostname":"108-app-synctest","unit":"synctest","binary":"/opt/synctest/bin/synctest","data_dirs":["/var/lib/synctest"]}}]}}}}}}"#,
        ),
    );
    // Present = Proxmox holds a configuration for it (what `pct status`
    // answered), read off pmxcfs.
    exec.seed_file("/etc/pve/lxc/108.conf", "hostname: 108-app-synctest\n");
    exec.respond_always(
        "listremotes",
        CmdOutput::ok(
            "gdrive:
",
        ),
    );
    exec.respond_always(
        "lsd gdrive:homelab-backups",
        CmdOutput::failed(3, "token expired"),
    );
    // The repository's own newest snapshot is 80h old — stale, even
    // though `last_backup` above claims `now`.
    exec.respond_always(
        "snapshots --json",
        CmdOutput::ok(
            r#"[{"time":"2023-11-11T14:13:20Z","short_id":"ab12cd34","id":"ab12cd34ef"}]"#,
        ),
    );
    let snapshot_cache =
        std::sync::Arc::new(homelab_core::ops::snapshot_cache::SnapshotCache::new(3));
    let backup_cfg = homelab_core::ops::backup::BackupCfg::default();
    // 2023-11-11T14:13:20Z is exactly 80h before 1_700_000_000; `now`
    // here is unrelated to that epoch, so the cache is seeded directly
    // with the same reading `gather_probes` would otherwise have to kick
    // a background refresh for.
    snapshot_cache
        .refresh(&exec, &backup_cfg, "synctest", 1_700_000_000)
        .await;
    let probes = gather_probes(
        &exec,
        "/var/lib/homelab",
        None,
        None,
        homelab_core::ops::restoredrill::DEFAULT_DRILL_SCRATCH_DIR,
        1_700_000_000,
        &snapshot_cache,
        &backup_cfg,
        &|_| {},
    )
    .await;
    assert_eq!(probes.managed_stacks.len(), 1);
    assert!(
        probes.managed_stacks[0].snapshot_read,
        "the cache was seeded before gather_probes ran"
    );
    assert_eq!(probes.managed_stacks[0].snapshot_age_h, Some(80));
    assert!(probes.managed_stacks[0].container_present);
    assert!(probes.offsite_configured);
    assert!(!probes.offsite_token_valid, "expired token must show");
    // And the diagnosis flags both problems — a fresh `last_backup` must
    // not paper over the stale real snapshot (the live fault, fix-218).
    let checks = homelab_core::doctor::diagnose(&probes);
    assert!(
        checks
            .iter()
            .any(|c| c.health != homelab_core::doctor::Health::Ok)
    );
    let backup_line = checks
        .iter()
        .find(|c| c.name == "stack synctest backup")
        .expect("a stack synctest backup line");
    assert_eq!(backup_line.health, homelab_core::doctor::Health::Warn);
    assert!(
        backup_line.detail.contains("80h ago"),
        "{}",
        backup_line.detail
    );
}

/// The daemon's shared state around `config`, as `main` builds it.
fn test_state(config: Config) -> AppState {
    let (log_tx, _) = broadcast::channel(64);
    AppState::new(config, log_tx)
}

/// fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): the new
/// doctor probes read the machine: file modes, privileged containers
/// (never a no-touch guest's configuration), the host-meta and drill
/// records in state, the password file and Drive's space.
#[tokio::test]
async fn fix_130_the_new_doctor_probes_read_the_machine() {
    use homelab_core::executor::{CmdOutput, MockExecutor};
    let now = 1_800_000_000u64;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &format!(
            r#"{{"schema_version":1,"stacks":{{}},"last_host_meta":{},
            "last_restore_drill":{},"restore_drills":{{"kyu-config":{{"last_attempt":{},"last_pass":0,"last_error":"the restore itself failed"}}}}}}"#,
            now - 30 * 3600,
            now - 10 * 86_400,
            now - 86_400
        ),
    );
    exec.respond_always(
        "stat -c",
        CmdOutput::ok("644 /etc/homelab/host.toml\n600 /var/lib/homelab/tls-key.pem\n700 /var/lib/homelab/incidents\n"),
    );
    exec.respond_always("unprivileged: 1", CmdOutput::ok("105\n106\n108\n"));
    exec.respond_always("test -s", CmdOutput::ok(""));
    exec.respond_always("listremotes", CmdOutput::ok("gdrive:\n"));
    exec.respond_always(
        "about gdrive:",
        CmdOutput::ok(
            r#"{"total":107374182400,"used":90000000000,"trashed":2147483648,"free":17374182400}"#,
        ),
    );
    exec.respond_always(
        "ls -1A '/opt/traefik-config/routes'",
        CmdOutput::ok("104-app-gateway.yml\nmanual-leftover.yml\n"),
    );
    let pc = ProbeContext {
        listen: "0.0.0.0:8443".into(),
        exec_enabled: false,
        config_path: "/etc/homelab/host.toml".into(),
        password_file: "/var/lib/homelab/secrets/restic.pass".into(),
        privileged_vmids: vec![105, 106],
        no_touch: vec![100, 101, 102, 103],
        drill_interval_s: 90 * 86_400,
        state_dir: "/var/lib/homelab".into(),
        gateway_vmid: 104,
        gateway_routes_dir: "/opt/traefik-config/routes".into(),
    };
    // As `gather_probes` leaves it when the gdrive remote exists.
    let mut probes = homelab_core::doctor::Probes {
        offsite_configured: true,
        ..Default::default()
    };
    gather_security_probes(&exec, &pc, now, &mut probes).await;
    assert_eq!(
        probes.loose_files,
        Some(vec!["/etc/homelab/host.toml (644)".to_string()])
    );
    let pv = probes.privileged.expect("privileged probed");
    assert_eq!(
        (pv.vmids, pv.outside_policy),
        (vec![105, 106, 108], vec![108])
    );
    let listing = exec.calls_containing("unprivileged: 1");
    assert!(
        listing.iter().all(|c| c.contains("100|101|102|103")),
        "no-touch configurations are skipped, not read: {:?}",
        listing
    );
    assert_eq!(probes.host_meta.and_then(|h| h.age_h), Some(30));
    let drill = probes.restore_drill.expect("drill probed");
    assert_eq!(drill.age_h, Some(240));
    assert!(
        drill.failing[0].starts_with("kyu-config"),
        "{:?}",
        drill.failing
    );
    assert_eq!(probes.password_file_ok, Some(true));
    let drive = probes.drive.expect("drive probed");
    assert_eq!((drive.total, drive.trashed), (107374182400, 2147483648));
    assert_eq!(
        probes.exposure.map(|e| e.listen),
        Some("0.0.0.0:8443".into())
    );
    // fix-130 (second half): no stack is recorded in this state, so
    // every file the gateway lists is unowned.
    assert_eq!(
        probes.unowned_route_files,
        Some(vec![
            "104-app-gateway.yml".to_string(),
            "manual-leftover.yml".to_string()
        ])
    );
}

/// fix-131 (expert panel, orchestrator-logs-only-on-pve, 2026-09-27):
/// `homelab incidents` listed names only; reading a bundle took a root
/// shell on pve. `incidents show <name>` brings the error, the versions
/// and the end of the transcript to the workstation, and a name that is
/// not a plain bundle name is refused before it becomes a path.
#[tokio::test]
async fn fix_131_incident_show_reads_one_bundle_and_nothing_else() {
    let dir = std::env::temp_dir().join(format!("homelab-fix131-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bundle = dir.join("incidents/1800000000-deploy-media");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(
        bundle.join("report.json"),
        r#"{"op":"deploy-media","steps":[],"ok":false,"error":{"what":"pull images failed","why":"image not found","remedy":"check the tag"}}"#,
    )
    .unwrap();
    std::fs::write(bundle.join("versions.txt"), "host=3.60.0\nproto=1\n").unwrap();
    // Written the way `write_bundle` writes them.
    let line = |m: &str| {
        let ev = PipelineEvent::Line {
            level: homelab_core::sink::Level::Info,
            source: "HOST".into(),
            msg: m.into(),
        };
        format!("{}\n", serde_json::to_string(&ev).unwrap())
    };
    std::fs::write(
        bundle.join("events.jsonl"),
        format!(
            "{}{}",
            line("[run ] docker compose pull"),
            line("image not found")
        ),
    )
    .unwrap();
    let toml = dir.join("host.toml");
    std::fs::write(
        &toml,
        format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        ),
    )
    .unwrap();
    let state = test_state(load_config_from(toml.to_string_lossy().into_owned()));
    let show = |name: &str| RpcRequest {
        id: 3,
        command: Rpc::IncidentShow { name: name.into() },
    };
    let got = handle_rpc(&state, show("1800000000-deploy-media")).await;
    let refused = handle_rpc(&state, show("../../etc")).await;
    let missing = handle_rpc(&state, show("1800000001-deploy-x")).await;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(got.ok, "{}", got.message);
    for want in [
        "pull images failed",
        "image not found",
        "check the tag",
        "host=3.60.0",
        "docker compose pull",
    ] {
        assert!(
            got.message.contains(want),
            "{} missing from:\n{}",
            want,
            got.message
        );
    }
    assert!(
        !refused.ok && refused.message.contains("not a bundle name"),
        "{}",
        refused.message
    );
    assert!(!missing.ok, "{}", missing.message);
}

/// fix-131: the pruner removes what `bundles_to_prune` names, on disk.
#[test]
fn fix_131_old_bundles_are_removed_from_disk() {
    let dir = std::env::temp_dir().join(format!("homelab-fix131p-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let now = 1_800_000_000u64;
    let old = dir.join(format!("incidents/{}-deploy-media", now - 100 * 86_400));
    let young = dir.join(format!("incidents/{}-deploy-kyu", now - 86_400));
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&young).unwrap();
    std::fs::write(old.join("report.json"), "{}").unwrap();
    let removed = prune_incidents(
        &dir.to_string_lossy(),
        now,
        homelab_core::incidents::BUNDLE_MAX_AGE_DAYS,
        homelab_core::incidents::BUNDLE_MAX_COUNT,
    );
    let (old_gone, young_kept) = (!old.exists(), young.exists());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(removed, 1);
    assert!(old_gone && young_kept);
}

/// rule-20: `cleanup_push_staging` removes a `push-staging-*` file it
/// finds on disk (the age judgement itself is
/// `util::stale_push_staging_tests`, pure) and leaves anything else in
/// the state dir alone.
#[test]
fn rule_20_orphaned_push_staging_files_are_removed_from_disk() {
    let dir = std::env::temp_dir().join(format!("homelab-rule20-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let staging = dir.join("push-staging-118-abcd1234");
    let other = dir.join("state.json");
    std::fs::write(&staging, "stale compose content").unwrap();
    std::fs::write(&other, "{}").unwrap();
    // max_age_s is baked into the call via `now`, well past any file's
    // mtime — the threshold itself is covered in core's pure tests.
    let removed = cleanup_push_staging(&dir.to_string_lossy(), u64::MAX / 2);
    let (staging_gone, other_kept) = (!staging.exists(), other.exists());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(removed, 1);
    assert!(staging_gone && other_kept);
}

/// gap-26: absent, the default (`incidents::JOURNAL_MAX_BYTES`) still
/// applies; set, host.toml wins. See REGISTER.md.
#[test]
fn gap_26_journal_max_bytes_defaults_and_follows_host_toml() {
    let cfg = config_from_text("token = \"0123456789abcdef0123\"\n");
    assert_eq!(
        cfg.journal_max_bytes,
        homelab_core::incidents::JOURNAL_MAX_BYTES as u64
    );
    let cfg = config_from_text("token = \"0123456789abcdef0123\"\njournal_max_bytes = 65536\n");
    assert_eq!(cfg.journal_max_bytes, 65536);
}

/// gap-26: `compact_journal_file` honours the configured limit rather
/// than the hardcoded default — a small `max_bytes` cuts a journal the
/// default would have left alone.
#[test]
fn gap_26_compact_journal_file_uses_the_configured_limit() {
    let dir = std::env::temp_dir().join(format!("homelab-gap26-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let line = "{\"op\":\"deploy\",\"at\":1,\"running\":false}\n";
    let content = line.repeat(2000); // well under JOURNAL_MAX_BYTES, well over 64 KiB
    assert!(content.len() < homelab_core::incidents::JOURNAL_MAX_BYTES);
    let path = dir.join("journal.jsonl");
    std::fs::write(&path, &content).unwrap();
    compact_journal_file(&dir.to_string_lossy(), 65536);
    let cut = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        cut.len() < content.len(),
        "a 64 KiB configured limit must cut a journal the 4 MiB default would not: \
         {} -> {}",
        content.len(),
        cut.len()
    );
}

/// A session over a real socket on a free local port, with `handler` in
/// place of `handle_rpc`. Returns the address to connect to.
async fn serve_on_loopback<H, Fut>(handler: H) -> SocketAddr
where
    H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    let config = config_from_text("token = \"0123456789abcdef0123\"\n");
    serve_state_on_loopback(test_state(config), handler).await
}

/// [`serve_on_loopback`] around a state the test built itself.
async fn serve_state_on_loopback<H, Fut>(state: AppState, handler: H) -> SocketAddr
where
    H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    let app =
        Router::new()
            .route(
                "/ws",
                get(
                    move |ws: WebSocketUpgrade,
                          State(st): State<AppState>,
                          headers: HeaderMap| async move {
                        // The tests that predate arch-tokens send no header
                        // and ran as the one token there was: scope all.
                        let who = identify(
                            headers.get("authorization").and_then(|v| v.to_str().ok()),
                            &st.config.token,
                            &st.config.tokens,
                        )
                        .unwrap_or(Identity {
                            name: "legacy".into(),
                            scope: homelab_proto::Scope::All,
                        });
                        ws.on_upgrade(move |s| serve_ws(s, st, who, handler))
                    },
                ),
            )
            .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

/// Stands in for a deploy whose service check reaches a question: it asks
/// through the real `LiveAsker` and reports the answer it got. `Answer`
/// goes to the real handler, which is what delivers it.
async fn asking_handler(st: AppState, req: RpcRequest) -> RpcResponse {
    asking_handler_with(st, req, false).await
}

/// fix-240: [`asking_handler`] that also raises the question as a notice
/// (the test's state directory is its own).
async fn notifying_asking_handler(st: AppState, req: RpcRequest) -> RpcResponse {
    asking_handler_with(st, req, true).await
}

async fn asking_handler_with(st: AppState, req: RpcRequest, notify: bool) -> RpcResponse {
    if matches!(req.command, Rpc::Answer { .. } | Rpc::AnswerOpen { .. }) {
        return handle_rpc(&st, req).await;
    }
    let asker = LiveAsker {
        state: &st,
        timeout_s: 5,
        notify,
    };
    let q = homelab_core::ask::Question {
        op: "deploy-gateway".into(),
        step: "service checks".into(),
        what: "routes went from 29 to 28".into(),
        if_allowed: "the deploy goes on".into(),
        if_stopped: "the deploy stops".into(),
    };
    let answer = homelab_core::ask::Asker::ask(&asker, &q).await;
    RpcResponse {
        id: req.id,
        ok: answer.may_continue(),
        message: format!("{:?}", answer),
        deferred: None,
    }
}

/// covers: fix-66
///
/// An operation started over a connection asks a question, and the
/// operator answers over the SAME connection, which is what the TUI does.
/// Until 2026-09-27 the session read the next frame only after the
/// running request returned, so the answer sat unread behind the deploy
/// that was waiting for it, and every question asked of a TUI-started
/// operation ended as Unattended whatever was pressed
/// (host-questions-unanswerable). Real socket, real session loop.
#[tokio::test]
async fn fix_66_an_answer_on_the_same_connection_reaches_the_waiting_operation() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(asking_handler).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());
    tx.send(frame(RpcRequest {
        id: 1,
        command: Rpc::Ping,
    }))
    .await
    .unwrap();
    let verdict = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            match serde_json::from_str::<ServerMsg>(&t).unwrap() {
                ServerMsg::Ask { id, .. } => {
                    tx.send(frame(RpcRequest {
                        id: 2,
                        command: Rpc::Answer {
                            boot: None,
                            id,
                            allow: true,
                        },
                    }))
                    .await
                    .unwrap();
                }
                ServerMsg::RpcDone(r) if r.id == 1 => return r.message,
                _ => {}
            }
        }
        "the connection closed".to_string()
    })
    .await
    .expect("the operation never finished");
    assert_eq!(
        verdict, "Allow",
        "the operator allowed over the same connection, but the operation heard: {}",
        verdict
    );
}

/// covers: fix-240
///
/// A headless run (`homelab deploy … < /dev/null`) cannot answer the
/// question its own operation raised. A SECOND session answers it by the
/// operation's subject (`homelab answer gateway allow`), the waiting
/// operation hears Allow, the asking session learns the question is
/// settled, and the question carried the host's own wait. Real socket,
/// two real sessions on one host state.
#[tokio::test]
async fn fix_240_another_session_answers_a_running_operations_question() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(asking_handler).await;
    let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());
    let (headless, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect the headless session");
    let (mut htx, mut hrx) = headless.split();
    htx.send(frame(RpcRequest {
        id: 1,
        command: Rpc::Ping,
    }))
    .await
    .unwrap();
    let (outcome, wait_s, settled) = tokio::time::timeout(Duration::from_secs(20), async {
        let mut wait_s = None;
        let mut settled = false;
        // The reply and the broadcast travel separate channels, so
        // either may arrive first: wait for both.
        let mut outcome: Option<String> = None;
        while let Some(Ok(WsMsg::Text(t))) = hrx.next().await {
            match serde_json::from_str::<ServerMsg>(&t).unwrap() {
                // The headless session does not answer; another one does.
                ServerMsg::Ask { wait_s: w, .. } => {
                    wait_s = w;
                    let (other, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
                        .await
                        .expect("connect the answering session");
                    let (mut otx, mut orx) = other.split();
                    otx.send(frame(RpcRequest {
                        id: 7,
                        command: Rpc::AnswerOpen {
                            op: Some("gateway".into()),
                            allow: true,
                        },
                    }))
                    .await
                    .unwrap();
                    while let Some(Ok(WsMsg::Text(t))) = orx.next().await {
                        if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t)
                            && r.id == 7
                        {
                            assert!(r.ok, "the answer was not taken: {}", r.message);
                            break;
                        }
                    }
                }
                ServerMsg::AskSettled { .. } => settled = true,
                ServerMsg::RpcDone(r) if r.id == 1 => outcome = Some(r.message),
                _ => {}
            }
            if let (Some(o), true) = (&outcome, settled) {
                return (o.clone(), wait_s, settled);
            }
        }
        (
            outcome.unwrap_or_else(|| "the connection closed".to_string()),
            wait_s,
            settled,
        )
    })
    .await
    .expect("the operation never finished");
    assert_eq!(outcome, "Allow", "the operation heard: {}", outcome);
    assert_eq!(wait_s, Some(5), "the question must carry the host's wait");
    assert!(
        settled,
        "the asking session was not told the question is settled"
    );
}

/// covers: fix-240
///
/// Nobody answers: the question still fails its operation (silence is
/// never permission), and on the way it was raised as an urgent notice
/// in the host's notices that names the question and how to answer it.
#[tokio::test]
async fn fix_240_an_unanswered_question_is_a_notice_and_still_fails() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let dir = std::env::temp_dir().join(format!("fix240-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    ));
    let addr = serve_state_on_loopback(test_state(config), notifying_asking_handler).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .unwrap();
    let (mut tx, mut rx) = ws.split();
    tx.send(WsMsg::Text(
        serde_json::to_string(&RpcRequest {
            id: 1,
            command: Rpc::Ping,
        })
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t)
                && r.id == 1
            {
                return r;
            }
        }
        panic!("the connection closed");
    })
    .await
    .expect("the operation never finished");
    assert!(
        !outcome.ok,
        "an unanswered question let the operation go on"
    );
    assert!(
        outcome.message.contains("Unattended"),
        "{}",
        outcome.message
    );
    let notices = std::fs::read_to_string(dir.join("notices.jsonl")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let all = homelab_core::notify::parse_notices(&notices);
    let n = all
        .iter()
        .find(|n| n.op.contains("deploy-gateway"))
        .unwrap_or_else(|| panic!("no notice for the question: {notices}"));
    assert!(n.urgent, "{:?}", n);
    assert!(n.what.contains("routes went from 29 to 28"), "{:?}", n);
    assert!(
        n.remedy.contains("homelab answer deploy-gateway"),
        "{:?}",
        n
    );
}

/// A handler shaped like the real `Rpc::BackupCalendar`/`Rpc::GetBackups`
/// arms (fix-180): a cache miss kicks the slow restic read into a
/// detached background task and answers "not read yet" at once, instead
/// of awaiting it. Everything else goes to the real `handle_rpc`.
async fn cache_answering_handler(st: AppState, req: RpcRequest) -> RpcResponse {
    if let Rpc::BackupCalendar { .. } = req.command {
        let cache = st.snapshot_cache.clone();
        tokio::spawn(async move {
            // Stands in for a real, slow `restic snapshots` over rclone.
            let exec = SlowMockExecutor::new(Duration::from_secs(2));
            cache
                .refresh(
                    &exec,
                    &homelab_core::ops::backup::BackupCfg::default(),
                    "gateway",
                    1,
                )
                .await;
        });
        return RpcResponse {
            id: req.id,
            ok: true,
            message: "not read yet".into(),
            deferred: None,
        };
    }
    handle_rpc(&st, req).await
}

/// `MockExecutor` with every `run` delayed, so a test can stand in for a
/// restic read slow enough to matter without a real restic binary.
struct SlowMockExecutor {
    inner: homelab_core::mock::MockExecutor,
    delay: Duration,
}

impl SlowMockExecutor {
    fn new(delay: Duration) -> Self {
        Self {
            inner: homelab_core::mock::MockExecutor::new(),
            delay,
        }
    }
}

#[async_trait::async_trait]
impl homelab_core::executor::Executor for SlowMockExecutor {
    async fn run(
        &self,
        cmd: &homelab_core::executor::Cmd,
    ) -> Result<homelab_core::executor::CmdOutput, homelab_core::error::CoreError> {
        tokio::time::sleep(self.delay).await;
        self.inner.run(cmd).await
    }
    async fn read_file(&self, path: &str) -> Result<String, homelab_core::error::CoreError> {
        self.inner.read_file(path).await
    }
    async fn write_file(
        &self,
        path: &str,
        content: &str,
        mode: u32,
    ) -> Result<(), homelab_core::error::CoreError> {
        self.inner.write_file(path, content, mode).await
    }
    async fn sleep_ms(&self, ms: u64) {
        self.inner.sleep_ms(ms).await
    }
}

/// covers fix-180: before this fix, `Rpc::BackupCalendar` read restic
/// straight on the request path, so a slow repository (restic over
/// rclone, measured live on CT 120 taking past 180 s) ran inside the
/// one serial worker (fix-66) and blocked every request queued behind
/// it — a deploy, a Live view step, anything. With the cache, a miss
/// answers "not read yet" at once and the slow read moves to a detached
/// background task, so a request sent right after it on the SAME
/// connection (and so queued behind it on the very same `work_tx` the
/// serial worker drains) finishes long before the simulated 2 s restic
/// read would have.
#[tokio::test]
async fn fix_180_a_cache_miss_backup_calendar_does_not_block_the_queue_behind_it() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(cache_answering_handler).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());

    let started = std::time::Instant::now();
    // Neither request opts into `reads_beside_queue` (the TUI/CLI
    // never do, fix-66), so both go through the one serial `work_tx`.
    tx.send(frame(RpcRequest {
        id: 1,
        command: Rpc::BackupCalendar {
            stacks: Vec::new(),
            force: false,
        },
    }))
    .await
    .unwrap();
    tx.send(frame(RpcRequest {
        id: 2,
        command: Rpc::Ping,
    }))
    .await
    .unwrap();

    let mut done = std::collections::HashSet::new();
    tokio::time::timeout(Duration::from_secs(1), async {
        while done.len() < 2 {
            if let ServerMsg::RpcDone(r) = serde_json::from_str::<ServerMsg>(
                &rx.next().await.unwrap().unwrap().into_text().unwrap(),
            )
            .unwrap()
            {
                done.insert(r.id);
            }
        }
    })
    .await
    .expect(
        "both the cache-miss calendar read and the request queued behind it \
         must finish in well under the simulated 2 s restic read — the queue \
         must not be blocked by it",
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "took {:?}, which is not meaningfully faster than the 2 s background read \
         it should never have waited for",
        started.elapsed()
    );
}

/// fix-217 (Kenny, Dutch, 2026-10-02: "bij kalender bv blijft die op 12
/// steken, welke doet die dan niet?"): a freshly deployed stack with a
/// real manifest, a real (non-`no_backup`) mount and NO snapshots at
/// all yet (`restic snapshots` on a never-initialised repository exits
/// 10, which `list_snapshots` already reads as `Ok(vec![])`, not an
/// error) must still reach a terminal, counted-as-read answer — "no
/// snapshots yet" — once its cache entry has been filled, the same as
/// any other stack. The first call (cold cache) answers "not read yet"
/// (fix-180's documented shape); the kicked background refresh (here,
/// against a `restic` binary this test environment does not have,
/// which fails fast rather than hanging) still produces a cache entry,
/// so the very next call already resolves with a real `measured_at`
/// and an empty `stacks["inbox"]` — never stuck pending forever.
#[tokio::test]
async fn fix_217_a_never_backed_up_stack_still_resolves_to_a_read_empty_answer() {
    let dir = std::env::temp_dir().join(format!("homelab-fix217-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    )));

    let manifest: homelab_core::manifest::StackManifest = serde_json::from_value(serde_json::json!({
        "stack_name": "inbox",
        "vmid": 150,
        "hostname": "150-app-inbox",
        "network": {"ip": "10.10.10.50/24", "gateway": "10.10.10.1", "bridge": "vmbr0"},
        "resources": {"cores": 1, "memory_mb": 512, "swap_mb": 0, "disk_gb": 4, "storage": "local-zfs"},
        "lxc": {"timezone": "host", "template": "t", "unprivileged": true, "features": "", "protection": false, "gpu": false, "vpn": false},
        "boot": {"onboot": true},
        // A real, backed-up mount — not `no_data`, not `no_backup` —
        // so this stack is exactly the "will get a real repository,
        // just never read yet" case fix-202 left untouched.
        "storage": [{"host_path": "/mnt/inbox/data", "mount_point": "/data"}],
        "apps": ["inbox"],
    }))
    .unwrap();

    let mut st = homelab_core::state::HostState::default();
    st.stacks.insert(
        "inbox".into(),
        homelab_core::state::StackState {
            applied_source: None,
            vmid: 150,
            hostname: "150-app-inbox".into(),
            apps: vec!["inbox".into()],
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: Some(manifest),
            enabled: true,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
        },
    );
    homelab_core::state::StateStore::new(&RealExecutor, &dir.to_string_lossy())
        .save(st)
        .await
        .unwrap();

    let ask = || RpcRequest {
        id: 1,
        command: Rpc::BackupCalendar {
            stacks: Vec::new(),
            force: false,
        },
    };

    let first = handle_rpc(&state, ask()).await;
    assert!(first.ok);
    let body: serde_json::Value = serde_json::from_str(&first.message).unwrap();
    assert!(
        body["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str() == Some("inbox: not read yet")),
        "a cold cache must say so, not nothing: {body}"
    );
    assert!(
        body["measured_at"]["inbox"].is_null(),
        "the first, cold-cache answer must not claim a reading yet: {body}"
    );

    // The kick fired by that first call is a detached background task
    // (`kick_snapshot_refresh`, real executor); give it a moment to run
    // and store its answer — it fails fast here (no `restic` binary in
    // this test environment), which is itself still a cache entry
    // (`SnapshotCache::refresh` caches the error too), not a hang.
    // fix-262: a fixed 300 ms was not enough under a loaded gate run (the
    // release gate of 3.70.7 failed here once); poll until the entry is
    // stored, up to 10 s, instead of guessing how long the kick takes.
    let unread = |b: &serde_json::Value| {
        b["skipped"]
            .as_array()
            .is_some_and(|a| a.iter().any(|s| s.as_str() == Some("inbox: not read yet")))
    };
    let mut second = handle_rpc(&state, ask()).await;
    for _ in 0..100 {
        let b: serde_json::Value = serde_json::from_str(&second.message).unwrap();
        if !unread(&b) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        second = handle_rpc(&state, ask()).await;
    }
    assert!(second.ok);
    let body: serde_json::Value = serde_json::from_str(&second.message).unwrap();
    assert!(
        !body["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str() == Some("inbox: not read yet")),
        "the second call must no longer call this stack unread, once its \
         cache entry exists: {body}"
    );
    assert!(
        body["measured_at"]["inbox"].is_number(),
        "a stack whose cache entry now exists must answer a real \
         measured_at, never stay null: {body}"
    );
    assert_eq!(
        body["stacks"]["inbox"],
        serde_json::json!([]),
        "zero snapshots ever is still a read, empty answer, not an absent one: {body}"
    );
}

/// fix-127 (expert panel, websocket-edge-cases, 2026-09-27): the session
/// read `while let Some(Ok(Message::Text(..)))`, so the first Ping,
/// Pong or Binary frame ended it. A keepalive ping from a client or a
/// proxy closed the line without a word.
#[tokio::test]
async fn fix_127_a_ping_or_binary_frame_does_not_end_the_session() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(|st, req| async move { handle_rpc(&st, req).await }).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    tx.send(WsMsg::Ping(vec![1, 2, 3].into())).await.unwrap();
    tx.send(WsMsg::Binary(vec![0xff; 8].into())).await.unwrap();
    let req = RpcRequest {
        id: 7,
        command: Rpc::Ping,
    };
    tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
        .await
        .unwrap();
    let answered = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(frame)) = rx.next().await {
            if let WsMsg::Text(t) = frame
                && let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t)
            {
                return r.id == 7 && r.ok;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(
        answered,
        "the request after a ping and a binary frame is answered"
    );
}

/// feat-platform-2: an app's status from a reading, and what stands in
/// for it before the first reading.
#[test]
fn feat_platform_2_app_status_comes_from_the_reading() {
    use homelab_core::ops::livestatus::{AppStatus, LiveStatus};
    assert_eq!(
        live_app(None, 106, "jellyfin"),
        (true, 0),
        "no reading: the old answer"
    );
    let mut r = LiveStatus::default();
    r.apps.entry(106).or_default().insert(
        "jellyfin".into(),
        AppStatus {
            running: false,
            containers: 1,
            restarts: 4,
            health: None,
        },
    );
    assert_eq!(live_app(Some(&r), 106, "jellyfin"), (false, 4));
    assert_eq!(
        live_app(Some(&r), 106, "bazarr"),
        (false, 0),
        "probed and absent: not running"
    );
    assert_eq!(
        live_app(Some(&r), 107, "kuma"),
        (true, 0),
        "guest not probed: unknown"
    );
}

/// redesign-stackhub-2: an app's own health check goes out from the
/// reading as it was read; nothing read is None, never "healthy".
#[test]
fn redesign_stackhub_2_app_health_comes_from_the_reading() {
    use homelab_core::ops::livestatus::{AppStatus, LiveStatus};
    assert_eq!(live_health(None, 106, "jellyfin"), None);
    let mut r = LiveStatus::default();
    r.apps.entry(106).or_default().insert(
        "jellyfin".into(),
        AppStatus {
            running: true,
            containers: 1,
            restarts: 0,
            health: Some("unhealthy".into()),
        },
    );
    assert_eq!(
        live_health(Some(&r), 106, "jellyfin").as_deref(),
        Some("unhealthy")
    );
    assert_eq!(live_health(Some(&r), 106, "bazarr"), None);
}

/// feat-platform-3: every line an operation prints carries its request
/// and the time; step starts and ends carry a structured mark; the ring
/// keeps only the newest lines.
#[test]
fn feat_platform_3_lines_carry_request_time_and_step_and_the_ring_is_capped() {
    use homelab_core::sink::{Level, PipelineEvent};
    let (tx, mut rx) = broadcast::channel(16);
    let recent = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
    let sink = BroadcastSink {
        log_tx: tx,
        req: Some(7),
        recent: recent.clone(),
        recent_cap: 2,
        timings: std::sync::Mutex::new(Vec::new()),
        subject: std::sync::Mutex::new(None),
        by: None,
    };
    sink.emit(PipelineEvent::StepStarted {
        op: "deploy".into(),
        step: "pull".into(),
    });
    sink.emit(PipelineEvent::Line {
        level: Level::Info,
        source: "HOST".into(),
        msg: "pulling".into(),
    });
    sink.emit(PipelineEvent::StepFinished {
        op: "deploy".into(),
        step: "pull".into(),
        changed: true,
    });
    let first = rx.try_recv().unwrap();
    match first {
        ServerMsg::Log {
            req,
            ts,
            step: Some(mark),
            ..
        } => {
            assert_eq!(req, Some(7));
            assert!(ts.unwrap_or(0) > 1_700_000_000);
            assert_eq!((mark.step.as_str(), mark.finished), ("pull", false));
        }
        other => panic!("{:?}", other),
    }
    match rx.try_recv().unwrap() {
        ServerMsg::Log { req, step, msg, .. } => {
            assert_eq!((req, step, msg.as_str()), (Some(7), None, "pulling"));
        }
        other => panic!("{:?}", other),
    }
    let ring = recent.lock().unwrap();
    assert_eq!(ring.len(), 2, "capped at recent_cap");
    assert!(matches!(&ring[1], ServerMsg::Log { step: Some(m), .. } if m.finished && m.changed));
}

/// feat-platform-3: CurrentOp answers the holder and the kept lines.
#[tokio::test]
async fn feat_platform_3_current_op_answers_the_holder_and_the_newest_lines() {
    let state = test_state(config_from_text("token = \"0123456789abcdef0123\"\n"));
    *state.busy.lock().unwrap() = Some(homelab_core::oplock::Holder {
        what: "deploy media".into(),
        started_unix: 1_800_000_000,
        done: 0,
        total: 0,
        stack: Some("media".into()),
    });
    state.recent.lock().unwrap().push_back(ServerMsg::Log {
        level: homelab_proto::LogLevel::Info,
        source: "HOST".into(),
        msg: "step 3".into(),
        req: Some(9),
        ts: Some(1_800_000_010),
        step: None,
        plan: None,
        by: None,
    });
    let resp = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::CurrentOp,
        },
    )
    .await;
    assert!(resp.ok);
    let view: homelab_proto::CurrentOpView = serde_json::from_str(&resp.message).unwrap();
    assert_eq!(view.holder.as_deref(), Some("deploy media"));
    assert_eq!(view.lines.len(), 1);
}

/// feat-platform-1: asked for JSON, the incident list and the manual
/// checks answer JSON; asked for text, the text stays as it was.
#[tokio::test]
async fn feat_platform_1_reports_answer_json_when_asked() {
    let dir = std::env::temp_dir().join(format!("homelab-json-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("incidents/1800000000-deploy-media")).unwrap();
    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    )));
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::Incidents { json: true },
        },
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
    assert_eq!(v["incidents"][0], "1800000000-deploy-media");
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 2,
            command: Rpc::Incidents { json: false },
        },
    )
    .await;
    assert!(r.message.starts_with("incidents:"), "{}", r.message);
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 3,
            command: Rpc::ListManualChecks { json: true },
        },
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
    assert!(v["checks"].is_array() && v["now"].is_u64(), "{}", r.message);
}

/// arch-history: the sink times each step and remembers the subject.
#[test]
fn arch_history_the_sink_times_steps_and_names_the_subject() {
    use homelab_core::sink::PipelineEvent;
    let (tx, _rx) = broadcast::channel(16);
    let sink = BroadcastSink {
        log_tx: tx,
        req: None,
        recent: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
        recent_cap: 10,
        timings: std::sync::Mutex::new(Vec::new()),
        subject: std::sync::Mutex::new(None),
        by: None,
    };
    sink.emit(PipelineEvent::StepStarted {
        op: "deploy media".into(),
        step: "pull".into(),
    });
    sink.emit(PipelineEvent::StepFinished {
        op: "deploy media".into(),
        step: "pull".into(),
        changed: true,
    });
    sink.emit(PipelineEvent::StepStarted {
        op: "deploy media".into(),
        step: "up".into(),
    });
    let t = sink.timings.lock().unwrap().clone();
    assert_eq!(t.len(), 2);
    assert!(t[0].end >= t[0].start && t[0].end > 0 && t[0].changed);
    assert_eq!(t[1].end, 0, "a step still running has no end");
    assert_eq!(
        sink.subject.lock().unwrap().as_deref(),
        Some("deploy media")
    );
}

/// arch-history: History answers what history.jsonl holds since a moment.
#[tokio::test]
async fn arch_history_the_history_command_reads_since_a_moment() {
    let dir = std::env::temp_dir().join(format!("homelab-hist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    )));
    for (start, name) in [(100u64, "old"), (200, "backup")] {
        record_history(
            &state,
            &homelab_core::history::HistoryEntry::Phase {
                start,
                end: start + 5,
                name: name.into(),
                count: 3,
            },
        );
    }
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::History {
                since: 150,
                until: None,
                limit: 10,
            },
        },
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
    assert_eq!(v["entries"].as_array().unwrap().len(), 1, "{}", r.message);
    assert_eq!(v["entries"][0]["name"], "backup");
}

/// covers: feat-secrets-6
///
/// Kenny, 2026-10-03, "Alle drie": a
/// reveal and a copy each leave one history line — what Activity reads —
/// naming who and which secret, and neither history.jsonl nor audit.log
/// ever holds the value itself. A reveal from an old dashboard (no
/// audit) is still recorded, as a reveal.
#[tokio::test]
async fn redesign_371_a_reveal_is_audited_in_history_without_the_value() {
    let dir = std::env::temp_dir().join(format!("homelab-reveal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("secrets/gateway")).unwrap();
    let value = "TUNNEL_TOKEN=never-in-a-log-7f3a";
    std::fs::write(dir.join("secrets/gateway/traefik.env"), value).unwrap();
    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    )));
    let secret = homelab_proto::SecretRef::Env {
        app: "traefik".into(),
    };
    for (id, audit) in [
        (
            3,
            Some(homelab_proto::RevealAudit {
                purpose: homelab_proto::RevealPurpose::Reveal,
                by: "Kenny".into(),
            }),
        ),
        (
            4,
            Some(homelab_proto::RevealAudit {
                purpose: homelab_proto::RevealPurpose::Copy,
                by: "Claude (Live view)".into(),
            }),
        ),
        (5, None),
    ] {
        let r = handle_rpc(
            &state,
            RpcRequest {
                id,
                command: Rpc::RevealSecret {
                    stack: "gateway".into(),
                    secret: secret.clone(),
                    audit,
                },
            },
        )
        .await;
        assert!(r.ok, "{}", r.message);
        assert_eq!(r.message, value, "the reveal itself still answers");
    }
    let history = std::fs::read_to_string(dir.join("history.jsonl")).unwrap_or_default();
    let audit = std::fs::read_to_string(dir.join("audit.log")).unwrap_or_default();
    assert!(!history.contains("never-in-a-log"), "{history}");
    assert!(!audit.contains("never-in-a-log"), "{audit}");
    let entries = homelab_core::history::parse(&history);
    let said: Vec<(String, Option<String>, Option<String>)> = entries
        .iter()
        .filter_map(|e| match e {
            homelab_core::history::HistoryEntry::Op {
                label, subject, by, ..
            } => Some((label.clone(), subject.clone(), by.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        said,
        vec![
            (
                "reveal-secret".into(),
                Some("revealed gateway/traefik/.env".into()),
                Some("Kenny".into())
            ),
            (
                "copy-secret".into(),
                Some("copied gateway/traefik/.env".into()),
                Some("Claude (Live view)".into())
            ),
            (
                "reveal-secret".into(),
                Some("revealed gateway/traefik/.env".into()),
                None
            ),
        ],
        "{history}"
    );
    assert!(audit.contains("copy-secret stack=gateway"), "{audit}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Decision notify-routing (2026-09-30): every event becomes a notice
/// the dashboard reads after its cursor; only an urgent one tries the
/// push, and what became of it is recorded; the sequence survives a
/// restart.
#[tokio::test]
async fn notify_routing_every_event_is_a_notice_and_only_urgent_pushes() {
    let dir = std::env::temp_dir().join(format!("homelab-notices-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    );
    let state = test_state(config_from_text(&cfg));
    let report = |op: &str, ok: bool| homelab_core::runner::OperationReport {
        op: op.into(),
        steps: Vec::new(),
        ok,
        error: (!ok).then(|| homelab_core::error::OperatorError {
            what: "restic failed".into(),
            why: "repo locked".into(),
            remedy: "unlock it".into(),
        }),
        deferred: None,
        note: None,
    };
    notify(
        &state,
        &RealExecutor,
        "deploy",
        &report("deploy-media", true),
        90,
        Some(4),
        None,
    )
    .await;
    notify(
        &state,
        &RealExecutor,
        "scheduled-backup",
        &report("backup-home", false),
        95,
        None,
        Some("100-backup-home".into()),
    )
    .await;
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::Notices {
                after: 0,
                limit: 10,
            },
        },
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
    let n = v["notices"].as_array().unwrap();
    assert_eq!(n.len(), 2, "{}", r.message);
    assert_eq!(n[0]["push"], "centre only");
    assert_eq!(n[0]["req"], 4);
    assert_eq!(n[0]["page"], "/stacks/media");
    assert_eq!(n[1]["urgent"], true);
    assert!(
        n[1]["push"]
            .as_str()
            .unwrap()
            .starts_with("failed: no notification route"),
        "{}",
        n[1]
    );
    assert_eq!(n[1]["since"], 95);
    assert_eq!(n[1]["incident"], "100-backup-home");
    assert!(
        n[1]["remedy"]
            .as_str()
            .unwrap()
            .contains("`homelab backup home`")
    );
    let first = n[0]["seq"].as_u64().unwrap();
    assert_eq!(v["last_seq"], n[1]["seq"]);
    let after = handle_rpc(
        &state,
        RpcRequest {
            id: 2,
            command: Rpc::Notices {
                after: first,
                limit: 10,
            },
        },
    )
    .await;
    let v2: serde_json::Value = serde_json::from_str(&after.message).unwrap();
    assert_eq!(v2["notices"].as_array().unwrap().len(), 1);
    // A new start reads the newest seq back and only grows from there.
    let again = test_state(config_from_text(&cfg));
    assert_eq!(
        *again.notice_seq.lock().unwrap(),
        v["last_seq"].as_u64().unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// arch-host-link: an answer stamped with an earlier start of the host
/// is refused; one stamped with this start, or unstamped (CLI, TUI), is
/// delivered.
#[tokio::test]
async fn arch_host_link_a_stale_answer_does_not_answer_a_new_question() {
    let state = test_state(config_from_text("token = \"0123456789abcdef0123\"\n"));
    let pending = |state: &AppState| {
        let (tx, rx) = tokio::sync::oneshot::channel();
        state.pending_asks.lock().unwrap().insert(
            1,
            PendingAsk {
                reply: tx,
                ask: ServerMsg::Log {
                    level: homelab_proto::LogLevel::Info,
                    source: "HOST".into(),
                    msg: "question".into(),
                    req: None,
                    ts: None,
                    by: None,
                    step: None,
                    plan: None,
                },
            },
        );
        rx
    };
    let _rx = pending(&state);
    let stale = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::Answer {
                id: 1,
                allow: true,
                boot: Some("0-1".into()),
            },
        },
    )
    .await;
    assert!(
        !stale.ok && stale.message.contains("earlier start"),
        "{}",
        stale.message
    );
    assert!(
        state.pending_asks.lock().unwrap().contains_key(&1),
        "the question still waits"
    );
    let now = handle_rpc(
        &state,
        RpcRequest {
            id: 2,
            command: Rpc::Answer {
                id: 1,
                allow: true,
                boot: Some(state.boot_id.clone()),
            },
        },
    )
    .await;
    assert!(now.ok, "{}", now.message);
    let _rx = pending(&state);
    let plain = handle_rpc(
        &state,
        RpcRequest {
            id: 3,
            command: Rpc::Answer {
                id: 1,
                allow: false,
                boot: None,
            },
        },
    )
    .await;
    assert!(plain.ok, "{}", plain.message);
}

// ── arch-tokens and arch-host-link (homelab-admin, 2026-09-28) ──────

fn sha_hex(t: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(t.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// A state with a legacy token and two scoped ones, its audit log in a
/// directory of its own.
fn scoped_state(tag: &str) -> (AppState, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("homelab-scope-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let raw = format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n\
         [[tokens]]\nname = \"dash-read\"\nscope = \"read\"\nsha256 = \"{}\"\n\
         [[tokens]]\nname = \"dash-operate\"\nscope = \"operate\"\nsha256 = \"{}\"\n\
         [[tokens]]\nname = \"dash-all\"\nscope = \"all\"\nsha256 = \"{}\"\n",
        dir.display(),
        sha_hex("read-token-aaaaaaaaaaaaaaaa"),
        sha_hex("operate-token-bbbbbbbbbbbbbb"),
        sha_hex("all-token-cccccccccccccccccc"),
    );
    (test_state(config_from_text(&raw)), dir)
}

/// Opens a session with `token`, sends `commands` in order and returns
/// the replies in the order they ARRIVED, as (id, ok, message).
async fn session_replies(
    addr: SocketAddr,
    token: &str,
    commands: Vec<(u64, Rpc)>,
) -> Vec<(u64, bool, String)> {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    let mut request = format!("ws://{}/ws", addr).into_client_request().unwrap();
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );
    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("connect");
    let (mut tx, mut rx) = ws.split();
    let n = commands.len();
    for (id, command) in commands {
        let req = RpcRequest { id, command };
        tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
            .await
            .unwrap();
    }
    let mut out = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(frame)) = rx.next().await {
            if let WsMsg::Text(t) = frame
                && let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t)
            {
                out.push((r.id, r.ok, r.message));
                if out.len() == n {
                    return;
                }
            }
        }
    })
    .await;
    out
}

/// Answers every command with "ran <name>"; a non-read command takes a
/// moment, so a read sent after it can overtake it only beside the queue.
async fn naming_handler(_st: AppState, req: RpcRequest) -> RpcResponse {
    if !req.command.is_read_only() {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    RpcResponse {
        id: req.id,
        ok: true,
        message: format!("ran {}", req.command.name()),
        deferred: None,
    }
}

/// Answers with the token name the request runs under.
async fn by_handler(_st: AppState, req: RpcRequest) -> RpcResponse {
    RpcResponse {
        id: req.id,
        ok: true,
        message: requested_by().unwrap_or_else(|| "<none>".into()),
        deferred: None,
    }
}

/// milestone act: the token name reaches the handler of a queued
/// request and of a read beside the queue alike; outside a session
/// there is none.
#[tokio::test]
async fn act_the_token_name_reaches_the_operation_queued_or_beside() {
    let (state, _dir) = scoped_state("by");
    let addr = serve_state_on_loopback(state, by_handler).await;
    let replies = session_replies(
        addr,
        "operate-token-bbbbbbbbbbbbbb",
        vec![
            (
                1,
                Rpc::SessionOptions {
                    reads_beside_queue: true,
                },
            ),
            (2, Rpc::BackupDevices),
            (3, Rpc::GetState),
        ],
    )
    .await;
    for id in [2, 3] {
        let r = replies.iter().find(|r| r.0 == id).expect("a reply");
        assert_eq!(r.2, "dash-operate", "{:?}", replies);
    }
    assert_eq!(requested_by(), None);
}

/// feat-platform-10 (milestone follow).
///
/// Over real sockets: the dashboard's session attaches; a `homelab ui`
/// step from another token reaches it with that token's name and scope,
/// and the dashboard's answer is the CLI's reply. A read token may read
/// the state but not drive; a step never waits behind the queue.
#[tokio::test]
async fn follow_ui_steps_are_relayed_to_the_attached_dashboard_by_scope() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    let (state, _dir) = scoped_state("ui");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    // The dashboard: attaches, then answers every step with what it saw.
    let mut request = format!("ws://{}/ws", addr).into_client_request().unwrap();
    request.headers_mut().insert(
        "Authorization",
        "Bearer all-token-cccccccccccccccccc".parse().unwrap(),
    );
    let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (mut tx, mut rx) = ws.split();
    let attach = RpcRequest {
        id: 1,
        command: Rpc::UiAttach,
    };
    tx.send(WsMsg::Text(serde_json::to_string(&attach).unwrap().into()))
        .await
        .unwrap();
    let (attached_tx, attached_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut attached_tx = Some(attached_tx);
        let mut next = 100;
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            match serde_json::from_str::<ServerMsg>(&t) {
                Ok(ServerMsg::RpcDone(r)) if r.id == 1 => {
                    if let Some(a) = attached_tx.take() {
                        let _ = a.send(());
                    }
                }
                Ok(ServerMsg::Ui {
                    relay,
                    by,
                    scope,
                    step,
                    ..
                }) => {
                    next += 1;
                    let reply = RpcRequest {
                        id: next,
                        command: Rpc::UiReply {
                            relay,
                            ok: true,
                            message: format!("{} {:?} {}", by, scope, step.verb()),
                        },
                    };
                    tx.send(WsMsg::Text(serde_json::to_string(&reply).unwrap().into()))
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }
    });
    attached_rx.await.unwrap();
    let replies = session_replies(
        addr,
        "operate-token-bbbbbbbbbbbbbb",
        vec![
            // A deploy-like command first: the step must not wait for it.
            (1, Rpc::BackupDevices),
            (
                2,
                Rpc::Ui {
                    step: homelab_proto::UiStep::Close,
                    client_version: "3.70.0".into(),
                },
            ),
        ],
    )
    .await;
    assert_eq!(replies.first().map(|r| r.0), Some(2), "{:?}", replies);
    assert_eq!(replies[0].2, "dash-operate Operate close");
    let read = session_replies(
        addr,
        "read-token-aaaaaaaaaaaaaaaa",
        vec![
            (
                1,
                Rpc::Ui {
                    step: homelab_proto::UiStep::State,
                    client_version: "3.70.0".into(),
                },
            ),
            (
                2,
                Rpc::Ui {
                    step: homelab_proto::UiStep::Close,
                    client_version: "3.70.0".into(),
                },
            ),
        ],
    )
    .await;
    let state = read.iter().find(|r| r.0 == 1).unwrap();
    assert!(state.1 && state.2 == "dash-read Read state", "{:?}", read);
    let close = read.iter().find(|r| r.0 == 2).unwrap();
    assert!(
        !close.1 && close.2.contains("needs scope Operate"),
        "{:?}",
        read
    );
    // A session that is not the dashboard cannot answer for it.
    let forged = session_replies(
        addr,
        "operate-token-bbbbbbbbbbbbbb",
        vec![(
            1,
            Rpc::UiReply {
                relay: 1,
                ok: true,
                message: "forged".into(),
            },
        )],
    )
    .await;
    assert!(!forged[0].1, "{:?}", forged);
}

/// milestone act: every line of an operation carries who asked for it.
#[test]
fn act_the_sink_stamps_the_token_name_on_every_line() {
    use homelab_core::sink::PipelineEvent;
    let (tx, mut rx) = broadcast::channel(16);
    let sink = BroadcastSink {
        log_tx: tx,
        req: Some(3),
        recent: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
        recent_cap: 10,
        timings: std::sync::Mutex::new(Vec::new()),
        subject: std::sync::Mutex::new(None),
        by: Some("admin".into()),
    };
    sink.emit(PipelineEvent::StepStarted {
        op: "deploy-media".into(),
        step: "pull".into(),
    });
    sink.emit(PipelineEvent::Line {
        level: homelab_core::sink::Level::Info,
        source: "HOST".into(),
        msg: "pulled".into(),
    });
    for _ in 0..2 {
        match rx.try_recv().unwrap() {
            ServerMsg::Log { by, req, .. } => {
                assert_eq!((by.as_deref(), req), (Some("admin"), Some(3)))
            }
            other => panic!("{:?}", other),
        }
    }
}

#[tokio::test]
async fn arch_tokens_a_read_token_cannot_operate_and_the_refusal_is_audited() {
    let (state, dir) = scoped_state("read");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    let replies = session_replies(
        addr,
        "read-token-aaaaaaaaaaaaaaaa",
        vec![(1, Rpc::GetState), (2, Rpc::ZfsReplicate)],
    )
    .await;
    assert_eq!(replies.len(), 2, "{:?}", replies);
    let read = replies.iter().find(|r| r.0 == 1).unwrap();
    let op = replies.iter().find(|r| r.0 == 2).unwrap();
    assert!(read.1 && read.2 == "ran get_state", "{:?}", read);
    assert!(
        !op.1,
        "an operate command on a read token is refused: {:?}",
        op
    );
    assert!(
        op.2.contains("refused") && op.2.contains("dash-read"),
        "{:?}",
        op
    );
    let audit = std::fs::read_to_string(dir.join("audit.log")).unwrap();
    assert!(
        audit.contains("refused token=dash-read") && audit.contains("cmd=zfs_replicate"),
        "{}",
        audit
    );
}

#[tokio::test]
async fn arch_tokens_an_operate_token_cannot_destroy_exec_or_update_the_host() {
    let (state, _dir) = scoped_state("operate");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    let replies = session_replies(
        addr,
        "operate-token-bbbbbbbbbbbbbb",
        vec![
            (1, Rpc::BackupDevices),
            (
                2,
                Rpc::ExecIn {
                    vmid: 106,
                    command: "ls".into(),
                },
            ),
            (
                3,
                Rpc::DestroyRecorded {
                    stack: "media".into(),
                    confirm: "media".into(),
                    skip_backup: false,
                },
            ),
            (
                4,
                Rpc::SelfUpdateHost {
                    binary_b64: String::new(),
                    proof: None,
                },
            ),
        ],
    )
    .await;
    let by_id = |id: u64| replies.iter().find(|r| r.0 == id).cloned().unwrap();
    assert!(by_id(1).1, "operate may back up: {:?}", replies);
    for id in [2, 3, 4] {
        assert!(
            !by_id(id).1 && by_id(id).2.contains("refused"),
            "{:?}",
            by_id(id)
        );
    }
}

#[tokio::test]
async fn arch_tokens_a_scope_all_command_by_a_named_token_is_audited_before_it_runs() {
    let (state, dir) = scoped_state("all");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    let replies = session_replies(
        addr,
        "all-token-cccccccccccccccccc",
        vec![(
            1,
            Rpc::ForgetStack {
                stack: "drill".into(),
            },
        )],
    )
    .await;
    assert!(replies[0].1, "{:?}", replies);
    let audit = std::fs::read_to_string(dir.join("audit.log")).unwrap();
    assert!(
        audit.contains("scope-all token=dash-all") && audit.contains("cmd=forget_stack"),
        "{}",
        audit
    );
}

#[tokio::test]
async fn arch_tokens_an_unknown_token_opens_no_session() {
    let (state, _dir) = scoped_state("unknown");
    assert_eq!(
        identify(
            Some("Bearer not-a-token-at-all"),
            &state.config.token,
            &state.config.tokens
        ),
        None
    );
    assert_eq!(
        identify(
            Some("Bearer read-token-aaaaaaaaaaaaaaaa"),
            &state.config.token,
            &state.config.tokens
        ),
        Some(Identity {
            name: "dash-read".into(),
            scope: homelab_proto::Scope::Read
        })
    );
    assert_eq!(
        identify(
            Some("Bearer 0123456789abcdef0123"),
            &state.config.token,
            &state.config.tokens
        )
        .map(|i| i.scope),
        Some(homelab_proto::Scope::All)
    );
    assert_eq!(
        identify(None, &state.config.token, &state.config.tokens),
        None
    );
}

#[test]
fn arch_tokens_a_bad_token_list_is_refused_at_start() {
    let e = |name: &str, sha: &str| TokenEntry {
        name: name.into(),
        scope: homelab_proto::Scope::Read,
        sha256: sha.into(),
    };
    let good = sha_hex("x");
    assert!(validate_tokens(&[e("a", &good)]).is_ok());
    assert!(
        validate_tokens(&[e("a", &good), e("a", &good)]).is_err(),
        "duplicate names"
    );
    assert!(
        validate_tokens(&[e("legacy", &good)]).is_err(),
        "reserved name"
    );
    assert!(validate_tokens(&[e("a", "abc")]).is_err(), "short digest");
    assert!(
        validate_tokens(&[e("a", &good.to_uppercase())]).is_err(),
        "uppercase digest"
    );
    assert!(validate_tokens(&[e(" ", &good)]).is_err(), "empty name");
}

/// arch-host-link: without the option, replies keep the order of the
/// requests (the TUI depends on it); with it, a read overtakes a slow
/// command and is told apart by its id.
#[tokio::test]
async fn arch_host_link_reads_skip_the_queue_only_for_a_session_that_asks() {
    let (state, _dir) = scoped_state("beside");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    let plain = session_replies(
        addr,
        "all-token-cccccccccccccccccc",
        vec![(1, Rpc::BackupDevices), (2, Rpc::GetState)],
    )
    .await;
    assert_eq!(
        plain.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![1, 2],
        "{:?}",
        plain
    );
    let asked = session_replies(
        addr,
        "all-token-cccccccccccccccccc",
        vec![
            (
                1,
                Rpc::SessionOptions {
                    reads_beside_queue: true,
                },
            ),
            (2, Rpc::BackupDevices),
            (3, Rpc::GetState),
        ],
    )
    .await;
    assert_eq!(
        asked.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![1, 3, 2],
        "{:?}",
        asked
    );
}

/// Asks a question, and while it waits floods the broadcast channel far
/// past its capacity, so a slow reader lags past the question.
async fn flooding_asker(st: AppState, req: RpcRequest) -> RpcResponse {
    if matches!(req.command, Rpc::Answer { .. }) {
        return handle_rpc(&st, req).await;
    }
    // Spawned before the question is sent, so on this single-threaded
    // test runtime it runs as soon as the asker waits: after the Ask is
    // in the channel, before the session's forwarder has taken it out.
    let flood = st.log_tx.clone();
    tokio::spawn(async move {
        for i in 0..500 {
            let _ = flood.send(ServerMsg::Log {
                req: None,
                step: None,
                ts: None,
                by: None,
                plan: None,
                level: homelab_proto::LogLevel::Debug,
                source: "HOST".into(),
                msg: format!("noise {}", i),
            });
        }
    });
    asking_handler(st, req).await
}

/// fix-127: a client that reads slower than the host writes lags the
/// broadcast channel, and the forwarder skipped what it missed without
/// a word, questions included, so the operation waited for an answer to
/// a question the operator never saw and ended Unattended. Now the
/// client is told how much it missed and every open question is sent
/// again.
#[tokio::test]
async fn fix_127_a_lagging_client_still_gets_the_open_question() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(flooding_asker).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());
    tx.send(frame(RpcRequest {
        id: 1,
        command: Rpc::Ping,
    }))
    .await
    .unwrap();
    let mut told_of_the_gap = false;
    let verdict = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            match serde_json::from_str::<ServerMsg>(&t).unwrap() {
                ServerMsg::Ask { id, .. } => {
                    tx.send(frame(RpcRequest {
                        id: 2,
                        command: Rpc::Answer {
                            boot: None,
                            id,
                            allow: true,
                        },
                    }))
                    .await
                    .unwrap();
                }
                ServerMsg::Log { msg, .. } if msg.contains("dropped") => told_of_the_gap = true,
                ServerMsg::RpcDone(r) if r.id == 1 => return r.message,
                _ => {}
            }
        }
        "the connection closed".to_string()
    })
    .await
    .expect("the operation never finished");
    assert_eq!(verdict, "Allow", "the question reached the operator");
    assert!(
        told_of_the_gap,
        "and the client was told it missed messages"
    );
}

/// A state whose `state_dir` is a fresh directory of its own, holding a
/// self-update marker armed at `armed_at`.
fn state_with_marker(tag: &str, armed_at: u64) -> (AppState, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("homelab-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let toml = dir.join("host.toml");
    std::fs::write(
        &toml,
        format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        ),
    )
    .unwrap();
    let marker = dir.join("selfupdate.pending");
    std::fs::write(
        &marker,
        format!("{{\"to_version\":\"9.9.9\",\"armed_at\":{}}}\n", armed_at),
    )
    .unwrap();
    (
        test_state(load_config_from(toml.to_string_lossy().into_owned())),
        marker,
    )
}

/// One Ping over a real session, answered.
async fn ping_over(addr: SocketAddr) {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    let req = RpcRequest {
        id: 1,
        command: Rpc::Ping,
    };
    tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t) {
                assert!(r.ok);
                return;
            }
        }
        panic!("the connection closed before the ping was answered");
    })
    .await
    .expect("the ping was answered");
    // The acceptance runs after the answer is queued; give it its turn.
    tokio::time::sleep(Duration::from_millis(200)).await;
}

/// fix-121 (expert panel, self-update-acceptance-weak, 2026-09-27): a new
/// daemon was accepted after five seconds alive, before anything had
/// talked to it, so a binary that ran but could not serve a client was
/// kept. It is accepted now by the first authenticated request it
/// answers.
#[tokio::test]
async fn fix_121_a_self_update_is_accepted_by_an_answered_request() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (state, marker) = state_with_marker("fix121-new", now - 100);
    let addr =
        serve_state_on_loopback(state, |st, req| async move { handle_rpc(&st, req).await }).await;
    assert!(marker.exists(), "precondition: the update is pending");
    ping_over(addr).await;
    let accepted = !marker.exists();
    let _ = std::fs::remove_dir_all(marker.parent().unwrap());
    assert!(
        accepted,
        "an answered request from the new daemon accepts the update"
    );
}

/// fix-121: the daemon that ARMED the marker answers the self-update
/// request itself; that answer must not accept the binary that replaces
/// it.
#[tokio::test]
async fn fix_121_the_old_daemon_never_accepts_its_successor() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (state, marker) = state_with_marker("fix121-old", now + 100);
    let addr =
        serve_state_on_loopback(state, |st, req| async move { handle_rpc(&st, req).await }).await;
    ping_over(addr).await;
    let kept = marker.exists();
    let _ = std::fs::remove_dir_all(marker.parent().unwrap());
    assert!(kept, "armed after this daemon started: not its own");
}

/// fix-124 (expert panel, exec-logged-verbatim, 2026-09-27): a remote
/// exec is recorded in audit.log and the journal before it runs, and was
/// recorded verbatim, so a password typed on the command line was kept
/// on pve indefinitely. The record still names the vmid and the command,
/// with the values the shared masker recognises taken out.
#[test]
fn fix_124_an_exec_command_is_masked_before_it_is_recorded() {
    let line = exec_audit_line(
        1_800_000_000,
        108,
        "PGPASSWORD=hunter2-x9 psql -h db -c 'select 1'; curl -H 'Authorization: Bearer tk-77' x",
    );
    assert!(
        !line.contains("hunter2-x9") && !line.contains("tk-77"),
        "{}",
        line
    );
    assert!(
        line.starts_with("1800000000 exec vmid=108 ") && line.contains("psql -h db"),
        "the record still says what ran where: {}",
        line
    );
    assert!(
        line.ends_with('\n') && line.lines().count() == 1,
        "{:?}",
        line
    );
}

/// fix-125 (expert panel, bundles-audit-world-readable, 2026-09-27):
/// audit.log (every exec command) and the incident bundles were readable
/// by any local account on pve. A new audit.log is created 0600, and at
/// start the daemon takes group and world access off what exists.
#[test]
fn fix_125_the_audit_log_and_existing_bundles_are_made_private() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let set = |p: &std::path::Path, m: u32| {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
    };
    let dir = std::env::temp_dir().join(format!("homelab-fix125-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bundle = dir.join("incidents/1800000000-deploy-x");
    std::fs::create_dir_all(&bundle).unwrap();
    for (f, m) in [("report.json", 0o644), ("commands.sh", 0o755)] {
        std::fs::write(bundle.join(f), "x").unwrap();
        set(&bundle.join(f), m);
    }
    set(&dir.join("incidents"), 0o755);
    set(&bundle, 0o755);
    std::fs::write(dir.join("audit.log"), "old\n").unwrap();
    set(&dir.join("audit.log"), 0o644);

    tighten_private_paths(&dir.to_string_lossy());
    let fresh = dir.join("fresh-audit.log");
    append_audit(&fresh.to_string_lossy(), "1 exec vmid=108 cmd=\"ls\"\n").unwrap();

    let got = [
        mode(&dir.join("incidents")),
        mode(&bundle),
        mode(&bundle.join("report.json")),
        mode(&bundle.join("commands.sh")),
        mode(&dir.join("audit.log")),
        mode(&fresh),
    ];
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got, [0o700, 0o700, 0o600, 0o700, 0o600, 0o600]);
}

/// One plain HTTP GET against `addr`, with an optional bearer token;
/// returns the status code and the body.
async fn http_get(addr: SocketAddr, path: &str, token: Option<&str>) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let auth = token
        .map(|t| format!("Authorization: Bearer {}\r\n", t))
        .unwrap_or_default();
    s.write_all(
        format!(
            "GET {} HTTP/1.1\r\nHost: x\r\n{}Connection: close\r\n\r\n",
            path, auth
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    let code = out
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

/// fix-126 (expert panel, unauth-version-plaintext-kyu, 2026-09-27):
/// `/api/version` told any neighbour which release runs, which is what a
/// probe needs to pick a known fault. It now takes the token like the
/// line itself; `/api/health` stays open, it says only `ok`.
#[tokio::test]
async fn fix_126_the_version_endpoint_needs_the_token() {
    let path = format!("/tmp/homelab-fix126-{}.toml", std::process::id());
    std::fs::write(&path, "token = \"0123456789abcdef0123\"\n").unwrap();
    let state = test_state(load_config_from(path.clone()));
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = app_router(state.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let (code, body) = http_get(addr, "/api/version", None).await;
    assert_eq!(code, 401, "no token, no version: {}", body);
    assert!(!body.contains(VERSION), "{}", body);
    assert_eq!(state.auth_failures.snapshot().count, 1, "and it is counted");
    let (code, body) = http_get(addr, "/api/version", Some("0123456789abcdef0123")).await;
    assert_eq!((code, body.trim()), (200, VERSION));
    assert_eq!(http_get(addr, "/api/health", None).await.0, 200);
}

/// fix-126: a notification route that sends the bearer token over plain
/// HTTP is named at start. The token to kyu crossed VLAN 10 in clear
/// text, where an ARP-spoofing container reads it.
#[test]
fn fix_126_a_bearer_sent_over_plain_http_is_named() {
    let routes = plaintext_bearer_routes(
        Some("http://10.10.10.9:8080/publish/notify.kenny"),
        Some("tok"),
        Some("http://10.10.5.101:8123/api/webhook/abc"),
        None,
    );
    assert_eq!(routes.len(), 1, "{:?}", routes);
    assert!(
        routes[0].starts_with("http://10.10.10.9:8080"),
        "{:?}",
        routes
    );
    assert!(
        !routes[0].contains("notify.kenny"),
        "path withheld: {:?}",
        routes
    );
    assert!(
        plaintext_bearer_routes(Some("https://kyu.example/publish"), Some("tok"), None, None)
            .is_empty()
    );
    assert!(
        plaintext_bearer_routes(Some("http://127.0.0.1:8080/x"), Some("tok"), None, None)
            .is_empty(),
        "loopback never crosses a wire"
    );
}

/// A `MakeWriter` into a shared buffer, to read what the journal gets.
#[derive(Clone, Default)]
struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// fix-122 (expert panel, journal-lines-lack-op-context, 2026-09-27):
/// three backups run at once at night, and their journal lines carried
/// only `source=HOST` and the message, so `image not found` could not be
/// tied to one of them; the lines also carried ANSI colour codes, which
/// break `grep` on the journal. Every line of an operation now names the
/// operation and its stack, steps are logged as they start and finish,
/// and no escape code reaches the journal.
#[test]
fn fix_122_journal_lines_name_their_operation_and_stack_without_colour() {
    use homelab_core::runner::{Runner, StepOutcome};
    use tracing::Instrument;
    let out = Captured::default();
    let subscriber = journal_subscriber(out.clone(), "info");
    let dir = std::env::temp_dir().join(format!("homelab-fix122-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let toml = dir.join("host.toml");
    std::fs::write(
        &toml,
        format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        ),
    )
    .unwrap();
    let state = test_state(load_config_from(toml.to_string_lossy().into_owned()));
    tracing::subscriber::with_default(subscriber, || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(
                async {
                    run_op_locked(&state, &RealExecutor, 0, "scheduled-backup", |ctx| {
                        Box::pin(async move {
                            let mut r = Runner::new("backup-paperwork", ctx.sink, ctx.journal);
                            r.plan(&["snapshot"]);
                            let _ = r
                                .step("snapshot", || async {
                                    ctx.sink.emit(PipelineEvent::Line {
                                        level: homelab_core::sink::Level::Warn,
                                        source: "HOST".into(),
                                        msg: "image not found".into(),
                                    });
                                    Ok(StepOutcome::Changed)
                                })
                                .await;
                            r.finish_ok()
                        })
                    })
                    .await
                }
                .instrument(stack_span("paperwork")),
            );
    });
    let _ = std::fs::remove_dir_all(&dir);
    let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
    assert!(!text.contains('\u{1b}'), "no colour codes:\n{}", text);
    let line = text
        .lines()
        .find(|l| l.contains("image not found"))
        .unwrap_or_else(|| panic!("the line reached the journal:\n{}", text));
    assert!(
        line.contains("stack=paperwork") && line.contains("op=scheduled-backup"),
        "the line names its stack and operation: {}",
        line
    );
    assert!(
        text.lines()
            .any(|l| l.contains("snapshot") && l.contains("stack=paperwork")),
        "the step's start and finish are journal lines too:\n{}",
        text
    );
}

#[test]
fn h12_scheduler_clock_logic() {
    // Weird `date` output never silently disables the scheduler.
    assert_eq!(parse_local_hour("04\n"), Some(4));
    assert_eq!(parse_local_hour("garbage"), None);
    assert_eq!(parse_local_hour("99"), None);
    let now = 1_800_000_000u64;
    assert!(backup_due(4, 4, now - 25 * 3600, now));
    assert!(
        !backup_due(4, 7, now - 25 * 3600, now),
        "outside the night window (fix-129)"
    );
    assert!(!backup_due(4, 4, now - 3600, now), "backed up an hour ago");
    assert!(backup_due(4, 4, 0, now), "never backed up");
}

/// fix-129 (expert panel, restart-skips-night, 2026-09-27): a backup was
/// due only while the local hour equalled `backup_hour`, and the first
/// check came twenty minutes after start. A self-update, crash or power
/// cut inside that hour cost the whole night: no backups, updates,
/// host-meta or fleet check. What was not done in the hour is caught up
/// in the next one, and the first check comes a minute after start.
#[test]
fn fix_129_a_night_missed_by_a_restart_is_caught_up_before_morning() {
    let now = 1_800_000_000u64;
    let stale = now - 25 * 3600;
    assert!(
        backup_due(4, 5, stale, now),
        "04:xx lost, caught up at 05:xx"
    );
    assert!(!backup_due(4, 6, stale, now), "not into the day");
    assert!(!backup_due(4, 3, stale, now), "not before the hour");
    assert!(backup_due(23, 0, stale, now), "the window wraps midnight");
    assert!(
        !backup_due(4, 5, now - 3600, now),
        "done in the hour: not again"
    );
    let st = NightlyState {
        last_integrity_check: 0,
        last_second_copy: 0,
        second_copy_configured: Default::default(),
        last_host_meta: stale,
        last_zfs: now,
        last_restore_drill: now,
        restore_drill_interval_s: 90 * 24 * 3600,
        zfs_configured: false,
        devices_configured: false,
    };
    assert_eq!(
        nightly_plan(4, 5, now, &[("a".into(), true, stale)], &st),
        vec![NightlyTask::Stack("a".into()), NightlyTask::HostMeta]
    );
    assert!(
        Duration::from_secs(SCHEDULER_FIRST_CHECK_S) <= Duration::from_secs(60),
        "the first look comes soon after a start, not a tick later"
    );
}

/// fix-nightly-1 (Kenny, 2026-10-09): a backup by hand in the evening
/// moved `last_backup`, the 20 h rule then skipped the stack that night.
/// The nightly reads its own clock; a manual backup does not move it.
#[test]
fn fix_nightly_1_a_manual_backup_does_not_count_as_tonights_nightly() {
    let now = 1_800_000_000u64;
    let mut hs = homelab_core::state::HostState::default();
    let mut st = homelab_core::state::StackState {
        applied_source: None,
        vmid: 108,
        hostname: "108-app-a".into(),
        apps: vec![],
        applied_at: 0,
        // Backed up by hand three hours ago.
        last_backup: now - 3 * 3600,
        applied_hash: String::new(),
        manifest: None,
        enabled: true,
        natives: Vec::new(),
        incomplete_step: None,
        route_file: None,
        extra_route_files: Vec::new(),
        pushed_file_hashes: std::collections::BTreeMap::new(),
        component_digests: Default::default(),
    };
    hs.stacks.insert("a".into(), st.clone());
    st.enabled = false;
    hs.stacks.insert("parked".into(), st);
    // The last night's own backup was 25 h ago.
    hs.last_nightly_backup.insert("a".into(), now - 25 * 3600);
    let inputs = nightly_stack_inputs(&hs);
    let nst = NightlyState {
        last_integrity_check: now,
        last_second_copy: 0,
        second_copy_configured: Default::default(),
        last_host_meta: now,
        last_zfs: now,
        last_restore_drill: now,
        restore_drill_interval_s: 90 * 24 * 3600,
        zfs_configured: false,
        devices_configured: false,
    };
    assert_eq!(
        nightly_plan(4, 4, now, &inputs, &nst),
        vec![NightlyTask::Stack("a".into())],
        "the evening's manual backup must not cost the stack its night"
    );
    // Once the night recorded its own backup, it is not due again.
    hs.last_nightly_backup.insert("a".into(), now - 600);
    assert!(nightly_plan(4, 4, now, &nightly_stack_inputs(&hs), &nst).is_empty());
}

/// fix-stale-refresh-1: only operations that replace a stack's running
/// images re-read them afterwards.
#[test]
fn fix_stale_refresh_1_only_image_changing_operations_refresh() {
    for l in ["deploy", "update", "restart"] {
        assert!(refreshes_running_images(l), "{l}");
    }
    for l in ["backup", "restore", "resize", "verify-restore", "adopt"] {
        assert!(!refreshes_running_images(l), "{l}");
    }
}

/// G14: the drill rides the backup hour and only when it is due.
#[test]
fn g14_the_restore_drill_is_planned_only_in_the_backup_hour_and_only_when_due() {
    let never = NightlyState {
        last_host_meta: 0,
        last_zfs: 0,
        last_restore_drill: 0,
        last_integrity_check: 0,
        restore_drill_interval_s: 90 * 24 * 3600,
        zfs_configured: false,
        devices_configured: false,
        second_copy_configured: false,
        last_second_copy: 0,
    };
    assert!(
        nightly_plan(4, 4, 1_000_000, &[], &never).contains(&NightlyTask::RestoreDrill),
        "never drilled, and it is the hour: it must be planned"
    );
    assert!(
        !nightly_plan(4, 7, 1_000_000, &[], &never).contains(&NightlyTask::RestoreDrill),
        "a restore pulls a whole snapshot back — not outside the backup hour"
    );
    let fresh = NightlyState {
        last_restore_drill: 1_000_000 - 3600,
        ..never
    };
    assert!(
        !nightly_plan(4, 4, 1_000_000, &[], &fresh).contains(&NightlyTask::RestoreDrill),
        "drilled an hour ago: not again tonight"
    );
}

/// fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): the
/// second copy runs once a night when configured, the rotating restic
/// check once a night either way, both in the backup hour.
#[test]
fn fix96_the_second_copy_and_the_check_are_planned_once_a_night() {
    let now = 1_000_000;
    let due = NightlyState {
        last_host_meta: now,
        last_zfs: 0,
        last_restore_drill: now,
        last_integrity_check: 0,
        restore_drill_interval_s: 90 * 24 * 3600,
        zfs_configured: false,
        devices_configured: false,
        second_copy_configured: true,
        last_second_copy: 0,
    };
    let plan = nightly_plan(4, 4, now, &[], &due);
    assert!(plan.contains(&NightlyTask::SecondCopy), "{:?}", plan);
    assert!(plan.contains(&NightlyTask::IntegrityCheck), "{:?}", plan);
    assert!(
        // fix-129: the night is backup_hour plus a catch-up hour, so
        // "outside" starts two hours later.
        nightly_plan(4, 7, now, &[], &due).is_empty(),
        "outside the two-hour night nothing runs"
    );
    let unconfigured = NightlyState {
        second_copy_configured: false,
        ..due
    };
    let plan = nightly_plan(4, 4, now, &[], &unconfigured);
    assert!(!plan.contains(&NightlyTask::SecondCopy));
    assert!(
        plan.contains(&NightlyTask::IntegrityCheck),
        "the Google Drive copy is checked with or without a second one"
    );
    let done = NightlyState {
        last_second_copy: now - 3600,
        last_integrity_check: now - 3600,
        ..due
    };
    assert!(nightly_plan(4, 4, now, &[], &done).is_empty());
}

/// F259 · a watcher pointed at a path no writer writes to (see
/// REGISTER.md).
#[test]
fn f259_a_watcher_that_matches_no_writer_is_named() {
    let dev = homelab_core::ops::devicebackup::DeviceBackup {
        name: "opnsense".into(),
        url: "https://10.10.10.1/api/core/backup/download/this".into(),
        cred_file: "/var/lib/homelab/secrets/opnsense-backup.conf".into(),
        filename: "config.xml".into(),
        pin: Some("sha256//abc".into()),
        ca_file: None,
    };
    let base = "rclone:gdrive:homelab-backups";

    for path in [
        // The repo root, spelled the way restic spells it.
        format!("{}/opnsense-config", base),
        // The same repo without restic's remote prefix, which is how
        // rclone itself must be given it.
        "gdrive:homelab-backups/opnsense-config".to_string(),
        // And the shape that actually works: the snapshots directory,
        // because the repo root holds `config`, written once at init, so
        // watching the root reports the creation date forever.
        "gdrive:homelab-backups/opnsense-config/snapshots".to_string(),
    ] {
        let w = WatchedBackup {
            name: "opnsense-config".into(),
            rclone_path: path.clone(),
            max_age_hours: 26,
        };
        assert!(
            orphan_watchers(&[w], std::slice::from_ref(&dev), base).is_empty(),
            "{} is the writer's own repository and must not be called an orphan — \
             a guard that cries wolf on a correct config teaches its reader to \
             skip the line",
            path
        );
    }

    let stray = WatchedBackup {
        name: "opnsense-config".into(),
        rclone_path: format!("{}/OPNSense-backups", base),
        max_age_hours: 26,
    };
    let out = orphan_watchers(&[stray], std::slice::from_ref(&dev), base);
    assert_eq!(out.len(), 1, "the live misconfiguration must be named");
    assert!(out[0].contains("OPNSense-backups"), "{:?}", out);

    // With nothing configured to write, every watcher is an orphan and
    // saying so is the point — that was the state for weeks.
    assert_eq!(
        orphan_watchers(&[stray_clone()], &[], base).len(),
        1,
        "no writers at all means the watcher watches nothing"
    );
}

fn stray_clone() -> WatchedBackup {
    WatchedBackup {
        name: "opnsense-config".into(),
        rclone_path: "rclone:gdrive:homelab-backups/OPNSense-backups".into(),
        max_age_hours: 26,
    }
}

#[test]
fn h10_nightly_plan_always_includes_host_meta() {
    // The bug this test was written for: the host-meta backup existed as
    // code but no scheduler path ever reached it, so the vault (holding
    // the ONLY copy of the restic password), state.json and the TLS
    // material were never backed up.
    let now = 1_800_000_000u64;
    let fresh = now - 3600; // backed up an hour ago
    let stale = now - 25 * 3600;

    // No stack is due — the host's own crown jewels still get a snapshot.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[("a".into(), true, fresh)],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: 0,
            last_zfs: now,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta]);

    // Due stacks come first, host-meta closes the run.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[("a".into(), true, stale), ("b".into(), true, stale)],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: 0,
            last_zfs: now,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(
        plan,
        vec![
            NightlyTask::Stack("a".into()),
            NightlyTask::Stack("b".into()),
            NightlyTask::HostMeta
        ]
    );

    // Already snapshotted this run — not repeated on the next 20-min tick.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[("a".into(), true, fresh)],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: fresh,
            last_zfs: now,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert!(plan.is_empty());

    // Outside the night window (fix-129: 04:00-06:00): nothing at all.
    assert!(
        nightly_plan(
            4,
            7,
            now,
            &[("a".into(), true, stale)],
            &NightlyState {
                last_restore_drill: now,
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
            }
        )
        .is_empty()
    );

    // H8: a parked stack sits out, but the host-meta backup does not
    // depend on any stack being active.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[("a".into(), false, stale)],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: 0,
            last_zfs: now,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta]);
}

#[test]
fn e8_zfs_only_when_configured() {
    let now = 1_800_000_000u64;
    let stale = now - 25 * 3600;
    // No jobs declared → the feature is simply off.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: stale,
            last_zfs: stale,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta]);
    // Declared → runs once a night, after the host-meta snapshot.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: stale,
            last_zfs: stale,
            zfs_configured: true,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta, NightlyTask::Zfs]);
    // Already ran this cycle → not repeated on the next 20-min tick.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: stale,
            last_zfs: now - 3600,
            zfs_configured: true,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta]);
}

#[test]
fn y1_a_configured_zero_never_means_no_backups() {
    // The dangerous reading: "0 = unlimited" to a person, "0 = none" to
    // the stream. A night with no backups at all, and the only trace is
    // last_backup standing still.
    assert_eq!(effective_concurrency(0), 1, "zero must not mean none");
    assert_eq!(effective_concurrency(1), 1);
    assert_eq!(effective_concurrency(3), 3, "Kenny's choice, form Y4");
    assert_eq!(effective_concurrency(13), 13);
}

#[test]
fn y1_the_default_is_the_number_kenny_chose() {
    assert_eq!(
        default_backup_concurrency(),
        3,
        "form Y4: about a third of the wait, never more than three \
         services quiet at once"
    );
}

#[test]
fn h12_bearer_check() {
    assert!(bearer_ok(
        Some("Bearer secret-token-123"),
        "secret-token-123"
    ));
    assert!(!bearer_ok(Some("Bearer wrong"), "secret-token-123"));
    assert!(
        !bearer_ok(Some("secret-token-123"), "secret-token-123"),
        "scheme required"
    );
    assert!(!bearer_ok(None, "secret-token-123"));
}

#[test]
fn h16_capacity_numbers_parse_and_sum() {
    let free = "               total        used        free\nMem:           15908        9911        1268\nSwap:           8191         512        7679\n";
    let mut hs = homelab_core::state::HostState::default();
    let mk = |mem: u32| {
        let mut m = homelab_core::manifest::StackManifest {
            home_address_whitelist: None,
            tiles: Default::default(),
            log_files: Vec::new(),
            registry_login: None,
            retention: None,
            data_mounts: Vec::new(),
            native_only: false,
            no_apps_yet: false,
            on_demand: false,
            syslog_receivers: vec![],
            firewall: None,
            natives: Vec::new(),
            stack_name: "x".into(),
            vmid: 108,
            hostname: "108-app-x".into(),
            network: homelab_core::manifest::NetworkSpec {
                ip: "10.10.10.8/24".into(),
                gateway: "g".into(),
                bridge: "b".into(),
                vlan: None,
            },
            resources: homelab_core::manifest::ResourceSpec {
                cores: 2,
                memory_mb: mem,
                swap_mb: 0,
                disk_gb: 4,
                storage: "s".into(),
            },
            lxc: homelab_core::manifest::LxcSpec {
                timezone: "host".into(),
                template: "t".into(),
                unprivileged: true,
                features: String::new(),
                protection: false,
                gpu: false,
                vpn: false,
            },
            boot: homelab_core::manifest::BootSpec {
                onboot: true,
                order: None,
            },
            storage: vec![],
            apps: vec![],
        };
        m.hostname = m.canonical_hostname();
        m
    };
    hs.stacks.insert(
        "a".into(),
        homelab_core::state::StackState {
            applied_source: None,
            vmid: 108,
            hostname: "108-app-a".into(),
            apps: vec![],
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: Some(mk(1024)),
            enabled: true,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
        },
    );
    hs.stacks.insert(
        "b".into(),
        homelab_core::state::StackState {
            applied_source: None,
            vmid: 109,
            hostname: "109-app-b".into(),
            apps: vec![],
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: Some(mk(4096)),
            enabled: true,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
        },
    );
    let (total, used, committed, cores, load1) =
        capacity_numbers(free, "12\n", "2.53 1.80 1.20 2/500 12345", &hs);
    assert_eq!(total, 15908);
    assert_eq!(used, 9911);
    assert_eq!(committed, 5120, "sum of manifest RAM ceilings");
    assert_eq!(cores, 12);
    assert_eq!(load1, 253);
}
