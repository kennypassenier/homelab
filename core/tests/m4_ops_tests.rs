//! M4 operation tests: gated destroy (C2) and backup/restore (E1/E2).

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::backup::{backup, restore, BackupCfg};
use homelab_core::ops::destroy::destroy;
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

fn manifest(vmid: u16, stack: &str) -> StackManifest {
    StackManifest {
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        natives: Vec::new(),
        stack_name: stack.into(),
        vmid,
        hostname: format!("{}-app-{}", vmid, stack),
        network: NetworkSpec {
            ip: format!("10.10.10.{}/24", vmid - 100),
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: Some(10),
        },
        resources: ResourceSpec {
            cores: 1,
            memory_mb: 512,
            swap_mb: 256,
            disk_gb: 4,
            storage: "local-lvm".into(),
        },
        lxc: LxcSpec {
            template: "debian-12".into(),
            unprivileged: true,
            features: "nesting=1".into(),
            protection: true,
            gpu: false,
            vpn: false,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![MountSpec {
            host_path: "/appdata/test/test-config".into(),
            mount_point: "/appdata/test/test-config".into(),
            no_data: false,
            host_owner_uid: Some(101000),
            app: None,
        }],
        apps: vec!["app".into()],
    }
}

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        kea: None,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        backup: Default::default(),
        registry_cache: None,
    }
}

fn mock_hostname(exec: &MockExecutor, vmid: u16, stack: &str) {
    exec.respond_always(
        &format!("pct config {}", vmid),
        CmdOutput::ok(&format!("hostname: {}-app-{}\n", vmid, stack)),
    );
}

// ── C2: gated destroy ───────────────────────────────────────────────────────

#[tokio::test]
async fn c2_destroy_refuses_wrong_typed_name() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        "wrong",
        true,
    )
    .await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("does not match"));
    assert!(
        exec.calls().is_empty(),
        "no commands before name confirmation"
    );
}

#[tokio::test]
async fn c2_destroy_refuses_no_touch_vmid() {
    for vmid in [101u16, 102, 103] {
        let exec = MockExecutor::new();
        let sink = VecSink::new();
        let j = NullJournal;
        let report = destroy(
            &ctx(&exec, &sink, &j),
            &manifest(vmid, "evil"),
            "evil",
            true,
        )
        .await;
        assert!(!report.ok, "vmid {} must be refused", vmid);
        assert!(report.error.unwrap().why.contains("no-touch"));
        assert!(exec.calls_containing("pct destroy").is_empty());
    }
}

#[tokio::test]
async fn c2_destroy_refuses_hostname_mismatch() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 108-app-somethingelse\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(&ctx(&exec, &sink, &j), &manifest(108, "test"), "test", true).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("refusing to destroy"));
    assert!(exec.calls_containing("pct destroy").is_empty());
}

#[tokio::test]
async fn c2_destroy_happy_path_lifts_protection_then_destroys() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 108-app-test\nprotection: 1\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(&ctx(&exec, &sink, &j), &manifest(108, "test"), "test", true).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    // Protection is lifted before destroy; state updated after.
    assert!(pos("--protection 0") < pos("pct destroy 108"));
    assert!(exec.calls_containing("pct destroy 108 --purge").len() == 1);
    // State no longer lists the stack.
    let state = exec.file("/var/lib/homelab/state.json").unwrap_or_default();
    assert!(!state.contains("\"test\""));
}

// ── E1/E2: backup and restore ───────────────────────────────────────────────

#[tokio::test]
async fn e1_backup_runs_init_quiesce_snapshot_resume_retention() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    let sink = VecSink::new();
    let j = NullJournal;
    let cfg = BackupCfg::default();
    let report = backup(&ctx(&exec, &sink, &j), &manifest(108, "test"), &cfg).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(
        pos("restic init") < pos("backup.pause=true"),
        "init before quiesce"
    );
    assert!(
        pos("backup.pause=true") < pos("restic backup"),
        "quiesce before snapshot"
    );
    assert!(
        pos("restic backup") < pos("snapshots --json"),
        "snapshot before retention listing"
    );
    // Snapshot targets the /appdata path.
    assert!(exec
        .calls_containing("/appdata/test/test-config")
        .iter()
        .any(|c| c.contains("restic backup")));
    // Tiered retention: with no snapshots listed, nothing is forgotten
    // (fail-safe: malformed/empty listing keeps everything).
    assert!(exec.calls_containing("restic forget").is_empty());
}

