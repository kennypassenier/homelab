//! redesign-stackhub (3.71.0, Kenny approved the stack hub demo on
//! 2026-10-03): what the host has to send for the hub to tell the truth —
//! each app's own health check, a real "Env sealed" verdict instead of a
//! hard-coded true, and the seal-env action behind the hub's no-env row.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::OpCtx;
use homelab_core::ops::livestatus::{PROBE, parse_apps, probe};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        metrics_targets_dir: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        default_log_rotation: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    }
}

/// The docker probe reads the same healthcheck expression the deploy's
/// settle check reads, as a fifth field; each app takes its worst
/// container's verdict, an app with no healthcheck has none, and a line
/// from an older four-field probe still parses.
///
/// covers: redesign-stackhub-2
#[test]
fn redesign_stackhub_2_the_status_probe_carries_each_apps_health_check() {
    assert!(
        PROBE.contains("{{if .State.Health}}{{.State.Health.Status}}{{end}}"),
        "the probe asks docker for the healthcheck verdict: {PROBE}"
    );
    let out = "/opt/media/jellyfin|jellyfin|true|0|healthy
/opt/media/arr|sonarr|true|1|healthy
/opt/media/arr|radarr|true|0|unhealthy
/opt/media/arr|prowlarr|true|0|starting
/opt/media/recyclarr|recyclarr|true|0|
/opt/media/old|old|true|2
unit:kyu|kyu|false|3|failed
";
    let a = parse_apps("media", out);
    assert_eq!(a["jellyfin"].health.as_deref(), Some("healthy"));
    assert_eq!(
        a["arr"].health.as_deref(),
        Some("unhealthy"),
        "the worst container's verdict is the app's"
    );
    assert_eq!(a["arr"].containers, 3);
    assert_eq!(
        a["recyclarr"].health, None,
        "no healthcheck declared: no verdict"
    );
    assert_eq!(a["old"].health, None, "an older probe line still parses");
    assert_eq!(a["old"].restarts, 2);
    assert_eq!(
        a["kyu"].health.as_deref(),
        Some("failed"),
        "a native unit's health is what systemctl is-active says"
    );
    assert!(
        probe(&["kyu".to_string()]).contains("$(systemctl is-active kyu 2>/dev/null)"),
        "the unit line carries systemctl is-active as its fifth field"
    );
}

fn kp_soft_state() -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "stacks": {
            "kp-soft": {"vmid": 116, "hostname": "116-app-kp-soft", "apps": ["web", "db"], "applied_at": 1},
            "notes": {"vmid": 117, "hostname": "117-app-notes", "apps": ["app"], "applied_at": 1},
            "gone": {"vmid": 118, "hostname": "118-app-gone", "apps": ["app"], "applied_at": 1}
        }
    })
}

/// "Env sealed" was hard-coded true in the fleet state; only the doctor
/// computed it. The host now computes the same verdict for every stack:
/// a container with a secret file the vault lacks is unsealed, one whose
/// files all have a copy is sealed, and one Proxmox does not know has
/// nothing to seal.
///
/// covers: redesign-stackhub-1
#[tokio::test]
async fn redesign_stackhub_1_the_host_computes_each_stacks_env_sealed_verdict() {
    use homelab_core::ops::facts::sealed_verdicts;
    let exec = MockExecutor::new();
    exec.seed_file("/etc/pve/lxc/116.conf", "hostname: 116-app-kp-soft\n");
    exec.seed_file("/etc/pve/lxc/117.conf", "hostname: 117-app-notes\n");
    exec.respond_always("test -s '/opt/kp-soft/web/.env'", CmdOutput::ok("yes"));
    exec.respond_always("test -s '/opt/kp-soft/db/.env'", CmdOutput::ok(""));
    exec.respond_always("test -s '/opt/notes/app/.env'", CmdOutput::ok("yes"));
    exec.seed_file("/var/lib/homelab/secrets/notes/app.env", "K=1\n");
    let hs: homelab_core::state::HostState = serde_json::from_value(kp_soft_state()).unwrap();
    let v = sealed_verdicts(&exec, "/var/lib/homelab", &hs).await;
    assert_eq!(v.get("kp-soft"), Some(&false), "{v:?}");
    assert_eq!(v.get("notes"), Some(&true), "{v:?}");
    assert_eq!(
        v.get("gone"),
        Some(&true),
        "no container: nothing to seal: {v:?}"
    );
}

/// The hub's "Push the env…" is one host action: every secret file on the
/// container without a vault copy is copied into the vault (owner-only),
/// read without echoing; a file already sealed is left alone, and a second
/// run has nothing to do.
///
/// covers: redesign-stackhub-2
#[tokio::test]
async fn redesign_stackhub_2_seal_env_copies_every_unsealed_secret_file_into_the_vault() {
    use homelab_core::ops::sealenv::seal_env;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::to_string(&kp_soft_state()).unwrap(),
    );
    exec.respond_always(
        "pct config 116",
        CmdOutput::ok("hostname: 116-app-kp-soft\n"),
    );
    exec.respond_always("test -s '/opt/kp-soft/web/.env'", CmdOutput::ok("yes"));
    exec.respond_always("test -s '/opt/kp-soft/db/.env'", CmdOutput::ok("yes"));
    exec.seed_file("/var/lib/homelab/secrets/kp-soft/db.env", "DB=old\n");
    exec.respond_always(
        "cat '/opt/kp-soft/web/.env'",
        CmdOutput::ok("TOKEN=s3cret\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = seal_env(&ctx(&exec, &sink, &j), "kp-soft").await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.file("/var/lib/homelab/secrets/kp-soft/web.env")
            .as_deref(),
        Some("TOKEN=s3cret\n")
    );
    assert_eq!(
        exec.file("/var/lib/homelab/secrets/kp-soft/db.env")
            .as_deref(),
        Some("DB=old\n"),
        "a file that already has a vault copy is not read again"
    );
    assert!(
        exec.calls_containing("cat '/opt/kp-soft/db/.env'")
            .is_empty(),
        "only the unsealed file is read"
    );
    assert!(
        !sink.lines().iter().any(|l| l.contains("s3cret")),
        "the secret never reaches the operation's lines"
    );
    let again = seal_env(&ctx(&exec, &sink, &j), "kp-soft").await;
    assert!(again.ok, "{:?}", again.error);
    assert_eq!(
        exec.calls_containing("cat '/opt/kp-soft/web/.env'").len(),
        1,
        "a second run finds nothing left to seal"
    );
}

/// The action refuses a stack the host does not manage, before it reads
/// anything from a container.
///
/// covers: redesign-stackhub-2
#[tokio::test]
async fn redesign_stackhub_2_seal_env_refuses_an_unknown_stack() {
    use homelab_core::ops::sealenv::seal_env;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::to_string(&kp_soft_state()).unwrap(),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = seal_env(&ctx(&exec, &sink, &j), "nope").await;
    assert!(!report.ok);
    assert!(exec.calls_containing("pct exec").is_empty());
}
