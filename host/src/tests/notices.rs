//! Notices, notification routes and the dashboard watch.

use super::support::*;
use crate::*;

/// fix-126: an `https://` notify route with no pin configured is
/// refused at start, before any notification is ever attempted —
/// `http://` stays accepted (migration is stepwise).
#[test]
pub(crate) fn fix_126_an_https_notify_route_with_no_pin_is_refused_at_start() {
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
pub(crate) fn fix_126_pinned_cacert_checks_the_file_against_the_fingerprint() {
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
pub(crate) async fn fix_143_the_nightly_edge_check_reports_not_configured_without_a_token() {
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

/// fix-184: the down notice must wait for the full `down_after_s` of
/// unbroken failure; a single failed probe (elapsed 0, or anything short
/// of the threshold) must not declare down.
#[test]
pub(crate) fn fix_184_down_notice_waits_for_the_full_threshold() {
    // One failed probe, measured the instant it failed: elapsed is 0.
    assert!(!crate::watch_should_declare_down(0, 300));
    // Still short of five minutes.
    assert!(!crate::watch_should_declare_down(299, 300));
    // Exactly, and past, the threshold.
    assert!(crate::watch_should_declare_down(300, 300));
    assert!(crate::watch_should_declare_down(301, 300));
}

/// fix-184: a planned operation on the stack `watch_url` is declared for
/// suppresses the down timer; an operation on any other stack, or no
/// operation at all, does not — and the check never hardcodes which
/// stack that is, it only compares against `watch_stack`.
#[test]
pub(crate) fn fix_184_outage_expected_is_declared_not_hardcoded() {
    let holder_for = |stack: &str| homelab_core::oplock::Holder {
        what: "deploy".to_string(),
        started_unix: 0,
        done: 0,
        total: 0,
        stack: Some(stack.to_string()),
    };
    // Nothing running: never a known outage.
    assert!(!crate::outage_expected_for(None, Some("dashboard-stack")));
    // The declared stack is deploying: suppressed.
    let busy = holder_for("dashboard-stack");
    assert!(crate::outage_expected_for(
        Some(&busy),
        Some("dashboard-stack")
    ));
    // Some unrelated stack is deploying: not suppressed — this is the
    // generic form a hardcoded `"admin"` literal could never express for
    // an arbitrary configured name.
    let other = holder_for("some-other-stack");
    assert!(!crate::outage_expected_for(
        Some(&other),
        Some("dashboard-stack")
    ));
    // No watch_stack declared at all: a stack-scoped op never suppresses
    // (only a host-wide self-update/restart does).
    assert!(!crate::outage_expected_for(Some(&busy), None));
    // The host's own update or restart always suppresses, regardless of
    // watch_stack.
    let self_update = homelab_core::oplock::Holder {
        what: "self-update".to_string(),
        started_unix: 0,
        done: 0,
        total: 0,
        stack: None,
    };
    assert!(crate::outage_expected_for(Some(&self_update), None));
    assert!(crate::outage_expected_for(
        Some(&self_update),
        Some("dashboard-stack")
    ));
}

/// fix-184: the recovery notice is urgent (reaches `notify_webhook`)
/// only when the outage it closes was itself pushed out; a recovery
/// after a down notice that stayed centre-only (too short-lived, or
/// damped) must not suddenly become the first urgent push of the pair.
#[test]
pub(crate) fn fix_184_recovery_is_urgent_only_if_the_outage_was_pushed() {
    let down = crate::watch_facts(false, 0, "http://10.10.10.20/health", false);
    assert!(down.urgency.urgent, "going down is always urgent");

    let recovery_after_push = crate::watch_facts(true, 0, "http://10.10.10.20/health", true);
    assert!(
        recovery_after_push.urgency.urgent,
        "a recovery closing a pushed outage may be pushed"
    );

    let recovery_without_push = crate::watch_facts(true, 0, "http://10.10.10.20/health", false);
    assert!(
        !recovery_without_push.urgency.urgent,
        "a recovery after an outage nobody was pushed about stays centre-only"
    );
}

/// Decision notify-routing (2026-09-30): every event becomes a notice
/// the dashboard reads after its cursor; only an urgent one tries the
/// push, and what became of it is recorded; the sequence survives a
/// restart.
#[tokio::test]
pub(crate) async fn notify_routing_every_event_is_a_notice_and_only_urgent_pushes() {
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