/// W2's acceptance criterion: two stacks with different retention produce
/// demonstrably different forget decisions.
///
/// One fleet-wide policy does not fit stacks that differ by two orders of
/// magnitude. Media needed a shorter one typed by hand because 24 GB a night
/// against the fleet-wide fourteen days would cost half a terabyte, while kyu
/// at 231 MB could comfortably keep two months.
#[tokio::test]
async fn w2_a_stack_keeps_snapshots_by_its_own_policy() {
    use homelab_core::retention::RetentionTier;
    // Six daily snapshots, the newest a day old.
    let day = 86_400u64;
    let now = 1_788_000_000u64;
    let snaps: String = (1..=6)
        .map(|i| {
            format!(
                r#"{{"short_id":"s{}","time":"{}"}}"#,
                i,
                unix_to_rfc3339(now - i * day)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let listing = format!("[{}]", snaps);

    async fn forgotten(tiers: Option<Vec<RetentionTier>>, listing: &str, now: u64) -> String {
        let exec = MockExecutor::new();
        mock_hostname(&exec, 108, "test");
        exec.respond_always("snapshots --json", CmdOutput::ok(listing));
        let sink = VecSink::new();
        let j = NullJournal;
        let mut m = manifest(108, "test");
        m.retention = tiers;
        let mut c = ctx(&exec, &sink, &j);
        c.now_unix = now;
        let report = backup(&c, &m, &BackupCfg::default()).await;
        assert!(report.ok, "{:?}", report.error);
        exec.calls_containing("restic forget").join(" ")
    }

    // Fleet-wide: daily for a week, so six daily snapshots all survive.
    let fleet = forgotten(None, &listing, now).await;
    assert!(
        fleet.is_empty(),
        "the fleet-wide policy keeps a week of dailies: {}",
        fleet
    );

    // This stack's own: keep one every three days. The same six snapshots
    // now produce a forget list.
    let own = forgotten(
        Some(vec![RetentionTier {
            every_days: 3,
            span_days: None,
        }]),
        &listing,
        now,
    )
    .await;
    assert!(
        !own.is_empty(),
        "a tighter per-stack policy must forget something the fleet-wide one keeps"
    );
}

/// RFC3339 in the shape restic emits, without pulling in a date crate.
fn unix_to_rfc3339(t: u64) -> String {
    let days = t / 86_400;
    let rem = t % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm), epoch 1970-01-01.
    let z = days as i64 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// The backup must resume exactly what it paused, not what the manifest
/// happens to list.
///
/// On 2026-08-31 the metrics stack's nightly backup stopped prometheus AND
/// alertmanager (both carry the pause label) and resumed prometheus,
/// promtail and pve-exporter, because host state still held the app list
/// from before alertmanager existed. The snapshot then failed on a stale
/// path, so nothing else touched the stack. Alertmanager stayed down for six
/// hours and nothing reported it.
///
/// So: an app that is paused but NOT in `apps` must still come back.
/// covers: F75
#[tokio::test]
async fn e1_backup_resumes_every_container_it_paused_even_one_not_in_apps() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    // The container list the label filter returns — note `alertmanager` is
    // not among the manifest's apps.
    exec.respond_always(
        "backup.pause=true --format",
        CmdOutput::ok("prometheus\nalertmanager\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let cfg = BackupCfg::default();
    let report = backup(&ctx(&exec, &sink, &j), &manifest(108, "test"), &cfg).await;
    assert!(report.ok, "{:?}", report.error);

    let stopped = exec.calls_containing("docker stop");
    assert!(
        stopped.iter().any(|c| c.contains("alertmanager")),
        "quiesce must stop what the label selects: {:?}",
        stopped
    );
    let started = exec.calls_containing("docker start");
    assert!(
        started.iter().any(|c| c.contains("alertmanager")),
        "resume must start back exactly what was paused: {:?}",
        started
    );
    assert!(
        started.iter().any(|c| c.contains("prometheus")),
        "and the rest of them too: {:?}",
        started
    );
    // Order matters: nothing may be started before the snapshot has run.
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(
        pos("docker stop") < pos("restic backup"),
        "stop before snapshot"
    );
    assert!(
        pos("restic backup") < pos("docker start"),
        "snapshot before start"
    );
}

/// Nothing labelled means nothing stopped and nothing to start back — the
/// step must not invent a bare `docker start` with no arguments.
/// covers: F75
#[tokio::test]
async fn e1_backup_with_nothing_to_pause_starts_nothing() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always("backup.pause=true --format", CmdOutput::ok("\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        &BackupCfg::default(),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        exec.calls_containing("docker start").is_empty(),
        "no containers were paused, so none may be started"
    );
    assert!(
        exec.calls_containing("docker stop").is_empty(),
        "and none may be stopped"
    );
}

/// Every stack dashboard carries its own errors-only section, because Kenny
/// asked for it on every one rather than only on the fleet-wide page — and
/// because a stack deployed next month must get it without anyone
/// remembering to add it.
#[test]
fn every_stack_dashboard_has_an_errors_section() {
    let json = homelab_core::ops::dashboard::dashboard_json(
        "media",
        &["jellyfin".to_string(), "sonarr".to_string()],
    );
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    let panels = v["panels"].as_array().expect("panels");
    let titles: Vec<&str> = panels.iter().filter_map(|p| p["title"].as_str()).collect();
    for want in ["Errors in range", "Errors by container", "Error lines"] {
        assert!(titles.contains(&want), "missing '{}' in {:?}", want, titles);
    }
    // The log panels must read Loki, not the Prometheus datasource the four
    // resource panels use.
    let loki: Vec<&serde_json::Value> = panels
        .iter()
        .filter(|p| p["datasource"]["uid"] == "loki")
        .collect();
    assert_eq!(loki.len(), 3, "three panels should read Loki");

    // The level=info exclusion is load-bearing, not tidiness: Loki logs every
    // query it runs, those queries contain the word "error", and without this
    // Loki counts its own search for errors as an error.
    for p in &loki {
        let expr = p["targets"][0]["expr"].as_str().unwrap_or("");
        assert!(
            expr.contains("!= \"level=info\""),
            "the info exclusion must survive: {}",
            expr
        );
        assert!(
            expr.contains("stack=\"media\""),
            "a stack dashboard must only show its own errors: {}",
            expr
        );
    }
}

/// A restic run over a directory that exists and is empty succeeds, writes a
/// snapshot with nothing in it, and reports success. The record then claims
/// the stack is backed up while a restore would give back nothing.
///
/// A path that does NOT exist already fails loudly — that is how the metrics
/// stack's stale path was caught on 2026-08-31. The empty one is the case
/// nothing catches.
/// An app that declares it keeps nothing gets no repository, and one that
/// declares it and then keeps something fails loudly.
///
/// Kenny's answer to form B4. cloudflared has a mount and an empty directory
/// because the tunnel's configuration lives in the Cloudflare dashboard and
/// nowhere on disk; the empty-snapshot guard refused it — correctly — and
/// stopped the gateway's backup before grafana, loki and goaccess were ever
/// attempted. Saying "this app keeps nothing" in the stack file removes the
/// repository instead of the backup.
/// covers: F154
#[tokio::test]
async fn a_path_declared_empty_is_not_backed_up_and_must_really_be_empty() {
    use homelab_core::ops::backup::owner_groups;
    let mut m = manifest(108, "test");
    m.storage = vec![
        homelab_core::manifest::MountSpec {
            host_path: "/appdata/test/keeper-config".into(),
            mount_point: "/appdata/test/keeper-config".into(),
            no_data: false,
            host_owner_uid: Some(100000),
            app: Some("keeper".into()),
        },
        homelab_core::manifest::MountSpec {
            host_path: "/appdata/test/hollow-config".into(),
            mount_point: "/appdata/test/hollow-config".into(),
            no_data: true,
            host_owner_uid: Some(100000),
            app: Some("hollow".into()),
        },
    ];

    let owners: Vec<String> = owner_groups(&m).into_iter().map(|(o, _)| o).collect();
    assert_eq!(
        owners,
        vec!["keeper".to_string()],
        "a declared-empty app must get no repository at all"
    );

    // And the declaration is held to: a directory that fills up anyway stops
    // the backup with a sentence naming it, because by declaration nothing in
    // it is being saved.
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always("hollow-config' -mindepth 1", CmdOutput::ok("3\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup(&ctx(&exec, &sink, &j), &m, &BackupCfg::default()).await;
    assert!(
        !report.ok,
        "a non-empty declared-empty path is not a success"
    );
    let err = format!("{:?}", report.error);
    assert!(
        err.contains("hollow-config") && err.contains("no_data"),
        "the message must name the path and the declaration: {}",
        err
    );
}

/// Every restic invocation must name a cache directory.
///
/// restic finds its cache through `$XDG_CACHE_HOME` or `$HOME`, and a systemd
/// service has neither — so every backup this fleet has ever taken printed
/// "unable to open cache" and then fetched the whole repository index from
/// Google Drive again. Six repositories on the gateway, six index fetches, on
/// an operation already slow enough to look stuck.
/// covers: F153
#[tokio::test]
async fn every_restic_call_names_a_cache_directory() {
    use homelab_core::ops::backup::RESTIC_CACHE_DIR;
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    let sink = VecSink::new();
    let j = NullJournal;
    let cfg = BackupCfg::default();
    let _ = backup(&ctx(&exec, &sink, &j), &manifest(108, "test"), &cfg).await;

    let restic_calls = exec.calls_containing("RESTIC_REPOSITORY=");
    assert!(
        !restic_calls.is_empty(),
        "the fixture must actually reach restic"
    );
    for c in &restic_calls {
        assert!(
            c.contains(&format!("RESTIC_CACHE_DIR={}", RESTIC_CACHE_DIR)),
            "a restic call without a cache directory re-downloads the index: {}",
            c
        );
    }
}

/// covers: F105
#[test]
fn a_snapshot_that_stored_nothing_is_recognised() {
    use homelab_core::ops::backup::snapshot_is_empty;
    let empty = r#"{"message_type":"summary","total_files_processed":0,"total_bytes_processed":0}"#;
    assert!(snapshot_is_empty(empty), "zero files is empty");

    let real = r#"{"message_type":"status","percent_done":0.5}
{"message_type":"summary","total_files_processed":88,"total_bytes_processed":5998677}"#;
    assert!(!snapshot_is_empty(real), "88 files is not empty");

    // The rule that keeps it from crying wolf: output it does not recognise
    // is NOT called empty. A restic that changes its json would otherwise
    // turn every backup in the fleet into a failure overnight.
    assert!(
        !snapshot_is_empty("Files: 3 new, 0 changed\nAdded to the repository: 4.2 KiB\n"),
        "unrecognised output must never be reported as empty"
    );
    assert!(!snapshot_is_empty(""), "no output is not evidence of empty");
}

#[tokio::test]
async fn e2_restore_validates_quiesces_restores_resumes_verifies() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always(
        "snapshots --last",
        CmdOutput::ok("id  time  host\nabc123  today\n"),
    );
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let cfg = BackupCfg::default();
    let report = restore(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        &cfg,
        "latest",
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(
        pos("snapshots --last") < pos("compose down"),
        "validate before quiesce"
    );
    assert!(
        pos("compose down") < pos("restic restore"),
        "quiesce before restore"
    );
    assert!(
        pos("restic restore latest --target /") < pos("compose up -d"),
        "restore before resume"
    );
}

#[tokio::test]
async fn e2_restore_fails_when_app_not_running_after() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always("snapshots --last", CmdOutput::ok("id\nabc\n"));
    // verify returns empty → app not running.
    exec.respond_always("ps --status running --services", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = restore(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        &BackupCfg::default(),
        "latest",
    )
    .await;
    assert!(!report.ok);
    assert!(report
        .error
        .unwrap()
        .why
        .contains("not running after restore"));
}

// ── D9/B6: managed updates with rollback ────────────────────────────────────

use homelab_core::ops::update::update;

#[tokio::test]
async fn d9_update_capture_pull_verify_order() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always(
        "docker inspect --format",
        CmdOutput::ok("sha256:aaa myimg:latest\n"),
    );
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update(&ctx(&exec, &sink, &j), &manifest(108, "test"), None, false).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(
        pos("docker inspect --format") < pos("compose pull"),
        "capture before pull"
    );
    assert!(
        pos("compose pull") < pos("ps --status running"),
        "pull before verify"
    );
    // No rollback happened on the happy path.
    assert!(exec.calls_containing("docker tag").is_empty());
}

#[tokio::test]
async fn d9_auto_update_skips_non_auto_policy() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    // Policy label query returns "manual".
    exec.respond_always("com.homelab.update.policy", CmdOutput::ok("manual\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update(&ctx(&exec, &sink, &j), &manifest(108, "test"), None, true).await;
    assert!(report.ok);
    assert!(
        exec.calls_containing("compose pull").is_empty(),
        "manual-policy app must not be pulled on a scheduled run"
    );
}

#[tokio::test]
async fn b6_failed_update_rolls_back_to_captured_image() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always(
        "docker inspect --format",
        CmdOutput::ok("sha256:oldimg myimg:latest\n"),
    );
    // First verify after update: not running. Second verify (post-rollback): running.
    exec.enqueue("ps --status running --services", CmdOutput::ok(""));
    exec.enqueue("ps --status running --services", CmdOutput::ok("app\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update(&ctx(&exec, &sink, &j), &manifest(108, "test"), None, false).await;
    // The op FAILS (the new image is bad) but the system is healthy again.
    assert!(!report.ok);
    let why = report.error.unwrap().why;
    assert!(why.contains("ROLLED BACK"), "got: {}", why);
    // The rollback re-tagged the captured image and force-recreated.
    let tag_calls = exec.calls_containing("docker tag sha256:oldimg myimg:latest");
    assert_eq!(tag_calls.len(), 1);
    assert!(tag_calls[0].contains("--force-recreate"));
}

#[tokio::test]
async fn d9_update_unknown_app_refused() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        Some("nope"),
        false,
    )
    .await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("not part of stack"));
}

// ── H5: host self-update ────────────────────────────────────────────────────

use homelab_core::ops::selfupdate::{self_update, SelfUpdateCfg};

#[tokio::test]
async fn h5_selfupdate_verifies_before_touching_current() {
    let exec = MockExecutor::new();
    // Candidate fails selfcheck (wrong arch / truncated upload).
    exec.respond_always("--selfcheck", CmdOutput::failed(1, "exec format error"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = self_update(&ctx(&exec, &sink, &j), &SelfUpdateCfg::default()).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("failed selfcheck"));
    // The running binary was never backed up, replaced, or restarted.
    assert!(exec.calls_containing("cp -a").is_empty());
    assert!(exec.calls_containing("install").is_empty());
    assert!(exec.calls_containing("systemctl").is_empty());
}

#[tokio::test]
async fn h5_selfupdate_happy_path_arms_marker_before_restart() {
    let exec = MockExecutor::new();
    exec.respond_always("--selfcheck", CmdOutput::ok("2.1.0\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = self_update(&ctx(&exec, &sink, &j), &SelfUpdateCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(pos("--selfcheck") < pos("cp -a"), "verify before backup");
    assert!(
        pos("cp -a") < pos("install -m 755"),
        "backup before install"
    );
    assert!(
        pos("install -m 755") < pos("systemd-run"),
        "install before restart"
    );
    // Marker exists and records the new version before the restart fires.
    let marker = exec.file("/var/lib/homelab/selfupdate.pending").unwrap();
    assert!(marker.contains("2.1.0"));
    // Restart is delayed so the RPC reply can flush.
    assert!(exec.calls_containing("--on-active=2").len() == 1);
}

// ── H6: fleet patching ──────────────────────────────────────────────────────

use homelab_core::ops::patch::patch_fleet;

#[tokio::test]
async fn h6_patch_runs_apt_in_each_managed_stack_sequentially() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let targets = vec![("alpha".to_string(), 108u16), ("beta".to_string(), 109u16)];
    let report = patch_fleet(&ctx(&exec, &sink, &j), &targets).await;
    assert!(report.ok, "{:?}", report.error);
    let apt = exec.calls_containing("dist-upgrade");
    assert_eq!(apt.len(), 2);
    assert!(apt[0].contains("pct exec 108"));
    assert!(apt[1].contains("pct exec 109"));
}

#[tokio::test]
async fn h6_patch_never_touches_no_touch_vmids() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    // Poisoned state: a no-touch vmid somehow ended up in the target list.
    let targets = vec![("evil".to_string(), 101u16), ("ok".to_string(), 108u16)];
    let report = patch_fleet(&ctx(&exec, &sink, &j), &targets).await;
    assert!(report.ok);
    let apt = exec.calls_containing("dist-upgrade");
    assert_eq!(apt.len(), 1, "only the managed vmid gets patched");
    assert!(apt[0].contains("pct exec 108"));
}

#[tokio::test]
async fn h6_patch_fails_closed_on_apt_error() {
    let exec = MockExecutor::new();
    exec.enqueue(
        "dist-upgrade",
        CmdOutput::failed(100, "Could not get lock /var/lib/dpkg/lock"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let targets = vec![("alpha".to_string(), 108u16), ("beta".to_string(), 109u16)];
    let report = patch_fleet(&ctx(&exec, &sink, &j), &targets).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("apt failed in alpha"));
    // beta was never attempted (sequential, fail-closed).
    assert!(exec.calls_containing("pct exec 109").is_empty());
}

#[tokio::test]
async fn g8_tiered_retention_forgets_by_explicit_id() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    // Two snapshots on the same old day (age ~100d) → older one forgotten.
    exec.respond_always(
        "snapshots --json",
        CmdOutput::ok(
            r#"[{"short_id":"old1","time":"2026-05-03T04:00:00Z"},
                {"short_id":"old2","time":"2026-05-03T09:00:00Z"},
                {"short_id":"new1","time":"2026-08-11T04:00:00Z"}]"#,
        ),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.now_unix = 1_786_428_000; // 2026-08-11
    let report = backup(&c, &manifest(108, "test"), &BackupCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let forgets = exec.calls_containing("restic forget");
    assert_eq!(forgets.len(), 1);
    assert!(
        forgets[0].contains("old1"),
        "older same-bucket snapshot forgotten"
    );
    assert!(!forgets[0].contains("new1"), "newest never forgotten");
    assert!(forgets[0].contains("--prune"));
}

// ── B4: drift detection building blocks ─────────────────────────────────────

#[test]
fn b4_intent_hash_changes_with_any_file_edit() {
    use homelab_core::manifest::{intent_hash, DeploySpec, FileBlob};
    let base = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let h1 = intent_hash(&base);
    assert_eq!(h1, intent_hash(&base), "deterministic");
    let mut edited = base.clone();
    edited.files[0].content.push('\n');
    assert_ne!(h1, intent_hash(&edited), "one byte flips the hash");
    let mut env_changed = base.clone();
    env_changed.env.insert("app".into(), "SECRET=x".into());
    assert_ne!(h1, intent_hash(&env_changed), "env is part of intent");
}

// ── H4: hardware passthrough flags ──────────────────────────────────────────

#[tokio::test]
async fn h4_gpu_and_vpn_flags_produce_device_config() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    // qm status must fail (vmid is not a VM), pct config fails (not existing) → create path.
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    exec.seed_file(
        "/etc/pve/lxc/108.conf",
        "arch: amd64\nhostname: 108-app-test\n",
    );
    // W1: the host is asked what it has. This test used to assert the
    // literals 44 and 104 — the numbers the code carried — which made it a
    // test of the bug: measured on the real Proxmox host, renderD128 is
    // group 993, not 104 (F110).
    exec.respond_always(
        "stat -c %g",
        CmdOutput::ok(
            "/dev/dri/card0 44\n/dev/dri/renderD128 993\n/dev/net/tun 0\ndri: card0 renderD128 \n",
        ),
    );
    let mut m = manifest(108, "test");
    m.lxc.gpu = true;
    m.lxc.vpn = true;
    let spec = DeploySpec {
        manifest: m,
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    // GPU: exact targeted dev entries with the gids the host reported —
    // never chmod, and never a number this code invented.
    let dev = exec.calls_containing("--dev0 /dev/dri/card0,gid=44");
    assert_eq!(dev.len(), 1);
    assert!(dev[0].contains("--dev1 /dev/dri/renderD128,gid=993"));
    assert!(
        exec.calls_containing("chmod").is_empty(),
        "no ansible chmod-recurse bug"
    );
    // VPN: raw lxc lines appended to the container config.
    let conf = exec.file("/etc/pve/lxc/108.conf").unwrap();
    assert!(conf.contains("lxc.cgroup2.devices.allow: c 10:200 rwm"));
    assert!(conf.contains("lxc.mount.entry: /dev/net/tun dev/net/tun none bind,create=file"));
}

#[tokio::test]
async fn h4_no_flags_no_device_config() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let spec = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec.calls_containing("--dev0").is_empty());
}

// ── A6: exec guard ──────────────────────────────────────────────────────────

#[test]
fn a6_exec_guard_deny_by_default_and_no_touch_always() {
    use homelab_core::safety::exec_guard;
    let cfg = SafetyConfig::default();
    // Disabled (the default): refused even for a managed vmid.
    assert!(exec_guard(false, &cfg, 108).is_err());
    // Enabled: managed vmid allowed.
    assert!(exec_guard(true, &cfg, 108).is_ok());
    // Enabled: no-touch vmids refused regardless.
    for vmid in [101u16, 102, 103] {
        let err = exec_guard(true, &cfg, vmid).unwrap_err();
        assert!(format!("{}", err).contains("no-touch"));
    }
}

// ── D5: mirror push ─────────────────────────────────────────────────────────

#[tokio::test]
async fn d5_mirror_adds_remote_once_and_pushes() {
    use homelab_core::ops::mirror::mirror_push;
    let exec = MockExecutor::new();
    exec.enqueue(
        "remote get-url mirror",
        CmdOutput::failed(2, "no such remote"),
    );
    mirror_push(&exec, "/var/lib/homelab/repo", "git@github.com:k/m.git")
        .await
        .unwrap();
    assert_eq!(
        exec.calls_containing("remote add mirror git@github.com:k/m.git")
            .len(),
        1
    );
    assert_eq!(exec.calls_containing("push --quiet mirror --all").len(), 1);
    // Second run: remote exists, no re-add.
    mirror_push(&exec, "/var/lib/homelab/repo", "git@github.com:k/m.git")
        .await
        .unwrap();
    assert_eq!(
        exec.calls_containing("remote add").len(),
        1,
        "remote added once"
    );
}

#[tokio::test]
async fn d5_mirror_push_failure_is_an_error_not_a_panic() {
    use homelab_core::ops::mirror::mirror_push;
    let exec = MockExecutor::new();
    exec.respond_always(
        "push --quiet mirror",
        CmdOutput::failed(128, "could not resolve host"),
    );
    let err = mirror_push(&exec, "/r", "git@x:y.git").await.unwrap_err();
    assert!(format!("{}", err).contains("could not resolve host"));
}

// ── B8: golden template ─────────────────────────────────────────────────────

use homelab_core::ops::template::{build_template, TemplateCfg};

#[tokio::test]
async fn b8_template_build_owns_only_its_temp_vmid() {
    let exec = MockExecutor::new();
    exec.enqueue("pct config 999", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = build_template(&ctx(&exec, &sink, &j), &TemplateCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    // Every mutating pct call targets vmid 999 and nothing else.
    for call in exec.calls_containing("pct ") {
        if call.contains("create")
            || call.contains("stop")
            || call.contains("template")
            || call.contains("start")
            || call.contains("set")
        {
            assert!(call.contains("999"), "stray vmid in: {}", call);
        }
    }
    assert_eq!(exec.calls_containing("pct template 999").len(), 1);
    // Machine identity is scrubbed before conversion.
    assert!(!exec.calls_containing("rm -f /etc/machine-id").is_empty());
}

#[tokio::test]
async fn b8_template_build_refuses_no_touch_and_existing_vmids() {
    let sink = VecSink::new();
    let j = NullJournal;
    // No-touch vmid refused before anything runs.
    let exec = MockExecutor::new();
    let cfg = TemplateCfg {
        temp_vmid: 102,
        ..Default::default()
    };
    let report = build_template(&ctx(&exec, &sink, &j), &cfg).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("no-touch"));
    assert!(exec.calls_containing("pct create").is_empty());
    // Existing vmid refused (would destroy someone's container).
    let exec = MockExecutor::new();
    exec.respond_always("pct config 999", CmdOutput::ok("hostname: something\n"));
    let report = build_template(&ctx(&exec, &sink, &j), &TemplateCfg::default()).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("already exists"));
}

#[tokio::test]
async fn b8_clone_template_provisions_via_pct_clone() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    // The golden template is unprivileged, which is what this stack asks for.
    exec.respond_always("pct config 999", CmdOutput::ok("unprivileged: 1"));
    let mut m = manifest(108, "test");
    m.lxc.template = "clone:999".into();
    m.resources.disk_gb = 8;
    let spec = DeploySpec {
        manifest: m,
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(exec.calls_containing("pct clone 999 108").len(), 1);
    assert!(
        exec.calls_containing("pct create").is_empty(),
        "no full create on clone path"
    );
    assert_eq!(exec.calls_containing("pct resize 108 rootfs 8G").len(), 1);
    // Docker probe succeeded → bootstrap skipped the install.
    assert!(exec.calls_containing("get.docker.com").is_empty());
}

// ── C4: hot-apply resources ─────────────────────────────────────────────────

use homelab_core::ops::resize::hot_apply;

fn resize_exec(mem: u32, cores: u32, disk: u32, running: bool) -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok(&format!(
            "hostname: 108-app-test\nmemory: {}\ncores: {}\nrootfs: local-lvm:vm-108-disk-0,size={}G\n",
            mem, cores, disk
        )),
    );
    exec.respond_always(
        "pct status",
        CmdOutput::ok(if running {
            "status: running"
        } else {
            "status: stopped"
        }),
    );
    exec
}

#[tokio::test]
async fn c4_grow_applies_live() {
    let exec = resize_exec(512, 1, 4, true);
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = manifest(108, "test");
    m.resources.memory_mb = 1024;
    m.resources.cores = 2;
    m.resources.disk_gb = 8;
    let report = hot_apply(&ctx(&exec, &sink, &j), &m).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(exec.calls_containing("--memory 1024 --cores 2").len(), 1);
    assert_eq!(exec.calls_containing("pct resize 108 rootfs 8G").len(), 1);
}

#[tokio::test]
async fn c4_shrink_refused_while_running() {
    let exec = resize_exec(2048, 4, 8, true);
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = manifest(108, "test");
    m.resources.memory_mb = 512;
    m.resources.cores = 1;
    let report = hot_apply(&ctx(&exec, &sink, &j), &m).await;
    assert!(!report.ok);
    assert!(report
        .error
        .unwrap()
        .why
        .contains("shrink refused while running"));
    assert!(exec.calls_containing("pct set 108 --memory").is_empty());
}

#[tokio::test]
async fn c4_ram_shrink_allowed_when_stopped_but_disk_never() {
    // Stopped: RAM/cores may shrink…
    let exec = resize_exec(2048, 4, 8, false);
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = manifest(108, "test");
    m.resources.memory_mb = 512;
    m.resources.cores = 1;
    m.resources.disk_gb = 8;
    let report = hot_apply(&ctx(&exec, &sink, &j), &m).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(exec.calls_containing("--memory 512").len(), 1);
    // …but a disk shrink is refused even stopped.
    let exec = resize_exec(512, 1, 8, false);
    let mut m = manifest(108, "test");
    m.resources.disk_gb = 4;
    let report = hot_apply(&ctx(&exec, &sink, &j), &m).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("disk shrink refused"));
}

#[tokio::test]
async fn c4_no_touch_and_hostname_guarded() {
    let sink = VecSink::new();
    let j = NullJournal;
    let exec = resize_exec(512, 1, 4, true);
    let report = hot_apply(&ctx(&exec, &sink, &j), &manifest(102, "evil")).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("no-touch"));
    // Wrong hostname refused too.
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 108-app-other\nmemory: 512\ncores: 1\n"),
    );
    let report = hot_apply(&ctx(&exec, &sink, &j), &manifest(108, "test")).await;
    assert!(!report.ok);
}

// ── H2: Kea DHCP reservations ───────────────────────────────────────────────

fn ctx_with_kea<'a>(
    exec: &'a MockExecutor,
    sink: &'a VecSink,
    journal: &'a NullJournal,
) -> OpCtx<'a> {
    let mut c = ctx(exec, sink, journal);
    c.kea = Some(homelab_core::ops::kea::KeaCfg {
        base_url: "https://10.10.10.1".into(),
        cred_file: "/var/lib/homelab/secrets/opnsense".into(),
    });
    c
}

#[tokio::test]
async fn h2_deploy_registers_kea_reservation_on_create() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist")); // safety gate
    exec.respond_always(
        "pct config 108",
        CmdOutput::ok(
            "hostname: 108-app-test\nnet0: name=eth0,hwaddr=BC:24:11:AA:BB:CC,ip=10.10.10.8/24\n",
        ),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    exec.respond_always(
        "search_subnet",
        CmdOutput::ok(r#"{"rows":[{"uuid":"sub-1","subnet":"10.10.10.0/24"}]}"#),
    );
    exec.respond_always("search_reservation", CmdOutput::ok(r#"{"rows":[]}"#));
    exec.respond_always("add_reservation", CmdOutput::ok(r#"{"result":"saved"}"#));
    exec.respond_always("reconfigure", CmdOutput::ok(r#"{"status":"ok"}"#));
    let spec = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx_with_kea(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    let adds = exec.calls_containing("add_reservation");
    assert_eq!(adds.len(), 1);
    assert!(adds[0].contains("10.10.10.8"));
    assert!(adds[0].contains("BC:24:11:AA:BB:CC"));
    assert!(adds[0].contains("sub-1"));
    assert_eq!(exec.calls_containing("service/reconfigure").len(), 1);
    // The secret is read inside the shell, never in argv.
    assert!(adds[0].contains("$(cat /var/lib/homelab/secrets/opnsense)"));
}

#[tokio::test]
async fn h2_kea_failure_never_blocks_deploy() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config 108",
        CmdOutput::ok("hostname: 108-app-test\nnet0: name=eth0,hwaddr=BC:24:11:AA:BB:CC\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    // OPNsense down.
    exec.respond_always("search_subnet", CmdOutput::failed(7, "connection refused"));
    let spec = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx_with_kea(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "kea failure must not block: {:?}", report.error);
}

#[tokio::test]
async fn h2_no_kea_config_no_api_calls() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    let spec = DeploySpec {
        manifest: manifest(108, "test"),
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok);
    assert!(exec.calls_containing("api/kea").is_empty());
}

// ── V8: undeclared /appdata bind is a validation error ─────────────────────

#[test]
fn v8_validate_rejects_undeclared_appdata_bind() {
    use homelab_core::manifest::{validate, DeploySpec, FileBlob};
    let mut m = manifest(108, "test");
    m.storage.clear(); // nothing declared
    let spec = DeploySpec {
        manifest: m,
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services:\n  app:\n    volumes:\n      - /appdata/test/app-config:/config\n"
                .into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    };
    let err = validate(&spec).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("not declared under storage"), "got: {}", msg);
    // Declaring it fixes the spec.
    let mut ok_spec = spec;
    ok_spec.manifest.storage = vec![homelab_core::manifest::MountSpec {
        host_path: "/appdata/test/app-config".into(),
        mount_point: "/appdata/test/app-config".into(),
        no_data: false,
        host_owner_uid: Some(101000),
        app: None,
    }];
    validate(&ok_spec).unwrap();
}

// ── A1 property: every no-touch vmid refused in EVERY mutating op ───────────

#[tokio::test]
async fn a1_property_every_no_touch_vmid_refused_in_every_op() {
    use homelab_core::safety::DEFAULT_NO_TOUCH;
    for &vmid in DEFAULT_NO_TOUCH {
        let m = manifest(vmid, "evil");
        let sink = VecSink::new();
        let j = NullJournal;
        // backup
        let exec = MockExecutor::new();
        let r = backup(&ctx(&exec, &sink, &j), &m, &BackupCfg::default()).await;
        assert!(!r.ok, "backup must refuse vmid {}", vmid);
        assert!(exec.calls_containing("restic").is_empty());
        // restore
        let exec = MockExecutor::new();
        let r = restore(&ctx(&exec, &sink, &j), &m, &BackupCfg::default(), "latest").await;
        assert!(!r.ok, "restore must refuse vmid {}", vmid);
        assert!(exec.calls_containing("compose down").is_empty());
        // update
        let exec = MockExecutor::new();
        let r = update(&ctx(&exec, &sink, &j), &m, None, false).await;
        assert!(!r.ok, "update must refuse vmid {}", vmid);
        assert!(exec.calls_containing("compose pull").is_empty());
    }
}

/// A1: the no-touch list is a policy Kenny stated, not a convenience default,
/// so it is pinned here — widening or narrowing it must break a test rather
/// than pass review. Decided 2026-08-30 (deployment project, Phase 0 gate C5):
/// VM 100 OPNsense, CT 102 omada and CT 103 fileserver are untouchable under
/// every circumstance; VM 101 Home Assistant keeps its VM lifecycle out of the
/// orchestrator's hands (its in-app config changes go through the HA API with
/// explicit consent, which this list does not govern). Every other LXC comes
/// under management as the deployment project integrates it.
#[test]
fn a1_no_touch_list_is_exactly_the_four_untouchable_guests() {
    use homelab_core::safety::DEFAULT_NO_TOUCH;
    assert_eq!(DEFAULT_NO_TOUCH, &[100u16, 101, 102, 103]);
}

#[tokio::test]
async fn a2_hostname_mismatch_refused_in_backup_restore_update() {
    let sink = VecSink::new();
    let j = NullJournal;
    let m = manifest(108, "test");
    for op in ["backup", "restore", "update"] {
        let exec = MockExecutor::new();
        exec.respond_always("pct config 108", CmdOutput::ok("hostname: 108-app-other\n"));
        let r = match op {
            "backup" => backup(&ctx(&exec, &sink, &j), &m, &BackupCfg::default()).await,
            "restore" => restore(&ctx(&exec, &sink, &j), &m, &BackupCfg::default(), "latest").await,
            _ => update(&ctx(&exec, &sink, &j), &m, None, false).await,
        };
        assert!(!r.ok, "{} must refuse a hostname mismatch", op);
        assert!(r.error.unwrap().why.contains("refusing"));
    }
}

// ── H2 hardening: resume always runs; stale locks cleared ───────────────────

#[tokio::test]
async fn h2_failed_snapshot_still_resumes_containers() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always(
        "restic backup",
        CmdOutput::failed(1, "rclone: upload timeout"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        &BackupCfg::default(),
    )
    .await;
    assert!(!report.ok, "snapshot failure must still fail the op");
    // …but the paused containers were restarted anyway.
    assert!(
        !exec.calls_containing("docker compose up -d").is_empty(),
        "resume must run even when the snapshot fails"
    );
    // And stale locks were cleared up front.
    assert_eq!(exec.calls_containing("restic unlock").len(), 1);
}

// ── E3 auto-restore, D3 garbage collection, H6 host-side routes ─────────────

fn deploy_mocks(exec: &MockExecutor) {
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    // O5: the clone path reads the template's privilege level and refuses a
    // mismatch, so every deploy that clones needs a template to describe.
    exec.respond_always("pct config 999", CmdOutput::ok("unprivileged: 1"));
}

fn deploy_spec(m: StackManifest) -> homelab_core::manifest::DeploySpec {
    homelab_core::manifest::DeploySpec {
        manifest: m,
        files: vec![homelab_core::manifest::FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    }
}

#[tokio::test]
async fn e3_empty_dirs_with_snapshot_trigger_restore() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    // ls -A returns empty (dir empty); a snapshot exists.
    exec.respond_always(
        "snapshots --last --json",
        CmdOutput::ok(r#"[{"short_id":"abc"}]"#),
    );
    exec.respond_always("restic restore", CmdOutput::ok("restored"));
    let spec = deploy_spec(manifest(108, "test"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.calls_containing("restic restore latest --target /")
            .len(),
        1
    );
}

#[tokio::test]
async fn e3_nonempty_dirs_skip_restore_and_restic_failure_never_blocks() {
    use homelab_core::ops::deploy::deploy;
    // Non-empty: no restore attempted.
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(manifest(108, "test"))).await;
    assert!(report.ok);
    assert!(exec.calls_containing("restic restore").is_empty());
    // Empty + restic restore fails: loud warning, deploy still green.
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always(
        "snapshots --last --json",
        CmdOutput::ok(r#"[{"short_id":"abc"}]"#),
    );
    exec.respond_always("restic restore", CmdOutput::failed(1, "gdrive down"));
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(manifest(108, "test"))).await;
    assert!(
        report.ok,
        "restic failure must not block: {:?}",
        report.error
    );
    assert!(sink
        .lines()
        .iter()
        .any(|l| l.contains("AUTO-RESTORE FAILED")));
}

/// T40: `data_dirs` may only be empty when the service says so. kyu-runner is
/// deliberately stateless — its own unit file says "no state directory, no
/// disk to protect" and it runs under DynamicUser — so refusing it outright
/// forced a fabricated directory that would then be backed up for nothing.
#[test]
fn t40_stateless_must_be_declared_not_inferred() {
    use homelab_core::native::{validate_native, NativeServiceManifest};
    let base = NativeServiceManifest {
        stack_name: "kyu".into(),
        vmid: 109,
        hostname: "109-app-kyu".into(),
        unit: "kyu-runner".into(),
        binary: "/usr/local/bin/kyu-runner".into(),
        env_file: Some("/etc/kyu-runner/token.env".into()),
        data_dirs: vec![],
        update_cmd: None,
        stateless: false,
    };
    // Undeclared: still refused, and the message points at the way out.
    let problems = validate_native(&base).expect_err("empty data_dirs must be refused");
    assert!(
        problems.iter().any(|p| p.contains("stateless")),
        "the refusal must name the escape: {:?}",
        problems
    );
    // Declared: accepted.
    let stateless = NativeServiceManifest {
        stateless: true,
        ..base.clone()
    };
    validate_native(&stateless).expect("a declared-stateless service is valid");
    // Both at once is a contradiction, and guessing would silently decide
    // whether the service is backed up.
    let confused = NativeServiceManifest {
        stateless: true,
        data_dirs: vec!["/var/lib/x".into()],
        ..base
    };
    let problems = validate_native(&confused).expect_err("contradiction must be refused");
    assert!(problems
        .iter()
        .any(|p| p.contains("one of the two is wrong")));
}

/// T1: a stack becomes a scrape target because it was deployed, not because
/// somebody remembered to edit a list. Eleven node addresses and six cadvisor
/// addresses were hardcoded in prometheus.yml, and nothing kept them honest —
/// the scratch container at 10.10.10.14 was still a target this morning, on
/// its way to firing HostDown the moment it was removed.
#[tokio::test]
async fn t1_deploy_writes_a_discovery_file_and_destroy_removes_it() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.metrics_targets_dir = Some("/appdata/metrics/prometheus-config/targets".into());
    let report = deploy(&c, &deploy_spec(manifest(108, "test"))).await;
    assert!(report.ok, "{:?}", report.error);

    let written = exec
        .file_paths()
        .into_iter()
        .find(|p| p.contains("targets/test.json"))
        .expect("a discovery file must be written");
    assert_eq!(
        written,
        "/appdata/metrics/prometheus-config/targets/test.json"
    );

    // The CIDR from the manifest must not reach the scrape target.
    let body = homelab_core::ops::discovery::targets_json("test", "10.10.10.8/24", true);
    assert!(body.contains("10.10.10.8:9100"), "{}", body);
    assert!(body.contains("10.10.10.8:8081"), "{}", body);
    assert!(
        !body.contains("/24"),
        "the CIDR suffix must be stripped: {}",
        body
    );

    // Rewriting an unchanged file must be a no-op, or the drift check would
    // report a difference every single deploy.
    assert_eq!(
        homelab_core::ops::discovery::targets_json("test", "10.10.10.8/24", true),
        body
    );

    // A native-service stack has no docker, so it must not be given a
    // cadvisor target. Measured on this fleet: kyu (CT 109) and almanac
    // (CT 112) answer on 9100 and refuse 8081, so writing one would hand
    // Prometheus an endpoint that can never come up and Alertmanager a rule
    // that can never clear.
    let native = homelab_core::ops::discovery::targets_json("kyu", "10.10.10.9/24", false);
    assert!(native.contains("10.10.10.9:9100"), "{}", native);
    assert!(
        !native.contains("8081"),
        "a stack without docker must get no cadvisor target: {}",
        native
    );
    // Still valid JSON with exactly one entry.
    assert_eq!(native.matches("\"targets\"").count(), 1, "{}", native);
    assert!(!native.contains(",\n  {"), "no dangling comma: {}", native);
}

/// O10 end to end: an app that asks to be left alone while in use is skipped,
/// and nothing is pulled or restarted. The check fails closed, so this also
/// covers the case that matters most — Jellyfin answering with something the
/// orchestrator cannot read.
#[tokio::test]
async fn o10_an_app_in_use_is_skipped_entirely() {
    use homelab_core::ops::update::update;
    for (label, body) in [
        (
            "somebody watching",
            r#"[{"UserName":"kenny","NowPlayingItem":{"Name":"Arrival"},"PlayState":{"IsPaused":true}}]"#,
        ),
        ("an unreadable answer", "<html>502</html>"),
    ] {
        let exec = MockExecutor::new();
        exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
        exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test"));
        exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
        exec.respond_always("update.policy", CmdOutput::ok("auto\n"));
        exec.respond_always("update.busy-check", CmdOutput::ok("jellyfin\n"));
        exec.respond_always("Sessions", CmdOutput::ok(body));
        let sink = VecSink::new();
        let j = NullJournal;
        let mut m = manifest(108, "test");
        m.apps = vec!["app".into()];
        let report = update(&ctx(&exec, &sink, &j), &m, None, false).await;
        assert!(
            report.ok,
            "a skip is not a failure ({}): {:?}",
            label, report.error
        );
        assert!(
            exec.calls_containing("docker compose pull").is_empty(),
            "{}: nothing may be pulled: {:?}",
            label,
            exec.calls_containing("docker compose")
        );
        assert!(
            sink.lines().iter().any(|l| l.contains("[o10]")),
            "{}: the skip must say so out loud",
            label
        );
    }
}

/// O9: a database is stopped cleanly before its replacement comes up, not
/// killed under itself. `docker compose up -d` recreates a container by
/// killing it, and for Postgres — which SuperSync runs on CT 111 — that makes
/// the next start a recovery. The label mirrors `com.homelab.backup.pause`,
/// which already does exactly this for backups.
///
/// The order matters as much as the stop: pull first, then stop, then start,
/// so the service is down for the swap rather than for the download.
#[tokio::test]
async fn o9_stop_first_happens_between_the_pull_and_the_up() {
    use homelab_core::ops::update::update;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    exec.respond_always("update.policy", CmdOutput::ok("auto\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = manifest(108, "test");
    m.apps = vec!["app".into()];
    let report = update(&ctx(&exec, &sink, &j), &m, None, false).await;
    assert!(report.ok, "{:?}", report.error);

    let calls = exec.calls();
    let idx = |needle: &str| {
        calls
            .iter()
            .position(|c| c.contains(needle))
            .unwrap_or_else(|| panic!("{} never ran, calls: {:?}", needle, calls))
    };
    let pull = idx("docker compose pull");
    let stop = idx("com.homelab.update.stop-first");
    let up = idx("docker compose up -d --remove-orphans");
    assert!(
        pull < stop && stop < up,
        "order must be pull, stop, up — got pull={} stop={} up={}",
        pull,
        stop,
        up
    );
    // A clean stop, not a kill: the default 10 s is not enough for a database
    // to finish a checkpoint.
    assert!(
        calls.iter().any(|c| c.contains("docker stop -t 60")),
        "the stop must give the service time to shut down: {:?}",
        calls
    );
}

/// O2/M3: two golden templates, one privileged and one not. `pct clone` cannot
/// change a privilege level, so a container that must be privileged — CT 105
/// and 106 are — has to be cloned from a privileged template. The name has to
/// say which is which, because nothing else about a template does, and getting
/// it wrong produces a container that fails on permissions much later.
#[tokio::test]
async fn o2_two_templates_differ_in_privilege_and_in_name() {
    use homelab_core::ops::template::{build_template, TemplateCfg};
    for unprivileged in [true, false] {
        let exec = MockExecutor::new();
        exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
        exec.respond_always("pct config", CmdOutput::failed(2, "does not exist"));
        exec.respond_always("is-system-running", CmdOutput::ok("running"));
        let sink = VecSink::new();
        let j = NullJournal;
        let cfg = TemplateCfg {
            temp_vmid: 997,
            unprivileged,
            ..Default::default()
        };
        let report = build_template(&ctx(&exec, &sink, &j), &cfg).await;
        assert!(report.ok, "{:?}", report.error);

        let create = exec.calls_containing("pct create").join(" ");
        let expected = if unprivileged {
            "--unprivileged 1"
        } else {
            "--unprivileged 0"
        };
        assert!(
            create.contains(expected),
            "expected {} in: {}",
            expected,
            create
        );
        let described = exec
            .calls_containing("golden template (B8), clone with")
            .join(" ");
        if unprivileged {
            assert!(!described.contains("-priv"), "{}", described);
        } else {
            assert!(
                described.contains("-priv"),
                "the name must say so: {}",
                described
            );
        }
    }
}

/// O2: the agents that were installed by hand on six hosts come from the
/// template now. A container added after that day measured nothing until
/// somebody noticed, which is the failure this removes.
#[tokio::test]
async fn o2_the_template_bakes_the_observability_agents() {
    use homelab_core::ops::template::{build_template, TemplateCfg};
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.respond_always("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = build_template(
        &ctx(&exec, &sink, &j),
        &TemplateCfg {
            temp_vmid: 997,
            ..Default::default()
        },
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let all = exec.calls().join(" ");
    assert!(
        all.contains("prometheus-node-exporter"),
        "node_exporter must be baked in"
    );
    // Measured on the first v2 template: deleting the ssh host keys without
    // arranging for new ones left sshd FAILED on every clone. Kenny reaches
    // these containers by ssh, so this is not cosmetic.
    assert!(
        all.contains("ssh-host-keys.service") && all.contains("ssh-keygen -A"),
        "a clone must generate its own ssh host keys"
    );
    // A template that boots `degraded` makes "degraded" useless as a signal,
    // and the deploy's wait-for-systemd loop accepts exactly that word.
    assert!(
        all.contains("mask nvmf-autoconnect.service openipmi.service"),
        "hardware services an LXC cannot satisfy must be masked"
    );
    assert!(
        all.contains("purge -y -qq postfix"),
        "no mail daemon on every container"
    );
    assert!(
        all.contains("cadvisor"),
        "cadvisor image must be pre-pulled"
    );
    assert!(
        all.contains("promtail"),
        "promtail image must be pre-pulled"
    );
}

/// T2: a stack brings its own dashboard. The ones that exist today were built
/// by hand and lived in no repository until 2026-08-30, so a Grafana rebuild
/// would have taken them — and adding a stack meant remembering to open
/// Grafana, which is the step nobody remembers.
#[tokio::test]
async fn t2_deploy_provisions_a_dashboard_for_the_stack() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.grafana_dashboards_dir = Some("/opt/grafana/provisioning/dashboards".into());
    let report = deploy(&c, &deploy_spec(manifest(108, "test"))).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        !exec
            .calls_containing("/opt/grafana/provisioning/dashboards/homelab-test.json")
            .is_empty(),
        "the dashboard must be pushed to the gateway: {:?}",
        exec.calls_containing("grafana")
    );
}

/// The generated document has to be worth provisioning: real panels, the
/// Prometheus datasource by uid, and every query scoped to this stack so two
/// stacks never show each other's numbers.
#[test]
fn t2_the_generated_dashboard_is_scoped_and_stable() {
    use homelab_core::ops::dashboard::dashboard_json;
    let body = dashboard_json("media", &["jellyfin".to_string(), "sonarr".to_string()]);
    assert!(body.contains("\"uid\": \"homelab-media\""), "{}", body);
    assert!(body.contains("\"uid\": \"prometheus\""), "{}", body);
    // Assert what the sentence says, rather than a count that has to be
    // edited every time a panel is added. The count version broke the moment
    // the errors section arrived — which is the test doing its job, but it
    // was checking a number instead of the property it was written for.
    let v: serde_json::Value = serde_json::from_str(&body).expect("valid json");
    let exprs: Vec<String> = v["panels"]
        .as_array()
        .expect("panels")
        .iter()
        .flat_map(|p| p["targets"].as_array().cloned().unwrap_or_default())
        .filter_map(|t| t["expr"].as_str().map(|e| e.to_string()))
        .collect();
    assert!(
        !exprs.is_empty(),
        "a dashboard with no queries is decoration"
    );
    for e in &exprs {
        assert!(
            e.contains("stack=\"media\""),
            "every query must be scoped to this stack, this one is not: {}",
            e
        );
    }
    assert!(
        body.contains("jellyfin, sonarr"),
        "the description should say what is in the stack: {}",
        body
    );
    assert!(
        body.contains("change the generator, not the dashboard"),
        "an overwritten file must say so on its face"
    );
    // Byte-stable, or the fleet check reports drift after every deploy.
    assert_eq!(
        body,
        dashboard_json("media", &["jellyfin".to_string(), "sonarr".to_string()])
    );
}

/// T1: with no directory configured the feature is simply off, and a deploy
/// neither writes nor complains.
#[tokio::test]
async fn t1_discovery_is_off_when_unconfigured() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(manifest(108, "test"))).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec.file_paths().iter().all(|p| !p.contains("targets/")));
}

/// D25: one restic repository per APP, not per stack. Before this the
/// repository was named after the stack, so moving an app to another stack
/// left its whole history behind and started it from nothing — exactly the
/// "gedoe met backups" Kenny asked to be rid of.
#[tokio::test]
async fn d25_backup_writes_one_repo_per_owning_app() {
    use homelab_core::ops::backup::{backup, BackupCfg};
    let mut m = manifest(108, "test");
    m.apps = vec!["alpha".into(), "beta".into()];
    m.storage = vec![
        homelab_core::manifest::MountSpec {
            host_path: "/appdata/test/alpha-config".into(),
            mount_point: "/appdata/test/alpha-config".into(),
            no_data: false,
            host_owner_uid: Some(101000),
            app: Some("alpha".into()),
        },
        homelab_core::manifest::MountSpec {
            host_path: "/appdata/test/beta-config".into(),
            mount_point: "/appdata/test/beta-config".into(),
            no_data: false,
            host_owner_uid: Some(101000),
            app: Some("beta".into()),
        },
    ];
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test"));
    exec.respond_always("snapshots --json", CmdOutput::ok("[]"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup(&ctx(&exec, &sink, &j), &m, &BackupCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);

    let snaps = exec.calls_containing("restic backup");
    assert_eq!(snaps.len(), 2, "one snapshot per app, got: {:?}", snaps);
    assert!(
        snaps
            .iter()
            .any(|c| c.contains("/alpha-config ") || c.ends_with("/appdata/test/alpha-config")),
        "alpha must be snapshotted: {:?}",
        snaps
    );
    // Each app's paths go to its OWN repository.
    for app in ["alpha", "beta"] {
        let repo = format!("homelab-backups/{}-config", app);
        assert!(
            snaps.iter().any(|c| c.contains(&repo)),
            "expected a snapshot into {}, got: {:?}",
            repo,
            snaps
        );
    }
    // And the stack name is no longer a repository of its own.
    assert!(
        !snaps
            .iter()
            .any(|c| c.contains("homelab-backups/test-config")),
        "the stack-named repo should be gone: {:?}",
        snaps
    );
}

/// O5: `pct clone` takes no `--unprivileged`, so a clone always inherits the
/// template's privilege level. A manifest asking for a privileged container
/// while cloning an unprivileged template used to produce an unprivileged one
/// and report success — the file said one thing and the machine another, and
/// nothing looked wrong until an app failed on permissions with no
/// explanation. CT 105 and 106 are privileged and have to stay that way.
#[tokio::test]
async fn o5_clone_refuses_a_privilege_level_the_template_cannot_give() {
    use homelab_core::ops::deploy::deploy;
    let mut m = manifest(108, "test");
    m.lxc.template = "clone:999".into();
    m.lxc.unprivileged = false; // the stack wants privileged…
    for mount in &mut m.storage {
        mount.host_owner_uid = Some(1000); // privileged: no id mapping
    }
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    // …but the golden template it clones is unprivileged.
    exec.respond_always(
        "pct config 999",
        CmdOutput::ok("unprivileged: 1\nostype: debian"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(m)).await;
    assert!(
        !report.ok,
        "the mismatch must be refused, not cloned anyway"
    );
    let err = report.error.expect("an error");
    assert!(
        err.why.contains("unprivileged") && err.why.contains("999"),
        "the error must name the template and the mismatch: {}",
        err.why
    );
    assert!(!err.remedy.is_empty(), "an error must carry a remedy");
    assert!(
        exec.calls_containing("pct clone").is_empty(),
        "nothing should have been cloned"
    );
}

/// F38: the restore timeout was a hardcoded 1800 s while the backup side had
/// already been raised to four hours for exactly the same reason — a first
/// multi-GB transfer over a residential uplink. So a large restore from
/// Google Drive died at thirty minutes, on the one operation you least want
/// to discover is broken. It now comes from the configuration, and this test
/// fails if anyone pins it back to a constant.
#[tokio::test]
async fn f38_restore_honours_the_configured_timeout() {
    use homelab_core::ops::backup::{restore, BackupCfg};
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test"));
    let sink = VecSink::new();
    let j = NullJournal;
    let cfg = BackupCfg {
        restore_timeout_s: 9_999,
        ..Default::default()
    };
    let _ = restore(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        &cfg,
        "latest",
    )
    .await;
    let given = exec.timeouts_for("restic restore");
    assert!(!given.is_empty(), "no restore command was issued");
    assert!(
        given.iter().all(|t| *t == 9_999),
        "restore must use the configured timeout, got {:?}",
        given
    );
    // And the default is no longer the old half hour.
    assert_eq!(BackupCfg::default().restore_timeout_s, 4 * 3600);
}

/// O6: the auto-restore used to be all-or-nothing — it only ran when EVERY
/// path in `storage:` was empty. Wipe one app's config while its siblings are
/// intact and nothing was restored, and nothing said so, because from the
/// stack's point of view there was nothing wrong. The Ansible generation
/// checked each service directory separately; this restores that.
#[tokio::test]
async fn o6_restore_is_per_path_not_per_stack() {
    use homelab_core::ops::deploy::deploy;
    let mut m = manifest(108, "test");
    m.apps = vec!["alpha".into(), "beta".into()];
    m.storage = vec![
        homelab_core::manifest::MountSpec {
            host_path: "/appdata/test/alpha-config".into(),
            mount_point: "/appdata/test/alpha-config".into(),
            no_data: false,
            host_owner_uid: None,
            app: None,
        },
        homelab_core::manifest::MountSpec {
            host_path: "/appdata/test/beta-config".into(),
            mount_point: "/appdata/test/beta-config".into(),
            no_data: false,
            host_owner_uid: None,
            app: None,
        },
    ];
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    // alpha still holds data; beta was wiped.
    exec.respond_always("ls -A '/appdata/test/alpha-config'", CmdOutput::ok("db\n"));
    exec.respond_always("ls -A '/appdata/test/beta-config'", CmdOutput::ok(""));
    exec.respond_always(
        "snapshots --last --json",
        CmdOutput::ok(r#"[{"short_id":"abc"}]"#),
    );
    exec.respond_always("restic restore", CmdOutput::ok("restored"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(m)).await;
    assert!(report.ok, "{:?}", report.error);
    let restores = exec.calls_containing("restic restore");
    assert_eq!(
        restores.len(),
        1,
        "exactly the wiped path should be restored, got: {:?}",
        restores
    );
    assert!(
        restores[0].contains("--path /appdata/test/beta-config"),
        "the restore must target the wiped path only: {}",
        restores[0]
    );
    assert!(
        !restores[0].contains("alpha"),
        "the intact path must not be touched: {}",
        restores[0]
    );
}

#[tokio::test]
async fn e3_env_restored_from_vault_when_client_sends_none() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    exec.seed_file(
        "/var/lib/homelab/secrets/test/app.env",
        "API_KEY=from_the_vault\n",
    );
    let spec = deploy_spec(manifest(108, "test")); // env is empty
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    // The vault env was pushed to the container (via the staged tmp file).
    assert!(
        !exec
            .calls_containing("pct push 108 /var/lib/homelab/push-staging /opt/test/app/.env")
            .is_empty(),
        "vault fallback must push the .env"
    );
}

#[tokio::test]
async fn d3_removed_app_is_stopped_and_deleted_but_config_kept() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    // Previous state knows apps [app, oldapp]; the new spec only has [app].
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"test":{"vmid":108,"hostname":"108-app-test","apps":["app","oldapp"],"applied_at":1}}}"#,
    );
    let spec = deploy_spec(manifest(108, "test"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    let gc = exec.calls_containing("/opt/test/oldapp");
    assert!(
        gc.iter().any(|c| c.contains("docker compose down")),
        "removed app must be composed down"
    );
    assert!(
        gc.iter().any(|c| c.contains("rm -rf")),
        "and its /opt dir removed"
    );
    // Config dirs under /appdata are never touched by GC.
    assert!(!exec
        .calls()
        .iter()
        .any(|c| c.contains("rm") && c.contains("/appdata/")));
    // The surviving app was untouched by GC.
    assert!(!exec
        .calls_containing("rm -rf '/opt/test/app'")
        .iter()
        .any(|_| true));
}

#[tokio::test]
async fn h6_appdata_routes_dir_written_host_side() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    let mut spec = deploy_spec(manifest(108, "test"));
    spec.gateway_route = Some(homelab_core::manifest::GatewayRoute {
        gateway_vmid: 104,
        filename: "108-app-test.yml".into(),
        content: "http:\n  routers: {}\n".into(),
    });
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.safety.gateway_routes_dir = "/appdata/platform/traefik-config/routes".into();
    let report = deploy(&c, &spec).await;
    assert!(report.ok, "{:?}", report.error);
    // Written directly on the host, NOT pushed into the gateway container.
    assert!(exec
        .file("/appdata/platform/traefik-config/routes/108-app-test.yml")
        .is_some());
    assert!(exec.calls_containing("pct push 104").is_empty());
}

// ── H7 state fail-loud, H10 host-meta backup ────────────────────────────────

#[tokio::test]
async fn h7_corrupt_state_fails_loud_and_quarantines() {
    use homelab_core::state::StateStore;
    let exec = MockExecutor::new();
    exec.seed_file("/var/lib/homelab/state.json", "{ this is not json !!");
    let store = StateStore::new(&exec, "/var/lib/homelab");
    let err = store.load().await.unwrap_err();
    assert!(format!("{}", err).contains("does not parse"));
    // The corrupt content was preserved for forensics.
    assert!(exec.file("/var/lib/homelab/state.json.corrupt").is_some());
    // A deploy over corrupt state must fail, not silently erase the fleet.
    use homelab_core::ops::deploy::deploy;
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &deploy_spec(manifest(108, "test"))).await;
    assert!(!report.ok, "deploy must refuse to run over corrupt state");
}

#[tokio::test]
async fn h7_newer_schema_refused_missing_file_is_fresh() {
    use homelab_core::state::StateStore;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":99,"stacks":{}}"#,
    );
    let store = StateStore::new(&exec, "/var/lib/homelab");
    assert!(format!("{}", store.load().await.unwrap_err()).contains("newer"));
    let exec = MockExecutor::new();
    let store = StateStore::new(&exec, "/var/lib/homelab");
    assert!(
        store.load().await.unwrap().stacks.is_empty(),
        "missing file = fresh"
    );
}

#[tokio::test]
async fn h10_host_meta_backup_snapshots_vault_state_tls() {
    use homelab_core::ops::backup::backup_host_meta;
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup_host_meta(&ctx(&exec, &sink, &j), &BackupCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let snap = exec.calls_containing("restic backup");
    assert_eq!(snap.len(), 1);
    for path in ["/var/lib/homelab/secrets", "state.json", "tls-key.pem"] {
        assert!(
            snap[0].contains(path),
            "missing {} in host-meta snapshot",
            path
        );
    }
    assert!(snap[0].contains("host-meta"), "dedicated repo");
}

// ── H14: journal wiring actually asserted ───────────────────────────────────

use std::sync::Mutex as StdMutex;

struct RecJournal(StdMutex<Vec<(String, String, String)>>);
impl homelab_core::runner::Journal for RecJournal {
    fn record(&self, op: &str, step: &str, status: &str) {
        self.0
            .lock()
            .unwrap()
            .push((op.into(), step.into(), status.into()));
    }
}

#[tokio::test]
async fn h14_every_destroy_step_is_journaled_running_then_done() {
    let exec = MockExecutor::new();
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test\n"));
    let sink = VecSink::new();
    let j = RecJournal(StdMutex::new(Vec::new()));
    let report = destroy(
        &OpCtx {
            exec: &exec,
            sink: &sink,
            journal: &j,
            safety: SafetyConfig::default(),
            state_dir: "/var/lib/homelab".into(),
            now_unix: 1_760_000_000,
            kea: None,
            metrics_targets_dir: None,
            grafana_dashboards_dir: None,
            backup: Default::default(),
            registry_cache: None,
        },
        &manifest(108, "test"),
        "test",
        true,
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let records = j.0.lock().unwrap();
    assert!(!records.is_empty(), "destroy must journal its steps");
    // Every step appears as running BEFORE done, per step, in order.
    for pair in records.windows(2) {
        if pair[0].1 == pair[1].1 {
            assert_eq!(pair[0].2, "running");
            assert!(pair[1].2 == "done" || pair[1].2 == "failed");
        }
    }
    assert!(records
        .iter()
        .any(|r| r.1 == "destroy container" && r.2 == "done"));
}

#[tokio::test]
async fn h14_failed_step_leaves_running_then_failed_trail() {
    let exec = MockExecutor::new();
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test\n"));
    exec.respond_always("pct destroy", CmdOutput::failed(1, "device busy"));
    let sink = VecSink::new();
    let j = RecJournal(StdMutex::new(Vec::new()));
    let report = destroy(
        &OpCtx {
            exec: &exec,
            sink: &sink,
            journal: &j,
            safety: SafetyConfig::default(),
            state_dir: "/var/lib/homelab".into(),
            now_unix: 1_760_000_000,
            kea: None,
            metrics_targets_dir: None,
            grafana_dashboards_dir: None,
            backup: Default::default(),
            registry_cache: None,
        },
        &manifest(108, "test"),
        "test",
        true,
    )
    .await;
    assert!(!report.ok);
    let records = j.0.lock().unwrap();
    let destroy_records: Vec<_> = records
        .iter()
        .filter(|r| r.1 == "destroy container")
        .collect();
    assert_eq!(destroy_records.len(), 2);
    assert_eq!(destroy_records[0].2, "running");
    assert_eq!(destroy_records[1].2, "failed");
    // AR13 parses exactly this trail after a crash: running without done.
}

// ── H19 residue: golden argv, guards positive path, route escape, kea set ───

#[tokio::test]
async fn h19_golden_pct_create_argv() {
    use homelab_core::ops::deploy::deploy;
    let exec = MockExecutor::new();
    deploy_mocks(&exec);
    exec.respond_always("ls -A", CmdOutput::ok("config\n"));
    let mut m = manifest(108, "test");
    m.lxc.template = "local:vztmpl/debian-12.tar.zst".into(); // full-create path
    let spec = deploy_spec(m);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    let create = exec
        .calls()
        .into_iter()
        .find(|c| c.contains("pct create"))
        .expect("create ran");
    // The EXACT argument vector — a wrong flag here provisions wrongly.
    assert_eq!(
        create,
        "pct create 108 local:vztmpl/debian-12.tar.zst --hostname 108-app-test \
--rootfs local-lvm:4 --net0 name=eth0,bridge=vmbr0,firewall=0,ip=10.10.10.8/24,gw=10.10.10.1,tag=10 \
--memory 512 --swap 256 --cores 1 --unprivileged 1 --features nesting=1 --onboot 1 \
--description managed by homelab v2 :: stack test --tags homelab --timezone host \
--startup order=50",
        "golden pct create argv changed — verify deliberately and update"
    );
}

#[tokio::test]
async fn h19_guards_positive_branch_writes_and_restarts_once() {
    let exec = MockExecutor::new();
    // Everything differs from desired (sha mismatch: default empty response).
    let sink = VecSink::new();
    homelab_core::ops::guards::apply(&exec, &sink, 108, true, None)
        .await
        .unwrap();
    // daemon.json written + docker restarted exactly once.
    assert_eq!(exec.calls_containing("systemctl restart docker").len(), 1);
    assert!(
        !exec.calls_containing("pct push").is_empty(),
        "guard files pushed"
    );
}

#[test]
fn h19_gateway_route_filename_escape_refused() {
    use homelab_core::safety::{check_gateway_route, SafetyConfig};
    let cfg = SafetyConfig::default();
    assert!(check_gateway_route(&cfg, 104, "ok-route.yml").is_ok());
    for evil in ["../etc/passwd.yml", "a/b.yml", "route.sh", ""] {
        assert!(
            check_gateway_route(&cfg, 104, evil).is_err(),
            "must refuse: {}",
            evil
        );
    }
    // And only the configured gateway vmid is a valid destination.
    assert!(check_gateway_route(&cfg, 105, "ok.yml").is_err());
}

#[tokio::test]
async fn h19_kea_updates_existing_reservation_via_set_branch() {
    use homelab_core::ops::kea::{reserve, KeaCfg};
    let exec = MockExecutor::new();
    exec.respond_always(
        "search_subnet",
        CmdOutput::ok(r#"{"rows":[{"uuid":"sub-1","subnet":"10.10.10.0/24"}]}"#),
    );
    // An EXISTING reservation for this ip → set_reservation, not add.
    exec.respond_always(
        "search_reservation",
        CmdOutput::ok(r#"{"rows":[{"uuid":"res-9","ip_address":"10.10.10.8"}]}"#),
    );
    exec.respond_always("set_reservation", CmdOutput::ok(r#"{"result":"saved"}"#));
    exec.respond_always("reconfigure", CmdOutput::ok(r#"{"status":"ok"}"#));
    let cfg = KeaCfg {
        base_url: "https://10.10.10.1".into(),
        cred_file: "/var/lib/homelab/secrets/opnsense".into(),
    };
    reserve(
        &exec,
        &cfg,
        "10.10.10.8",
        "BC:24:11:AA:BB:CC",
        "108-app-test",
    )
    .await
    .unwrap();
    assert_eq!(exec.calls_containing("set_reservation/res-9").len(), 1);
    assert!(exec.calls_containing("add_reservation").is_empty());
}

// ── H8 (light): enabled flag ────────────────────────────────────────────────

#[tokio::test]
async fn b8_disable_parks_stack_and_clears_onboot() {
    use homelab_core::ops::enable::set_enabled;
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"test":{"vmid":108,"hostname":"108-app-test","apps":["app"],"applied_at":1}}}"#,
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = set_enabled(&ctx(&exec, &sink, &j), "test", false).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        !exec.calls_containing("pct set 108 --onboot 0").is_empty(),
        "disable must clear onboot so parking survives a host reboot"
    );
    let state = exec.file("/var/lib/homelab/state.json").unwrap();
    assert!(state.contains("\"enabled\": false"), "{}", state);
}

#[tokio::test]
async fn b8_enable_restores_onboot_and_flag() {
    use homelab_core::ops::enable::set_enabled;
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"test":{"vmid":108,"hostname":"108-app-test","apps":["app"],"applied_at":1,"enabled":false}}}"#,
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = set_enabled(&ctx(&exec, &sink, &j), "test", true).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        !exec.calls_containing("pct set 108 --onboot 1").is_empty(),
        "enable must restore onboot (manifest default when none stored)"
    );
    let state = exec.file("/var/lib/homelab/state.json").unwrap();
    assert!(state.contains("\"enabled\": true"), "{}", state);
}

#[tokio::test]
async fn b8_no_touch_vmid_refused() {
    use homelab_core::ops::enable::set_enabled;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"evil":{"vmid":104,"hostname":"104-app-evil","apps":["app"],"applied_at":1}}}"#,
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = set_enabled(&ctx(&exec, &sink, &j), "evil", false).await;
    assert!(!report.ok, "no-touch vmid must be refused");
    assert!(
        exec.calls_containing("pct set 104").is_empty(),
        "no pct mutation may reach a no-touch vmid"
    );
}

#[tokio::test]
async fn b8_old_state_json_defaults_to_enabled() {
    use homelab_core::state::StateStore;
    let exec = MockExecutor::new();
    // Pre-B8 state file: no `enabled` key anywhere.
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"test":{"vmid":108,"hostname":"108-app-test","apps":["app"],"applied_at":1}}}"#,
    );
    let store = StateStore::new(&exec, "/var/lib/homelab");
    let state = store.load().await.unwrap();
    assert!(
        state.stacks["test"].enabled,
        "stacks from before the flag existed must stay in the nightly rotation"
    );
}

// ── Protection vs drive changes ─────────────────────────────────────────────

/// Live-found on the first protected stack with a bind mount (metrics,
/// 2026-08-29): Proxmox refuses drive changes ("can't update CT 113 drive
/// 'mp0' - protection mode enabled") once the protection flag is set, so
/// protection must be the LAST provisioning act — after resize and every
/// mountpoint — on both the clone and the create path.
#[tokio::test]
async fn protection_is_set_after_all_drive_changes() {
    use homelab_core::manifest::{DeploySpec, FileBlob};
    use homelab_core::ops::deploy::deploy;
    for template in ["clone:999", "debian-12"] {
        let exec = MockExecutor::new();
        deploy_mocks(&exec);
        exec.respond_always("ls -A", CmdOutput::ok("config\n"));
        let mut m = manifest(113, "prot");
        m.lxc.template = template.into();
        m.lxc.protection = true;
        m.resources.disk_gb = 8;
        let spec = DeploySpec {
            manifest: m,
            files: vec![FileBlob {
                path: "app/docker-compose.yml".into(),
                content: "services: {}".into(),
                mode: None,
            }],
            env: Default::default(),
            gateway_route: None,
            checks: Default::default(),
        };
        let sink = VecSink::new();
        let j = NullJournal;
        let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
        assert!(report.ok, "{} :: {:?}", template, report.error);
        let calls = exec.calls();
        let protect_idx = calls
            .iter()
            .position(|c| c.contains("--protection 1"))
            .unwrap_or_else(|| panic!("{}: protection never set", template));
        let mp_idx = calls
            .iter()
            .rposition(|c| c.contains("-mp0"))
            .unwrap_or_else(|| panic!("{}: mount never set", template));
        assert!(
            protect_idx > mp_idx,
            "{}: protection (call {}) must come after the last mount (call {})",
            template,
            protect_idx,
            mp_idx
        );
        // Only the clone path resizes after the fact; create sizes the
        // rootfs in `pct create` itself.
        if template.starts_with("clone:") {
            let resize_idx = calls
                .iter()
                .rposition(|c| c.contains("pct resize"))
                .unwrap_or_else(|| panic!("{}: resize never ran", template));
            assert!(
                protect_idx > resize_idx,
                "{}: protection (call {}) must come after resize (call {})",
                template,
                protect_idx,
                resize_idx
            );
        }
    }
}

// ── B1/B2: the backup that a destroy takes first ───────────────────────────

/// Kenny asked whether a destroy takes a backup first. It did not — the
/// procedure said to, and nothing enforced it, so it existed only while
/// whoever ran it remembered. A habit is not a safety net.
#[tokio::test]
async fn a_destroy_backs_the_stack_up_before_it_removes_anything() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        "test",
        false,
    )
    .await;
    assert!(report.ok, "{:?}", report.error);

    let calls = exec.calls();
    let backup = calls
        .iter()
        .position(|c| c.contains("restic backup"))
        .expect("a backup runs");
    let gone = calls
        .iter()
        .position(|c| c.contains("pct destroy"))
        .expect("and then it destroys");
    assert!(
        backup < gone,
        "the backup comes FIRST: {} vs {}",
        backup,
        gone
    );
}

/// And when that backup fails, nothing is destroyed. A backup you may skip
/// silently is not a backup.
#[tokio::test]
async fn a_failed_backup_stops_the_destroy() {
    let exec = MockExecutor::new();
    mock_hostname(&exec, 108, "test");
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test\n"));
    exec.respond_always(
        "restic backup",
        CmdOutput::failed(1, "repository unreachable"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = destroy(
        &ctx(&exec, &sink, &j),
        &manifest(108, "test"),
        "test",
        false,
    )
    .await;
    assert!(!report.ok, "a failed backup must stop the destroy");
    let why = report.error.unwrap().why;
    assert!(
        why.contains("--no-backup"),
        "the escape must be named: {}",
        why
    );
    assert!(
        exec.calls_containing("pct destroy").is_empty(),
        "nothing may be removed"
    );

    // The escape works, and it is the operator's decision rather than a retry.
    let exec2 = MockExecutor::new();
    mock_hostname(&exec2, 108, "test");
    exec2.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test\n"));
    exec2.respond_always(
        "restic backup",
        CmdOutput::failed(1, "repository unreachable"),
    );
    let sink2 = VecSink::new();
    let j2 = NullJournal;
    let report = destroy(
        &ctx(&exec2, &sink2, &j2),
        &manifest(108, "test"),
        "test",
        true,
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec2.calls_containing("restic backup").is_empty());
    assert_eq!(exec2.calls_containing("pct destroy 108 --purge").len(), 1);
}

/// T66 · whatever a deploy registers, a destroy has to unregister.
///
/// The dashboard was the half that was missing. A destroyed stack left its
/// Grafana panel behind, showing a container that no longer exists — which
/// reads as "everything is down" rather than "this is gone", and nothing
/// distinguishes the two. The Prometheus target and the Traefik route were
/// already removed; only this one was not.
#[tokio::test]
async fn t66_destroy_removes_the_dashboard_the_deploy_wrote() {
    let exec = MockExecutor::new();
    exec.respond_always("pct config", CmdOutput::ok("hostname: 108-app-test\n"));
    exec.respond_always("pct status", CmdOutput::ok("status: stopped"));
    let sink = VecSink::new();
    let j = NullJournal;
    let mut c = ctx(&exec, &sink, &j);
    c.grafana_dashboards_dir = Some("/opt/grafana/provisioning/dashboards".into());
    c.metrics_targets_dir = Some("/opt/prometheus/targets".into());

    let report = destroy(&c, &manifest(108, "test"), "test", true).await;
    assert!(report.ok, "destroy failed: {:?}", report.error);

    let removed = exec.calls_containing("homelab-test.json");
    assert_eq!(
        removed.len(),
        1,
        "the dashboard this stack's deploy wrote must be removed exactly once: {:?}",
        removed
    );
    assert!(
        removed[0].contains("rm"),
        "and removed, not merely mentioned: {}",
        removed[0]
    );
    // The two that already worked must keep working.
    assert_eq!(
        exec.calls_containing("test.json")
            .iter()
            .filter(|c| c.contains("/opt/prometheus/targets"))
            .count(),
        1,
        "the Prometheus target is still removed"
    );
}
